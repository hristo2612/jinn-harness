import { CHAT_STORE, integer, object, type ChatSnapshot, type ChatTurn } from './api-chat'

export interface FeedResult { snapshot: ChatSnapshot; after: number | undefined; count: number; terminal: boolean }
const terminalKinds = new Set(['turn-ended', 'turn-failed', 'closed'])
function eventPage(value: unknown, id: string) {
  const page = object(value)
  if (page['session-id'] !== id || !Array.isArray(page.events) || !integer(page['next-after']) || !integer(page.dropped)) throw new Error('Invalid chat event page.')
  return { events: page.events, next: page['next-after'] }
}
function eventRow(value: unknown, id: string): Record<string, unknown> & { seq: number } {
  const row = object(value)
  if (row.store !== CHAT_STORE || row['session-id'] !== id || !integer(row.seq)) throw new Error('An event belongs to another conversation.')
  return { ...row, seq: row.seq }
}
function appendDelta(log: ChatTurn[], event: Record<string, unknown>): ChatTurn[] {
  if (typeof event.text !== 'string' || typeof event['turn-id'] !== 'string') throw new Error('Invalid chat text update.')
  const index = log.findIndex(turn => turn['turn-id'] === event['turn-id'])
  if (index < 0 || log[index].status !== 'running') throw new Error('The reply needs to be reconciled with saved history.')
  const updated = { ...log[index], answer: log[index].answer + event.text }
  return log.map((turn, i) => i === index ? updated : turn)
}
function applyEvent(log: ChatTurn[], event: Record<string, unknown>): ChatTurn[] {
  if (event.kind === 'delta') return appendDelta(log, event)
  if (event.kind === 'turn-started') {
    if (typeof event['turn-id'] !== 'string' || typeof event.message !== 'string') throw new Error('Invalid started turn.')
    if (log.some(turn => turn['turn-id'] === event['turn-id'])) return log
    return [...log, { 'turn-id': event['turn-id'], seq: log.length, message: event.message, answer: '', status: 'running' }]
  }
  if (event.kind !== 'created' && !terminalKinds.has(String(event.kind))) throw new Error('The chat feed changed; refreshing saved history.')
  return log
}
/** One ordered page. A lost or unknown event requires an authoritative snapshot. */
export function applyChatEvents(current: ChatSnapshot, value: unknown, after: number | undefined): FeedResult {
  const page = eventPage(value, current['session-id'])
  let log = current.log
  let cursor = after
  let terminal = false
  for (const raw of page.events) {
    const event = eventRow(raw, current['session-id'])
    if (cursor !== undefined && event.seq <= cursor) continue
    if (event.seq !== (cursor === undefined ? 0 : cursor + 1)) throw new Error('Some chat updates are no longer available.')
    log = applyEvent(log, event)
    if (terminalKinds.has(String(event.kind))) terminal = true
    cursor = event.seq
  }
  if (cursor !== undefined && page.next !== cursor) throw new Error('The chat feed returned an inconsistent cursor.')
  return { snapshot: { ...current, log, turns: log.length, 'event-after': cursor }, after: cursor, count: page.events.length, terminal }
}
