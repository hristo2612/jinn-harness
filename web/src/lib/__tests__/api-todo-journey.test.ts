import { describe, expect, it, vi } from 'vitest'
import { parseTodo, todoPrompt, reviewable } from '../api-todo-journey'

vi.mock('../auth', () => ({ authFetch: vi.fn() }))
const todo = { 'todo-id': 'work-1', store: 'work', revision: 4, title: 'Draft', body: 'For the team', acceptance: 'Include a date', status: 'executing', 'declared-status': 'executing', comments: [{ seq: 0, body: 'Friday', 'comment-id': 'c1' }], history: [], refused: [], dispatches: [] }
describe('bounded Todo reading and operator inspection', () => {
  it('rejects stale schema identity or missing revision instead of enabling writes', () => {
    expect(() => parseTodo({ ...todo, revision: undefined }, 'work-1')).toThrow()
    expect(() => parseTodo({ ...todo, store: 'other' }, 'work-1')).toThrow()
    expect(() => parseTodo({ ...todo, status: 'in_review' }, 'work-1')).toThrow()
  })
  it('shows the exact ordered context prompt and captured acceptance', () => {
    const record = parseTodo(todo, 'work-1')
    expect(todoPrompt(record)).toBe('Todo work-1: Draft\n\nFor the team\n\nAcceptance: Include a date\n\nContext 1: Friday')
  })
  it('binds an inspection to the latest complete dispatch and current revision', () => {
    const done = { 'dispatch-id': 'd1', status: 'done', answer: 'Friday', engine: 'codex', 'session-store': 'tasks' }
    const record = parseTodo({ ...todo, dispatches: [done] }, 'work-1')
    expect(reviewable(record, { revision: 4, dispatch: 'd1' })).toBe(true)
    expect(reviewable({ ...record, revision: 5 }, { revision: 4, dispatch: 'd1' })).toBe(false)
    expect(reviewable({ ...record, dispatches: [...record.dispatches, { ...done, 'dispatch-id': 'd2', status: 'running' }] }, { revision: 4, dispatch: 'd1' })).toBe(false)
    expect(reviewable(record, null)).toBe(false)
  })
})
