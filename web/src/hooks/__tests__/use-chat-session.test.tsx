import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import * as api from '@/lib/api-chat'
import { useChatSession } from '../use-chat-session'

vi.mock('@/lib/api-chat', async importOriginal => ({ ...await importOriginal<typeof import('@/lib/api-chat')>(),
  readChat: vi.fn(), listChats: vi.fn(), chatEvents: vi.fn(), prepareMessage: vi.fn(), postMessage: vi.fn(), stopChat: vi.fn() }))
const empty = (id = 'a'): api.ChatSnapshot => ({ 'session-id': id, store: 'chat', status: 'idle', turns: 0, log: [], 'event-after': 0 })
beforeEach(() => {
  vi.mocked(api.readChat).mockImplementation(async id => empty(id))
  vi.mocked(api.listChats).mockResolvedValue([])
  vi.mocked(api.chatEvents).mockImplementation(async id => ({ 'session-id': id, events: [], 'next-after': 0, dropped: 0 }))
  vi.mocked(api.prepareMessage).mockImplementation(async (_id, text) => text.toUpperCase())
  vi.mocked(api.postMessage).mockResolvedValue()
})
afterEach(() => vi.resetAllMocks())

it('posts once under a double submit and shows the transformed optimistic message', async () => {
  let release!: () => void
  vi.mocked(api.postMessage).mockImplementation(() => new Promise(resolve => { release = resolve }))
  const { result, unmount } = renderHook(() => useChatSession('a'))
  await waitFor(() => expect(result.current.record).not.toBeNull())
  let sent!: Promise<boolean>
  act(() => { sent = result.current.send('draft'); void result.current.send('draft') })
  await waitFor(() => expect(result.current.pending?.message).toBe('DRAFT'))
  expect(api.postMessage).toHaveBeenCalledTimes(1)
  await act(async () => { release(); await sent })
  unmount()
})

it('never replays an ambiguous send and keeps the pending text until history is readable', async () => {
  const { result, unmount } = renderHook(() => useChatSession('a'))
  await waitFor(() => expect(result.current.record).not.toBeNull())
  vi.mocked(api.postMessage).mockRejectedValue(new Error('connection lost'))
  vi.mocked(api.readChat).mockRejectedValue(new Error('offline'))
  await act(async () => { expect(await result.current.send('draft')).toBe(false) })
  expect(result.current.pending?.message).toBe('DRAFT')
  await act(async () => { await result.current.send('draft') })
  expect(api.postMessage).toHaveBeenCalledTimes(1)
  unmount()
})

it('discards a late snapshot from a conversation that is no longer selected', async () => {
  let release!: (value: api.ChatSnapshot) => void
  vi.mocked(api.readChat).mockImplementation(id => id === 'a' ? new Promise(resolve => { release = resolve }) : Promise.resolve(empty(id)))
  const { result, rerender, unmount } = renderHook(({ id }) => useChatSession(id), { initialProps: { id: 'a' } })
  rerender({ id: 'b' })
  await waitFor(() => expect(result.current.record?.['session-id']).toBe('b'))
  await act(async () => { release(empty('a')) })
  expect(result.current.record?.['session-id']).toBe('b')
  unmount()
})

it('retains received text when a stop request cannot be confirmed', async () => {
  vi.mocked(api.readChat).mockResolvedValue({ ...empty(), status: 'running', turns: 1,
    log: [{ 'turn-id': 't', seq: 0, message: 'hello', answer: 'prefix', status: 'running' }] })
  vi.mocked(api.stopChat).mockRejectedValue(new Error('offline'))
  const { result, unmount } = renderHook(() => useChatSession('a'))
  await waitFor(() => expect(result.current.record?.turns).toBe(1))
  await act(async () => { await result.current.stop() })
  expect(result.current.record?.log[0].answer).toBe('prefix')
  expect(result.current.error).toContain('Stop unconfirmed')
  unmount()
})
