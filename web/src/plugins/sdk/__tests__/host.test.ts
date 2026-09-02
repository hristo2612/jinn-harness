import { beforeEach, describe, expect, it, vi } from 'vitest'
import { host, PluginSdkError } from '../host'
import { clearHostBridge, registerHostNavigator, registerHostNotificationSink } from '../host-bridge'
import { dispatchHostEvent, resetHostEvents } from '../host-events'
import { publishHostState, resetHostState } from '../host-state'

beforeEach(() => {
  clearHostBridge()
  resetHostEvents()
  resetHostState()
})

describe('host.onEvent', () => {
  it('delivers to the handlers after one that throws, and does not throw to the dispatcher', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {})
    const second = vi.fn()
    host.onEvent('session:updated', () => {
      throw new Error('a plugin handler blew up')
    })
    host.onEvent('session:updated', second)

    const frame = { event: 'session:updated', payload: { id: 'sess-1' } }
    expect(() => dispatchHostEvent(frame)).not.toThrow()

    expect(second).toHaveBeenCalledWith(frame)
    expect(consoleError).toHaveBeenCalled()
    consoleError.mockRestore()
  })

  it('stops delivering once the returned unsubscribe runs', () => {
    const handler = vi.fn()
    const unsubscribe = host.onEvent('queue:updated', handler)

    dispatchHostEvent({ event: 'queue:updated', payload: {} })
    unsubscribe()
    dispatchHostEvent({ event: 'queue:updated', payload: {} })

    expect(handler).toHaveBeenCalledTimes(1)
  })

  it('unsubscribing one handler leaves the others on the same type subscribed', () => {
    const first = vi.fn()
    const second = vi.fn()
    const unsubscribeFirst = host.onEvent('queue:updated', first)
    host.onEvent('queue:updated', second)

    unsubscribeFirst()
    dispatchHostEvent({ event: 'queue:updated', payload: {} })

    expect(first).not.toHaveBeenCalled()
    expect(second).toHaveBeenCalledTimes(1)
  })

  it('delivers only the type that was subscribed to', () => {
    const handler = vi.fn()
    host.onEvent('queue:updated', handler)

    dispatchHostEvent({ event: 'session:updated', payload: {} })

    expect(handler).not.toHaveBeenCalled()
  })
})

describe('host.state', () => {
  it('returns the same snapshot object across two reads with no publish between them', () => {
    expect(host.state.getSnapshot()).toBe(host.state.getSnapshot())
  })

  it('keeps the snapshot identity when a publish changes nothing', () => {
    const before = host.state.getSnapshot()

    publishHostState({ gatewayStatus: 'disconnected' })

    expect(host.state.getSnapshot()).toBe(before)
  })

  it('fires subscribers on a publish that changes a value, and not on one that does not', () => {
    const listener = vi.fn()
    host.state.subscribe(listener)

    publishHostState({ activeSession: 'sess-1' })
    expect(listener).toHaveBeenCalledTimes(1)
    expect(listener).toHaveBeenCalledWith(expect.objectContaining({ activeSession: 'sess-1' }))

    publishHostState({ activeSession: 'sess-1' })
    expect(listener).toHaveBeenCalledTimes(1)
  })

  it('stops firing once the returned unsubscribe runs', () => {
    const listener = vi.fn()
    const unsubscribe = host.state.subscribe(listener)

    unsubscribe()
    publishHostState({ activeSession: 'sess-1' })

    expect(listener).not.toHaveBeenCalled()
  })

  it('isolates a subscriber that throws from the ones behind it', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {})
    const second = vi.fn()
    host.state.subscribe(() => {
      throw new Error('a plugin subscriber blew up')
    })
    host.state.subscribe(second)

    expect(() => publishHostState({ activeSession: 'sess-1' })).not.toThrow()
    expect(second).toHaveBeenCalledTimes(1)
    consoleError.mockRestore()
  })

  it('exposes no setter for the fields it publishes', () => {
    publishHostState({ activeSession: 'sess-1', gatewayStatus: 'connected' })
    const snapshot = host.state.getSnapshot()

    expect(Object.getOwnPropertyDescriptor(snapshot, 'activeSession')?.writable).toBe(false)
    expect(Object.getOwnPropertyDescriptor(snapshot, 'gatewayStatus')?.writable).toBe(false)
    expect(() => {
      ;(snapshot as { activeSession: string | null }).activeSession = 'sess-2'
    }).toThrow(TypeError)
    expect(host.state.getSnapshot().activeSession).toBe('sess-1')
  })
})

describe('host.navigate', () => {
  it('hands the path to the registered navigator', () => {
    const navigator = vi.fn()
    registerHostNavigator(navigator)

    host.navigate('/todos/ABC-1')

    expect(navigator).toHaveBeenCalledWith('/todos/ABC-1')
  })

  it('fails with a named SDK error when the app has registered no navigator', () => {
    expect(() => host.navigate('/todos')).toThrow(PluginSdkError)
    expect(() => host.navigate('/todos')).toThrow(/\[plugin-sdk]/)
  })
})

describe('host.notify', () => {
  it('delivers the message and level to a registered sink', () => {
    const sink = vi.fn()
    registerHostNotificationSink(sink)

    host.notify('the watcher stopped', 'error')

    expect(sink).toHaveBeenCalledWith({ title: 'the watcher stopped', level: 'error' })
  })

  it('sends an unlevelled notification as info', () => {
    const sink = vi.fn()
    registerHostNotificationSink(sink)

    host.notify('the watcher started')

    expect(sink).toHaveBeenCalledWith({ title: 'the watcher started', level: 'info' })
  })

  /* The description is the whole reason the object form exists, and the level
   * still defaults, so the surface never has to decide what an absent one meant. */
  it('carries a description through, and still defaults the level', () => {
    const sink = vi.fn()
    registerHostNotificationSink(sink)

    host.notify({ title: 'Import finished', description: '42 messages arrived.' })

    expect(sink).toHaveBeenCalledWith({
      title: 'Import finished',
      description: '42 messages arrived.',
      level: 'info',
    })
  })

  it('does not surface a throwing sink to the plugin that called it', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {})
    registerHostNotificationSink(() => {
      throw new Error('the notification surface blew up')
    })

    expect(() => host.notify('the watcher stopped', 'error')).not.toThrow()

    expect(consoleError).toHaveBeenCalled()
    expect(String(consoleError.mock.calls[0]?.[0])).toContain('[plugin-sdk]')
    consoleError.mockRestore()
  })

  it('falls back to a tagged console.warn when no surface is mounted, without throwing', () => {
    const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {})

    expect(() => host.notify('the watcher stopped', 'warning')).not.toThrow()

    const logged = String(consoleWarn.mock.calls[0]?.[0])
    expect(logged).toContain('[plugin-sdk]')
    expect(logged).toContain('the watcher stopped')
    expect(logged).toContain('warning')
    consoleWarn.mockRestore()
  })
})
