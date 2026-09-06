import { beforeEach, expect, it, vi } from 'vitest'
import { authFetch } from '../auth'
import { createChat, prepareMessage } from '../api-chat'

vi.mock('../auth', () => ({ authFetch: vi.fn() }))
beforeEach(() => vi.resetAllMocks())
const reply = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status })

it('creates the exact fixed identity through the existing moment and direct SessionSpec body', async () => {
  vi.mocked(authFetch).mockImplementation(async (path, init) => {
    if (path.includes('/moments/')) return reply(JSON.parse(String(init?.body)))
    return reply({ 'session-id': 'chat-1' })
  })
  expect(await createChat('request-1')).toBe('chat-1')
  const [, init] = vi.mocked(authFetch).mock.calls[1]
  expect(JSON.parse(String(init?.body))).toEqual({ engine: { engine: 'codex', model: 'gpt-6-astra', effort: 'high' },
    tools: { mode: 'denied', allow: [] }, 'transcript-context': true, attribution: {}, metadata: { 'client-request': 'request-1' } })
})

it('refuses unsupported folded creation fields before writing a session', async () => {
  vi.mocked(authFetch).mockImplementation(async (_path, init) => reply({ ...JSON.parse(String(init?.body)), tools: { mode: 'unrestricted' } }))
  await expect(createChat('request-1')).rejects.toThrow('unsupported')
  expect(authFetch).toHaveBeenCalledTimes(1)
})

it('uses a folded text change, but rejects an identity change or attachment', async () => {
  vi.mocked(authFetch).mockResolvedValueOnce(reply({ text: 'rewritten 🟢', attachments: [], 'session-id': 'a' }))
  expect(await prepareMessage('a', 'original')).toBe('rewritten 🟢')
  vi.mocked(authFetch).mockResolvedValueOnce(reply({ text: 'rewritten', attachments: [], 'session-id': 'b' }))
  await expect(prepareMessage('a', 'original')).rejects.toThrow('unsupported')
  vi.mocked(authFetch).mockResolvedValueOnce(reply({ text: 'rewritten', attachments: ['file'], 'session-id': 'a' }))
  await expect(prepareMessage('a', 'original')).rejects.toThrow('unsupported')
})
