import { authFetch } from './auth'
import { momentResponse } from './api-moments'
import { writeHeaders } from './api-write'

export const CHAT_STORE = 'chat'
const base = `/v1/sessions/${CHAT_STORE}`
export type TurnStatus = 'running' | 'done' | 'failed' | 'cancelled' | 'interrupted'
export interface ChatTurn {
  'turn-id': string
  seq: number
  status: TurnStatus
  message: string
  answer: string
  reason?: string
}
export interface ChatSnapshot {
  'session-id': string
  store: string
  status: 'idle' | 'running' | 'failed' | 'closed'
  turns: number
  log: ChatTurn[]
  'event-after'?: number
}
export interface ChatSummary {
  'session-id': string
  status: string
  title?: string
  metadata?: Record<string, unknown>
  'created-ms': number
}
export class ChatHttpError extends Error {
  constructor(public status: number, message: string) { super(message) }
}
export function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('The chat service returned invalid data.')
  return value as Record<string, unknown>
}
export function integer(value: unknown): value is number { return Number.isSafeInteger(value) && Number(value) >= 0 }
export function turn(value: unknown): ChatTurn {
  const row = object(value)
  if (typeof row['turn-id'] !== 'string' || !integer(row.seq) || typeof row.message !== 'string' || typeof row.answer !== 'string'
    || !['running', 'done', 'failed', 'cancelled', 'interrupted'].includes(String(row.status))
    || (row.reason !== undefined && typeof row.reason !== 'string')) throw new Error('The chat service returned an invalid turn.')
  return row as unknown as ChatTurn
}
function validWatermark(row: Record<string, unknown>): boolean { return row['event-after'] === undefined || integer(row['event-after']) }
export function snapshot(value: unknown, id: string): ChatSnapshot {
  const row = object(value)
  if (row['session-id'] !== id || row.store !== CHAT_STORE || !integer(row.turns) || !Array.isArray(row.log)
    || !['idle', 'running', 'failed', 'closed'].includes(String(row.status))
    || !validWatermark(row)) throw new Error('The chat snapshot does not match this conversation.')
  const log = row.log.map(turn)
  if (log.length !== row.turns || log.some((item, i) => item.seq !== i)
    || new Set(log.map(item => item['turn-id'])).size !== log.length) throw new Error('The chat history is incomplete or duplicated.')
  return { ...row, log } as unknown as ChatSnapshot
}
async function response(res: Response): Promise<unknown> {
  const value: unknown = await res.json()
  if (!res.ok) {
    const error = object(value).error
    const details = error && typeof error === 'object' ? error as Record<string, unknown> : {}
    throw new ChatHttpError(res.status, String(details.detail ?? details.message ?? `Chat request failed (${res.status}).`))
  }
  return value
}
async function request(path: string, init: RequestInit = {}): Promise<unknown> {
  return response(await authFetch(path, init))
}
export function chatPath(id: string): string { return `${base}/${encodeURIComponent(id)}` }
export async function readChat(id: string, signal?: AbortSignal): Promise<ChatSnapshot> {
  return snapshot(await request(chatPath(id), { signal }), id)
}
export async function listChats(signal?: AbortSignal): Promise<ChatSummary[]> {
  const data = await request(base, { signal })
  if (!Array.isArray(data)) throw new Error('The chat service returned an invalid conversation list.')
  return data.map(value => {
    const row = object(value)
    if (typeof row['session-id'] !== 'string' || !integer(row['created-ms'])) throw new Error('Invalid saved conversation.')
    return row as unknown as ChatSummary
  }).sort((a, b) => b['created-ms'] - a['created-ms'])
}
export async function createChat(clientRequest: string): Promise<string> {
  const spec = { engine: { engine: 'codex', model: 'gpt-6-astra', effort: 'high' }, tools: { mode: 'denied', allow: [] },
    'transcript-context': true, attribution: {}, metadata: { 'client-request': clientRequest } }
  const folded = object(await response(await momentResponse('ui', 'before-create-session', spec)))
  if (JSON.stringify(folded) !== JSON.stringify(spec)) {
    // Compare structures independently of key order; this slice permits only the original text-chat identity.
    if (JSON.stringify(sortObject(folded)) !== JSON.stringify(sortObject(spec))) throw new Error('A customization changed unsupported conversation settings. Nothing was created.')
  }
  const result = object(await request(base, { method: 'POST', headers: writeHeaders(), body: JSON.stringify(folded) }))
  if (typeof result['session-id'] !== 'string') throw new Error('The new conversation was not confirmed. Check saved chats before trying again.')
  return result['session-id']
}
function sortObject(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortObject)
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, sortObject(v)]))
  return value
}
export async function prepareMessage(id: string, text: string): Promise<string> {
  const folded = object(await response(await momentResponse('ui', 'before-send', { text, attachments: [], 'session-id': id })))
  if (typeof folded.text !== 'string' || !folded.text.trim() || folded['session-id'] !== id
    || !Array.isArray(folded.attachments) || folded.attachments.length !== 0
    || Object.keys(folded).some(key => !['text', 'attachments', 'session-id'].includes(key))) {
    throw new Error('A customization changed unsupported message fields. Nothing was sent.')
  }
  return folded.text
}
export async function postMessage(id: string, message: string): Promise<void> {
  const result = object(await request(`${chatPath(id)}/turns`, { method: 'POST', headers: writeHeaders(), body: JSON.stringify({ message }) }))
  if (result['session-id'] !== id || typeof result['turn-id'] !== 'string') throw new Error('The send was not confirmed. Checking saved history.')
}
export async function stopChat(id: string): Promise<ChatSnapshot> {
  return snapshot(await request(`${chatPath(id)}/turns`, { method: 'DELETE' }), id)
}
export async function chatEvents(id: string, after: number | undefined, signal?: AbortSignal): Promise<unknown> {
  return request(`${chatPath(id)}/events?limit=50${after === undefined ? '' : `&after=${after}`}`, { signal })
}
