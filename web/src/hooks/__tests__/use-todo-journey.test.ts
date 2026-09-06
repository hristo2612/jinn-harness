import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TodoJourney } from '../use-todo-journey'
import { mutateTodo, readTodo, readTask, stopTask, TodoHttpError, type TodoRecord } from '@/lib/api-todo-journey'
vi.mock('@/lib/api-todo-journey', async () => {
  const actual = await vi.importActual<typeof import('@/lib/api-todo-journey')>('@/lib/api-todo-journey')
  return { ...actual, readTodo: vi.fn(), readTask: vi.fn(), mutateTodo: vi.fn(), stopTask: vi.fn() }
})
const record = (revision = 1): TodoRecord => ({ 'todo-id': 'work-1', store: 'work', title: 'Draft', body: '', acceptance: '', status: 'executing', 'declared-status': 'executing', 'created-ms': 0, revision, comments: [], dispatches: [], history: [], refused: [] })
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (cause: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
const flush = async () => { for (let i = 0; i < 10; i++) await Promise.resolve() }
let cleanup: (() => void) | undefined
beforeEach(() => { vi.useFakeTimers(); vi.resetAllMocks(); sessionStorage.clear(); vi.mocked(readTodo).mockResolvedValue(record()) })
afterEach(() => { cleanup?.(); cleanup = undefined; vi.useRealTimers() })
function start() { const journey = new TodoJourney('work-1'); cleanup = journey.connect(); return journey }
describe('Todo snapshot and mutation ownership', () => {
  it('serializes reads and fences a response begun before an acknowledged mutation', async () => {
    const journey = start(); await flush()
    const late = deferred<TodoRecord>(); vi.mocked(readTodo).mockReturnValueOnce(late.promise)
    await vi.advanceTimersByTimeAsync(2000)
    journey.retry(); journey.retry(); expect(readTodo).toHaveBeenCalledTimes(2)
    vi.mocked(mutateTodo).mockResolvedValue(record(2))
    expect(await journey.mutate('comments', { body: 'context' }, 1)).toBe(true)
    late.resolve(record(1)); await flush()
    expect(journey.snapshot().record?.revision).toBe(2)
    expect(journey.snapshot().connected).toBe(true)
    expect(await journey.mutate('status', { status: 'done' }, 1)).toBe(false)
    expect(mutateTodo).toHaveBeenCalledTimes(1)
  })
  it('holds ambiguous writes across remount and clears only a witnessed client marker', async () => {
    const journey = start(); await flush()
    const write = deferred<TodoRecord>(); vi.mocked(mutateTodo).mockReturnValueOnce(write.promise)
    const first = journey.mutate('comments', { body: 'context' }, 1)
    expect(await journey.mutate('comments', { body: 'context' }, 1)).toBe(false)
    write.reject(new Error('Connection lost')); expect(await first).toBe(false)
    const client = vi.mocked(mutateTodo).mock.calls[0][3]
    expect(journey.snapshot().pending?.client).toBe(client)
    cleanup?.(); const resumed = start(); await flush()
    expect(resumed.snapshot().pending?.client).toBe(client)
    expect(await resumed.mutate('dispatch', {}, 1)).toBe(false)
    const saved = { ...record(2), comments: [{ 'comment-id': 'c1', seq: 0, body: 'context', 'client-request': client }] }
    vi.mocked(readTodo).mockResolvedValue(saved); resumed.retry(); await flush()
    expect(resumed.snapshot().pending).toBeNull()
    expect(mutateTodo).toHaveBeenCalledTimes(1)
  })
  it('does not automatically repeat a typed refusal or an ambiguous absent write', async () => {
    const journey = start(); await flush()
    vi.mocked(mutateTodo).mockRejectedValueOnce(new TodoHttpError('stale', true))
    expect(await journey.mutate('comments', {}, 1)).toBe(false)
    expect(journey.snapshot().pending).toBeNull()
    vi.mocked(mutateTodo).mockRejectedValueOnce(new Error('append outcome unknown'))
    expect(await journey.mutate('dispatch', {}, 1)).toBe(false)
    await vi.advanceTimersByTimeAsync(5000)
    expect(journey.snapshot().pending).not.toBeNull()
    expect(mutateTodo).toHaveBeenCalledTimes(2)
  })
  it('Stop waits for the linked dispatch ending and never mutates Todo status', async () => {
    const running = { ...record(), dispatches: [{ 'dispatch-id': 'd1', 'session-id': 's1', 'session-store': 'tasks', 'turn-id': 't1', engine: 'codex', status: 'running' as const, answer: '' }] }
    vi.mocked(readTodo).mockResolvedValue(running)
    vi.mocked(readTask).mockResolvedValue({ 'turn-id': 't1', seq: 0, message: 'task', answer: 'prefix', status: 'running' })
    vi.mocked(stopTask).mockResolvedValue(undefined)
    const journey = start(); await flush(); await journey.stop()
    expect(journey.snapshot().pending?.action).toBe('stop')
    expect(journey.snapshot().record?.status).toBe('executing')
    vi.mocked(readTodo).mockResolvedValue({ ...running, revision: 2, dispatches: [{ ...running.dispatches[0], status: 'cancelled' }] })
    journey.retry(); await vi.advanceTimersByTimeAsync(500)
    expect(journey.snapshot().pending).toBeNull()
    expect(stopTask).toHaveBeenCalledWith(running.dispatches[0]); expect(mutateTodo).not.toHaveBeenCalled()
  })
  it('cannot issue a write when pending state cannot be retained', async () => {
    const journey = start(); await flush()
    const storage = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('storage denied') })
    expect(await journey.mutate('dispatch', {}, 1)).toBe(false)
    expect(mutateTodo).not.toHaveBeenCalled()
    storage.mockRestore()
  })
})
