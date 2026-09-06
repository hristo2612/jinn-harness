import { useEffect, useMemo, useSyncExternalStore } from 'react'
import { mutateTodo, readTask, readTodo, stopTask, TodoHttpError, witnessed, type TodoRecord } from '@/lib/api-todo-journey'
import type { ChatTurn } from '@/lib/api-chat'

interface Pending { client: string; action: string; dispatch?: string }
interface View { record: TodoRecord | null; turn: ChatTurn | null; busy: boolean; error: string | null; pending: Pending | null; connected: boolean }
const message = (cause: unknown) => cause instanceof Error ? cause.message : 'The Todo service could not be reached.'
function pendingKey(id: string) { return `todo-pending:${id}` }
function heldPending(id: string): Pending | null {
  try { return JSON.parse(sessionStorage.getItem(pendingKey(id)) ?? 'null') as Pending | null }
  catch { return null }
}

/** Serial snapshots, fenced by mutation/connection generation. No POST is retried. */
export class TodoJourney {
  private view: View
  private listeners = new Set<() => void>()
  private controller = new AbortController()
  private generation = 0
  private writeError: string | null = null
  private timer: ReturnType<typeof setTimeout> | undefined
  private wake: (() => void) | undefined
  constructor(private id: string) { this.view = { record: null, turn: null, busy: false, error: null, pending: heldPending(id), connected: false } }
  snapshot = () => this.view
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener) } }
  private update(patch: Partial<View>) { this.view = { ...this.view, ...patch }; this.listeners.forEach(listener => listener()) }
  private valid(stamp: number) { return !this.controller.signal.aborted && stamp === this.generation }
  private delay(ms: number) { return new Promise<void>(resolve => { this.wake = resolve; this.timer = setTimeout(resolve, ms) }) }
  retry = () => { clearTimeout(this.timer); this.wake?.() }
  connect() {
    this.controller = new AbortController(); this.generation += 1
    document.addEventListener('visibilitychange', this.retry)
    window.addEventListener('pageshow', this.retry)
    void this.run(this.controller.signal)
    return () => {
      this.controller.abort(); this.generation += 1; this.retry()
      document.removeEventListener('visibilitychange', this.retry)
      window.removeEventListener('pageshow', this.retry)
    }
  }
  private hold(pending: Pending | null) {
    if (pending) sessionStorage.setItem(pendingKey(this.id), JSON.stringify(pending))
    else sessionStorage.removeItem(pendingKey(this.id))
    this.update({ pending })
  }
  private reconcile(record: TodoRecord) {
    const pending = this.view.pending
    if (!pending) return
    const stopped = pending.action === 'stop' && record.dispatches.some(item => item['dispatch-id'] === pending.dispatch && item.status !== 'running')
    if (stopped || witnessed(record, pending.client)) {
      this.hold(null)
      this.writeError = record.refused.some(row => row['client-request'] === pending.client) ? 'The action was refused. See the recorded move in History.' : null
      this.update({ error: this.writeError })
    }
  }
  private async poll(stamp: number, signal: AbortSignal) {
    const record = await readTodo(this.id, signal)
    if (!this.valid(stamp)) return
    if (this.view.record && record.revision < this.view.record.revision) throw new Error('An older Todo snapshot arrived; inspect after reconnecting.')
    this.update({ record, connected: true, error: this.writeError })
    this.reconcile(record)
    const latest = record.dispatches.at(-1)
    if (latest?.status !== 'running') { this.update({ turn: null }); return }
    if (!latest['session-id']) { this.update({ turn: null }); return }
    const turn = await readTask(latest, signal)
    if (this.valid(stamp)) this.update({ turn: turn ?? null })
  }
  private async run(signal: AbortSignal) {
    while (!signal.aborted) {
      if (this.view.busy || document.visibilityState === 'hidden') { await this.delay(250); continue }
      const stamp = this.generation
      try { await this.poll(stamp, signal) }
      catch (cause) { if (this.valid(stamp)) this.update({ connected: false, error: message(cause) }) }
      if (!signal.aborted) await this.delay(this.view.record?.dispatches.at(-1)?.status === 'running' ? 500 : 2000)
    }
  }
  private prepare(action: string): { record: TodoRecord; stamp: number; client: string } | null {
    if (this.view.busy || this.view.pending || !this.view.connected || !this.view.record) return null
    const record = this.view.record
    const client = crypto.randomUUID()
    try { this.hold({ action, client, dispatch: record.dispatches.at(-1)?.['dispatch-id'] }) }
    catch { this.update({ error: 'This tab cannot retain a pending write. Enable session storage before making changes.' }); return null }
    this.generation += 1; this.writeError = null; this.update({ busy: true, error: null })
    return { record, stamp: this.generation, client }
  }
  private failed(cause: unknown) {
    if (cause instanceof TodoHttpError && cause.rejected) this.hold(null)
    this.writeError = message(cause); this.update({ error: this.writeError })
  }
  mutate = async (action: string, body: object, expected: number): Promise<boolean> => {
    if (this.view.record?.revision !== expected) { this.update({ error: 'This Todo changed. Inspect the current record before trying again.' }); return false }
    const prepared = this.prepare(action)
    if (!prepared) return false
    const { record, stamp, client } = prepared
    try {
      const saved = await mutateTodo(record, action, body, client)
      if (!this.valid(stamp)) return false
      this.update({ record: saved }); this.hold(null)
      return true
    } catch (cause) { if (this.valid(stamp)) this.failed(cause); return false }
    finally { if (this.valid(stamp)) { this.update({ busy: false }); this.retry() } }
  }
  stop = async (): Promise<void> => {
    const prepared = this.prepare('stop')
    if (!prepared) return
    const latest = prepared.record.dispatches.at(-1)
    try {
      if (!latest || latest.status !== 'running') throw new TodoHttpError('No linked task is running.', true)
      await stopTask(latest)
      // The DELETE acknowledgement is not the Todo's terminal result. Poll it.
    } catch (cause) { if (this.valid(prepared.stamp)) this.failed(cause) }
    finally { if (this.valid(prepared.stamp)) { this.update({ busy: false }); this.retry() } }
  }
}

export function useTodoJourney(id: string) {
  const journey = useMemo(() => new TodoJourney(id), [id])
  useEffect(() => journey.connect(), [journey])
  const view = useSyncExternalStore(journey.subscribe, journey.snapshot, journey.snapshot)
  return { ...view, mutate: journey.mutate, stop: journey.stop, retry: journey.retry }
}
