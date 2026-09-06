import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TodoDetail } from './detail'
import { CaptureTodo } from './capture'
import { useTodoJourney } from '@/hooks/use-todo-journey'
import { createTodo } from '@/lib/api-todo-journey'
vi.mock('@/hooks/use-todo-journey', () => ({ useTodoJourney: vi.fn() }))
vi.mock('@/lib/api-todo-journey', async () => ({ ...await vi.importActual('@/lib/api-todo-journey'), createTodo: vi.fn(), listTodos: vi.fn() }))
const result = { 'dispatch-id': 'd1', 'session-store': 'tasks', engine: 'codex', status: 'done' as const, answer: 'A useful answer' }
const saved = { 'todo-id': 'work-1', store: 'work' as const, revision: 4, title: 'Draft an update', body: '', acceptance: 'Keep it short', status: 'executing' as const, 'declared-status': 'executing' as const, 'created-ms': 0, comments: [], history: [], refused: [], dispatches: [result] }
const journey = () => ({ record: saved, turn: null, error: null, connected: true, pending: null, busy: false, mutate: vi.fn().mockResolvedValue(true), stop: vi.fn(), retry: vi.fn() })
beforeEach(() => { sessionStorage.clear(); vi.resetAllMocks() })
describe('explicit operator result review', () => {
  it('requires an inspection for each current revision, including acceptance after submission', () => {
    const current = journey(); vi.mocked(useTodoJourney).mockReturnValue(current)
    const { rerender } = render(<TodoDetail id="work-1" onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Submit for review' }).hasAttribute('disabled')).toBe(true)
    fireEvent.click(screen.getByRole('button', { name: 'I’ve inspected this result' }))
    expect(screen.getByRole('button', { name: 'Submit for review' }).hasAttribute('disabled')).toBe(false)
    vi.mocked(useTodoJourney).mockReturnValue({ ...current, record: { ...saved, revision: 5 } })
    rerender(<TodoDetail id="work-1" onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Submit for review' }).hasAttribute('disabled')).toBe(true)
    vi.mocked(useTodoJourney).mockReturnValue({ ...current, record: { ...saved, revision: 6, status: 'in-review', 'declared-status': 'in-review' } })
    rerender(<TodoDetail id="work-1" onBack={() => {}} />)
    expect(screen.getByRole('button', { name: 'Accept and close' }).hasAttribute('disabled')).toBe(true)
    expect(screen.getByRole('button', { name: 'Needs work' }).hasAttribute('disabled')).toBe(true)
    fireEvent.click(screen.getByRole('button', { name: 'I’ve inspected this result' }))
    fireEvent.click(screen.getByRole('button', { name: 'Accept and close' }))
    expect(current.mutate).toHaveBeenCalledWith('status', { status: 'done', note: '', 'reviewed-dispatch': 'd1' }, 6)
  })
  it('does not claim a worker is running while its terminal result awaits durable recording', () => {
    const current = journey()
    vi.mocked(useTodoJourney).mockReturnValue({ ...current, record: { ...saved, dispatches: [{ ...result, status: 'running', 'session-id': 's1' }] }, turn: { 'turn-id': 't1', status: 'done', answer: 'Finished', message: 'Do work' } as NonNullable<ReturnType<typeof useTodoJourney>['turn']> })
    render(<TodoDetail id="work-1" onBack={() => {}} />)
    expect(screen.getByText('The session has ended. Waiting for the Todo’s saved outcome.')).not.toBeNull()
    expect(screen.queryByText('Task running. Model completion will still need your review.')).toBeNull()
  })
  it('renders a stopped task separately from a cancelled Todo and discloses restart limits', () => {
    const current = journey(); vi.mocked(useTodoJourney).mockReturnValue({ ...current, record: { ...saved, status: 'blocked', dispatches: [{ ...result, status: 'interrupted', answer: '' }] } })
    render(<TodoDetail id="work-1" onBack={() => {}} />)
    expect(screen.getByText(/Its live session link and partial answer may be missing/)).not.toBeNull()
    expect(screen.queryByRole('button', { name: 'Stop task' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Accept and close' })).toBeNull()
  })
})
describe('capture keeps newer drafts', () => {
  it('does not erase text typed while an earlier capture acknowledgement is in flight', async () => {
    let finish!: (id: string) => void
    vi.mocked(createTodo).mockReturnValue(new Promise(resolve => { finish = resolve }))
    const created = vi.fn()
    render(<CaptureTodo onCreated={created} />)
    fireEvent.change(screen.getByLabelText('Title'), { target: { value: 'Original task' } })
    fireEvent.click(screen.getByRole('button', { name: 'Capture Todo' }))
    fireEvent.change(screen.getByLabelText('Title'), { target: { value: 'A newer draft' } })
    await act(async () => { finish('work-1'); await Promise.resolve() })
    await waitFor(() => expect(created).toHaveBeenCalledWith('work-1'))
    expect(JSON.parse(sessionStorage.getItem('todo-capture-draft') ?? '{}').title).toBe('A newer draft')
    expect(createTodo).toHaveBeenCalledTimes(1)
  })
})
