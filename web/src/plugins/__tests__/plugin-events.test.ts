import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPluginContext, pluginEventsUrl } from '../plugin-context'
import { createBrowserGatewayTransport, installGatewayTransport } from '../../lib/gateway-transport'

/**
 * `ctx.events`. The property under test is that a plugin reaches its OWN event
 * stream and has no way to name another's: the signature takes a handler and a
 * cursor, and the id comes from the context it was built with.
 */

interface FakeSocket {
  url: string
  closed: boolean
  listeners: Map<string, (event: MessageEvent) => void>
  addEventListener: (type: string, listener: (event: MessageEvent) => void) => void
  close: () => void
  /** Deliver a frame the way the gateway's events socket does. */
  deliver: (payload: unknown) => void
}

const sockets: FakeSocket[] = []

class SocketDouble {
  constructor(url: string) {
    const socket: FakeSocket = {
      url,
      closed: false,
      listeners: new Map(),
      addEventListener: (type, listener) => void socket.listeners.set(type, listener),
      close: () => void (socket.closed = true),
      deliver: (payload) =>
        socket.listeners.get('message')?.({ data: JSON.stringify(payload) } as MessageEvent),
    }
    sockets.push(socket)
    return socket as unknown as SocketDouble
  }
}

const lastSocket = () => sockets.at(-1)!
let restoreTransport: (() => void) | null = null

beforeEach(() => {
  sockets.length = 0
  vi.stubGlobal('WebSocket', SocketDouble)
  restoreTransport = installGatewayTransport(createBrowserGatewayTransport({
    origin: 'https://qa-a.example:7779',
    request: vi.fn(),
    navigate: vi.fn(),
  }))
})

afterEach(() => {
  restoreTransport?.()
  restoreTransport = null
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('pluginEventsUrl', () => {
  it('targets the plugin’s own events path, and carries a cursor when given one', () => {
    expect(pluginEventsUrl('inbox-demo')).toBe('wss://qa-a.example:7779/api/plugins/inbox-demo/events')
    expect(pluginEventsUrl('inbox-demo', 12)).toBe(
      'wss://qa-a.example:7779/api/plugins/inbox-demo/events?since=12',
    )
  })
})

describe('ctx.events', () => {
  it('opens the stream of the plugin the context was built for', () => {
    createPluginContext('inbox-demo').events(() => {})

    expect(lastSocket().url).toContain('/api/plugins/inbox-demo/events')
  })

  it('cannot be pointed at another plugin, whatever the caller passes', () => {
    const context = createPluginContext('inbox-demo')

    // The only options a caller has are a cursor. An id smuggled in beside it is
    // not read, because there is nothing in the signature that would read it.
    context.events(() => {}, { since: 3, id: 'other-plugin', pluginId: 'other-plugin' } as never)

    expect(lastSocket().url).toBe('wss://qa-a.example:7779/api/plugins/inbox-demo/events?since=3')
    expect(lastSocket().url).not.toContain('other-plugin')
  })

  it('hands the handler each event the backend emitted, and not the ring position', () => {
    const seen: unknown[] = []
    createPluginContext('inbox-demo').events((event) => seen.push(event))

    lastSocket().deliver({
      events: [
        { cursor: 1, event: { type: 'arrived', id: 'a.txt' } },
        { cursor: 2, event: { type: 'approved', id: 'a.txt' } },
      ],
      cursor: 2,
      dropped: false,
    })

    expect(seen).toEqual([
      { type: 'arrived', id: 'a.txt' },
      { type: 'approved', id: 'a.txt' },
    ])
  })

  it('reports an unreadable frame rather than dropping it in silence', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const seen: unknown[] = []
    createPluginContext('inbox-demo').events((event) => seen.push(event))

    lastSocket().listeners.get('message')?.({ data: 'not json' } as MessageEvent)

    expect(seen).toEqual([])
    expect(warn.mock.calls[0]?.[0]).toContain('inbox-demo')
  })

  it('returns an unsubscribe that closes the socket', () => {
    const unsubscribe = createPluginContext('inbox-demo').events(() => {})
    expect(lastSocket().closed).toBe(false)

    unsubscribe()

    expect(lastSocket().closed).toBe(true)
  })

  it('is tracked, so unloading the plugin closes the stream too', () => {
    const disposers: (() => void)[] = []
    createPluginContext('inbox-demo', (dispose) => disposers.push(dispose)).events(() => {})

    for (const dispose of disposers) dispose()

    expect(lastSocket().closed).toBe(true)
  })
})
