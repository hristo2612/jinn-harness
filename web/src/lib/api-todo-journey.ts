import { authFetch } from './auth'
import { writeHeaders } from './api-write'
import { integer, object, turn, type ChatTurn, type TurnStatus } from './api-chat'
import statusTable from './todo-status-table.json'

export type TodoStatus = keyof typeof statusTable
export interface TodoDispatch {
  'dispatch-id': string
  'session-store': string
  'session-id'?: string
  'turn-id'?: string
  engine: string
  status: TurnStatus
  answer: string
  reason?: string
  'client-request'?: string
}
export interface TodoComment { 'comment-id': string; seq: number; body: string; 'client-request'?: string }
export interface TodoChange { seq: number; from: TodoStatus; to: TodoStatus; note?: string; 'client-request'?: string; 'reviewed-dispatch'?: string }
export interface TodoSummary { 'todo-id': string; title: string; status: TodoStatus; 'created-ms': number }
export interface TodoRecord extends TodoSummary {
  store: 'work'
  revision: number
  body: string
  acceptance: string
  'declared-status': TodoStatus
  'status-reason'?: string
  comments: TodoComment[]
  dispatches: TodoDispatch[]
  history: TodoChange[]
  refused: TodoChange[]
  metadata?: Record<string, unknown>
}
export interface Inspection { revision: number; dispatch: string }
export interface TodoDraft { title: string; body: string; acceptance: string }
export const TODO_BASE = '/v1/todos/work'
export const TASK_ROUTE = { store: 'tasks', engine: { engine: 'codex', model: 'gpt-6-astra', effort: 'high' } } as const
export const statusLabel: Record<TodoStatus, string> = { backlog: 'Captured', executing: 'In progress', 'in-review': 'In review', blocked: 'Blocked', done: 'Accepted', cancelled: 'Cancelled' }
export function lawful(from: TodoStatus, to: TodoStatus): boolean { return (statusTable[from] as readonly string[]).includes(to) }
function status(value: unknown): value is TodoStatus { return typeof value === 'string' && Object.hasOwn(statusTable, value) }
function dispatch(value: unknown): TodoDispatch {
  const row = object(value)
  if (typeof row['dispatch-id'] !== 'string' || typeof row.answer !== 'string' || typeof row['session-store'] !== 'string'
    || !['running', 'done', 'failed', 'cancelled', 'interrupted'].includes(String(row.status))) throw new Error('The task result is unreadable.')
  return row as unknown as TodoDispatch
}
function comment(value: unknown): TodoComment {
  const row = object(value)
  if (typeof row.body !== 'string' || typeof row['comment-id'] !== 'string' || !integer(row.seq)) throw new Error('Todo context is unreadable.')
  return row as unknown as TodoComment
}
function change(value: unknown): TodoChange {
  const row = object(value)
  if (!integer(row.seq) || !status(row.from) || !status(row.to)) throw new Error('Todo history is unreadable.')
  return row as unknown as TodoChange
}
function summary(value: unknown): TodoSummary {
  const row = object(value)
  if (typeof row['todo-id'] !== 'string' || typeof row.title !== 'string' || !status(row.status)) throw new Error('The Todo list is unreadable.')
  return row as unknown as TodoSummary
}
function recordFields(row: Record<string, unknown>, id: string): boolean {
  return row['todo-id'] === id && row.store === 'work' && integer(row.revision) && Number(row.revision) > 0
    && typeof row.body === 'string' && typeof row.acceptance === 'string' && status(row['declared-status'])
}
export function parseTodo(value: unknown, id: string): TodoRecord {
  const row = object(value)
  summary(row)
  if (!recordFields(row, id) || !Array.isArray(row.comments) || !Array.isArray(row.dispatches)
    || !Array.isArray(row.history) || !Array.isArray(row.refused)) throw new Error('This Todo has no readable revision or history. Writes are unavailable.')
  return { ...row, comments: row.comments.map(comment), dispatches: row.dispatches.map(dispatch), history: row.history.map(change), refused: row.refused.map(change) } as unknown as TodoRecord
}
export function todoPrompt(todo: TodoRecord): string {
  let prompt = `Todo ${todo['todo-id']}: ${todo.title}`
  if (todo.body.trim()) prompt += `\n\n${todo.body.trim()}`
  if (todo.acceptance.trim()) prompt += `\n\nAcceptance: ${todo.acceptance.trim()}`
  for (const comment of [...todo.comments].sort((a, b) => a.seq - b.seq)) prompt += `\n\nContext ${comment.seq + 1}: ${comment.body}`
  for (const change of todo.history) {
    if (change.note?.trim()) prompt += `\n\nDecision note ${change.seq + 1} (${change.from} -> ${change.to}): ${change.note}`
  }
  return prompt
}
export function reviewable(todo: TodoRecord, inspected: Inspection | null): boolean {
  const latest = todo.dispatches.at(-1)
  return Boolean(inspected && inspected.revision === todo.revision && latest?.status === 'done' && latest['dispatch-id'] === inspected.dispatch)
}
export class TodoHttpError extends Error {
  constructor(message: string, public rejected: boolean) { super(message) }
}
async function request(path: string, init?: RequestInit): Promise<unknown> {
  const response = await authFetch(path, init)
  const value: unknown = await response.json()
  if (!response.ok) {
    const error = object(object(value).error)
    const code = String(error['store-code'] ?? error.code)
    throw new TodoHttpError(String(error.message ?? error.detail ?? 'Todo request failed.'), ['refused', 'invalid', 'not-found', 'unauthenticated'].includes(code))
  }
  return value
}
export function todoPath(id: string): string { return `${TODO_BASE}/${encodeURIComponent(id)}` }
export async function readTodo(id: string, signal?: AbortSignal): Promise<TodoRecord> { return parseTodo(await request(todoPath(id), { signal }), id) }
export async function listTodos(signal?: AbortSignal): Promise<TodoSummary[]> {
  const row = object(await request(TODO_BASE, { signal }))
  if (!Array.isArray(row.todos)) throw new Error('The Todo list is unreadable.')
  return row.todos.map(summary).sort((a, b) => b['created-ms'] - a['created-ms'])
}
export async function createTodo(draft: TodoDraft, client: string): Promise<string> {
  const row = object(await request(TODO_BASE, { method: 'POST', headers: writeHeaders(), body: JSON.stringify({ ...draft, actor: 'operator', metadata: { 'client-request': client } }) }))
  if (typeof row['todo-id'] !== 'string') throw new Error('Capture was not confirmed.')
  return row['todo-id']
}
export async function mutateTodo(todo: TodoRecord, action: string, body: object, client: string): Promise<TodoRecord> {
  return parseTodo(await request(`${todoPath(todo['todo-id'])}/${action}`, { method: 'POST', headers: writeHeaders(), body: JSON.stringify({ ...body, actor: 'operator', 'expected-revision': todo.revision, 'client-request': client }) }), todo['todo-id'])
}
function taskPath(dispatch: TodoDispatch): string {
  if (dispatch['session-store'] !== 'tasks' || !dispatch['session-id']) throw new Error('The live task link is unavailable; worker outcome is unconfirmed.')
  return `/v1/sessions/tasks/${encodeURIComponent(dispatch['session-id'])}`
}
export async function readTask(dispatch: TodoDispatch, signal?: AbortSignal): Promise<ChatTurn | undefined> {
  const row = object(await request(taskPath(dispatch), { signal }))
  if (!Array.isArray(row.log)) throw new Error('Live task history is unreadable.')
  return row.log.map(turn).find(item => item['turn-id'] === dispatch['turn-id'])
}
export async function stopTask(dispatch: TodoDispatch): Promise<void> {
  await request(`${taskPath(dispatch)}/turns`, { method: 'DELETE' })
}
export function witnessed(todo: TodoRecord, client: string): boolean {
  return [...todo.comments, ...todo.history, ...todo.dispatches, ...todo.refused].some(row => row['client-request'] === client)
}
