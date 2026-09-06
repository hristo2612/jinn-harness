import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, expect, it, vi } from 'vitest'
import type { ReactNode } from 'react'
import { formatMessage } from '../message-markdown'
import { TextTranscript } from '../text-transcript'
import ChatPage from '@/routes/chat/page'

const send = vi.hoisted(() => vi.fn().mockResolvedValue(true))
vi.mock('@/components/page-layout', () => ({ PageLayout: ({ children }: { children: ReactNode }) => <>{children}</> }))
vi.mock('@/lib/api-chat', async original => ({ ...await original<typeof import('@/lib/api-chat')>(), listChats: vi.fn().mockResolvedValue([]) }))
vi.mock('@/hooks/use-chat-session', () => ({ useChatSession: () => ({ record: { 'session-id': 'a', store: 'chat', status: 'idle', turns: 0, log: [] }, pending: null, busy: false, error: null, feed: 'live', send, stop: vi.fn(), retry: vi.fn() }) }))
vi.mock('../message-markdown', async original => {
  const actual = await original<typeof import('../message-markdown')>()
  return { ...actual, formatMessage: vi.fn(actual.formatMessage) }
})
beforeEach(() => { send.mockClear(); vi.mocked(formatMessage).mockClear() })

it('keeps settled message formatting out of partial updates', () => {
  const first = { 'turn-id': 'one', seq: 0, message: 'settled user', answer: 'settled answer', status: 'done' as const }
  const second = { 'turn-id': 'two', seq: 1, message: 'next user', answer: 'partial', status: 'running' as const }
  const view = render(<TextTranscript turns={[first, second]} />)
  vi.mocked(formatMessage).mockClear()
  view.rerender(<TextTranscript turns={[{ ...first }, { ...second, answer: 'partial update' }]} />)
  expect(vi.mocked(formatMessage).mock.calls.map(([text]) => text)).toEqual(['partial update'])
})

it('renders a stable unfinished code block, safe links, headings and table cells', () => {
  const view = render(<>{formatMessage('# Heading\n[unsafe](javascript:alert(1))\n[valid](https://example.com)\n| A | B |\n|---|---|\n| one | two |\n```ts\nconst value = 1')}</>)
  expect(screen.getByRole('link', { name: 'valid' }).getAttribute('href')).toBe('https://example.com')
  expect(screen.queryByRole('link', { name: 'unsafe' })).toBeNull()
  expect(screen.getByRole('table').textContent).toContain('onetwo')
  const code = screen.getByText('const value = 1')
  view.rerender(<>{formatMessage('# Heading\n[unsafe](javascript:alert(1))\n[valid](https://example.com)\n| A | B |\n|---|---|\n| one | two |\n```ts\nconst value = 1\n```')}</>)
  expect(screen.getByText('const value = 1')).toBe(code)
})

it('does not submit IME Enter or Shift Enter, then submits one ordinary Enter', async () => {
  render(<MemoryRouter initialEntries={['/?chat=a']}><ChatPage /></MemoryRouter>)
  const input = screen.getByRole('textbox', { name: 'Message' })
  fireEvent.change(input, { target: { value: 'A draft' } })
  fireEvent.compositionStart(input)
  fireEvent.keyDown(input, { key: 'Enter', keyCode: 229, isComposing: true })
  fireEvent.compositionEnd(input)
  fireEvent.keyDown(input, { key: 'Enter', shiftKey: true })
  expect(send).not.toHaveBeenCalled()
  fireEvent.keyDown(input, { key: 'Enter' })
  fireEvent.keyDown(input, { key: 'Enter' })
  await waitFor(() => expect(send).toHaveBeenCalledTimes(1))
  expect(send).toHaveBeenCalledWith('A draft')
})
