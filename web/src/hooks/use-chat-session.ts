import { useEffect, useMemo, useSyncExternalStore } from 'react'
import { chatEvents, listChats, postMessage, prepareMessage, readChat, stopChat, type ChatSnapshot, type ChatTurn } from '@/lib/api-chat'
import { applyChatEvents } from '@/lib/chat-events'

export type ChatFeedState = 'connecting' | 'live' | 'recovering'
interface View { record: ChatSnapshot | null; pending: ChatTurn | null; busy: boolean; error: string | null; feed: ChatFeedState }
const messageOf = (error: unknown) => error instanceof Error ? error.message : 'The chat service could not be reached.'

/** One selected conversation owns its reads and writes. It never retries a POST. */
class TextChatSession {
  private view: View = { record: null, pending: null, busy: false, error: null, feed: 'connecting' }
  private listeners = new Set<() => void>()
  private controller = new AbortController()
  private generation = 0
  private reconcile = true
  private lastSnapshot = 0
  private failures = 0
  private timer: ReturnType<typeof setTimeout> | undefined
  private wake: (() => void) | undefined
  constructor(private id: string | null) {}
  snapshot = () => this.view
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener) } }
  private update(patch: Partial<View>) { this.view = { ...this.view, ...patch }; this.listeners.forEach(listener => listener()) }
  private valid(stamp: number) { return !this.controller.signal.aborted && stamp === this.generation }
  private delay(ms: number) { return new Promise<void>(resolve => { this.wake = resolve; this.timer = setTimeout(resolve, ms) }) }
  retry = () => { this.reconcile = true; clearTimeout(this.timer); this.wake?.() }

  connect() {
    this.controller = new AbortController()
    document.addEventListener('visibilitychange', this.retry)
    window.addEventListener('pageshow', this.retry)
    void this.run()
    return () => {
      this.controller.abort(); this.generation += 1
      clearTimeout(this.timer); this.wake?.()
      document.removeEventListener('visibilitychange', this.retry)
      window.removeEventListener('pageshow', this.retry)
    }
  }

  private acceptSnapshot(saved: ChatSnapshot) {
    const pending = this.view.pending
    this.update({ record: saved })
    if (!pending) return
    const found = saved.log.some(turn => turn.seq === pending.seq && turn.message === pending.message)
    this.update({ pending: null, error: found ? null : 'No saved turn was found. Your draft is kept; you can send it again.' })
  }

  private snapshotDue() { return this.reconcile || !this.view.record || Date.now() - this.lastSnapshot >= 5000 }

  private async poll(stamp: number): Promise<number> {
    if (!this.id) return 1000
    if (this.snapshotDue()) {
      const saved = await readChat(this.id, this.controller.signal)
      if (!this.valid(stamp)) return 0
      this.acceptSnapshot(saved)
      this.reconcile = false; this.lastSnapshot = Date.now()
    }
    const before = this.view.record
    if (!before) return 250
    const page = await chatEvents(this.id, before['event-after'], this.controller.signal)
    if (!this.valid(stamp)) return 0
    const next = applyChatEvents(before, page, before['event-after'])
    this.update({ record: next.snapshot, feed: 'live', ...(this.failures > 0 ? { error: null } : {}) })
    this.failures = 0
    if (next.terminal) { this.reconcile = true; return 0 }
    if (next.count === 50) return 0
    return before.log.some(turn => turn.status === 'running') ? 250 : 1000
  }

  private async run() {
    if (!this.id) return
    const signal = this.controller.signal
    while (!signal.aborted) {
      if (this.view.busy || document.visibilityState === 'hidden') { await this.delay(250); continue }
      const stamp = this.generation
      try {
        const delay = await this.poll(stamp)
        if (delay > 0 && !signal.aborted) await this.delay(delay)
      } catch (cause) {
        if (!this.valid(stamp)) continue
        this.reconcile = true; this.failures += 1
        this.update({ feed: 'recovering', error: `Chat updates paused: ${messageOf(cause)} Checking saved history.` })
        await this.delay(Math.min(5000, 250 * 2 ** Math.min(this.failures, 5)))
      }
    }
  }

  private canSend() {
    return this.id && !this.view.busy && !this.view.pending && this.view.record && !this.view.record.log.some(turn => turn.status === 'running')
  }

  private async recoverSend(stamp: number, cause: unknown): Promise<boolean> {
    if (!this.valid(stamp) || !this.id) return false
    const pending = this.view.pending
    if (!pending) { this.update({ error: messageOf(cause) }); return false }
    try {
      const saved = await readChat(this.id)
      if (!this.valid(stamp)) return false
      const found = saved.log.some(turn => turn.seq === pending.seq && turn.message === pending.message)
      this.update({ record: saved, pending: null, error: found ? null : messageOf(cause) })
      return found
    } catch {
      this.update({ error: 'Send not confirmed. Your draft is kept. Check saved history before sending again.' })
      return false
    }
  }

  send = async (draft: string): Promise<boolean> => {
    if (!this.canSend() || !this.id) return false
    this.update({ busy: true, error: null })
    const stamp = ++this.generation
    try {
      const chats = await listChats()
      if (chats.some(chat => chat['session-id'] !== this.id && chat.status === 'running')) throw new Error('A reply is running in another conversation. Open it to wait or stop it first.')
      const text = await prepareMessage(this.id, draft)
      if (!this.valid(stamp) || !this.view.record) return false
      this.update({ pending: { 'turn-id': 'pending', seq: this.view.record.turns, message: text, answer: '', status: 'running' } })
      await postMessage(this.id, text)
      const saved = await readChat(this.id)
      if (!this.valid(stamp)) return false
      this.update({ record: saved, pending: null })
      return true
    } catch (cause) { return await this.recoverSend(stamp, cause) }
    finally { this.update({ busy: false }); this.retry() }
  }

  stop = async () => {
    if (!this.id || this.view.busy) return
    this.update({ busy: true })
    const stamp = ++this.generation
    try {
      const saved = await stopChat(this.id)
      if (this.valid(stamp)) this.update({ record: saved, error: null })
    } catch (cause) {
      if (this.valid(stamp)) this.update({ error: `Stop unconfirmed: ${messageOf(cause)} The reply may still be running.` })
    } finally { this.update({ busy: false }); this.retry() }
  }
}

export function useChatSession(id: string | null) {
  const session = useMemo(() => new TextChatSession(id), [id])
  useEffect(() => session.connect(), [session])
  const state = useSyncExternalStore(session.subscribe, session.snapshot)
  return { ...state, send: session.send, stop: session.stop, retry: session.retry }
}
