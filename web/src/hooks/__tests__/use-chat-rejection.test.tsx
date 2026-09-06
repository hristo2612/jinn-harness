import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { authFetch } from '@/lib/auth'
import type { ChatSnapshot } from '@/lib/api-chat'
import { useChatSession } from '../use-chat-session'

vi.mock('@/lib/auth', () => ({ authFetch: vi.fn() }))
afterEach(() => vi.resetAllMocks())
const refusal = 'This conversation exceeds the 32 KiB context limit. Start a new chat.'
const empty: ChatSnapshot = { 'session-id': 'a', store: 'chat', status: 'idle', turns: 0, log: [], 'event-after': 0 }
function scenario(post: () => Response | never) {
  const state = { sent: false, offline: true, posts: 0, saved: empty }
  vi.mocked(authFetch).mockImplementation(async (path, init) => {
    if (path.includes('/moments/')) return new Response(String(init?.body))
    if (path.endsWith('/turns') && init?.method === 'POST') {
      state.sent = true; state.posts += 1
      return post()
    }
    if (state.sent && state.offline) return new Response(JSON.stringify({ error: { code: 'refused', detail: 'History unavailable' } }), { status: 502 })
    if (path.includes('/events')) return new Response(JSON.stringify({ 'session-id': 'a', events: [], 'next-after': 0, dropped: 0 }))
    if (path === '/v1/sessions/chat') return new Response('[]')
    return new Response(JSON.stringify(state.saved))
  })
  return state
}
it('keeps a definitive refusal actionable through failed and recovered background reads', async () => {
  const state = scenario(() => new Response(JSON.stringify({ error: { code: 'refused', 'store-code': 'refused', detail: refusal } }), { status: 502 }))
  const { result, unmount } = renderHook(() => useChatSession('a'))
  try {
    await waitFor(() => expect(result.current.record).not.toBeNull())
    await act(async () => { expect(await result.current.send('x'.repeat(33000))).toBe(false) })
    await waitFor(() => expect(result.current.feed).toBe('recovering'))
    expect(result.current.pending).toBeNull()
    expect(result.current.busy).toBe(false)
    expect(result.current.error).toBe(refusal)
    state.offline = false
    act(() => result.current.retry())
    await waitFor(() => expect(result.current.feed).toBe('live'))
    expect(result.current.error).toBe(refusal)
    expect(result.current.pending).toBeNull()
    expect(state.posts).toBe(1)
  } finally { unmount() }
})
it.each(['accepted', 'network', 'store-failure', 'untyped-http'] as const)('protects %s delivery when history is unreadable', async kind => {
  const state = scenario(() => {
    if (kind === 'network') throw new TypeError('Failed to fetch')
    if (kind === 'accepted') return new Response(JSON.stringify({ 'session-id': 'a', 'turn-id': 't' }))
    const error = kind === 'store-failure' ? { code: 'refused', 'store-code': 'failed', detail: 'Write unconfirmed' } : { detail: 'Proxy unavailable' }
    return new Response(JSON.stringify({ error }), { status: 502 })
  })
  const { result, unmount } = renderHook(() => useChatSession('a'))
  try {
    await waitFor(() => expect(result.current.record).not.toBeNull())
    await act(async () => { expect(await result.current.send('draft')).toBe(false) })
    await waitFor(() => expect(result.current.feed).toBe('recovering'))
    expect(result.current.pending?.message).toBe('draft')
    expect(result.current.error).toContain('Send not confirmed')
    await act(async () => { expect(await result.current.send('draft')).toBe(false) })
    expect(state.posts).toBe(1)
    state.saved = { ...empty, turns: 1, log: [{ 'turn-id': 't', seq: 0, message: 'draft', answer: 'confirmed', status: 'done' }] }
    state.offline = false
    act(() => result.current.retry())
    await waitFor(() => expect(result.current.pending).toBeNull())
    expect(result.current.record?.log[0].answer).toBe('confirmed')
    expect(result.current.error).toBeNull()
    expect(state.posts).toBe(1)
  } finally { unmount() }
})
