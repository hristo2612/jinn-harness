import { notifyEach } from './listeners'

/**
 * A gateway frame as a plugin sees it. The host's own typed event union stays
 * out of the contract: it lives in a workspace package a plugin has no way to
 * install, and it grows every release. Plugins narrow `payload` themselves.
 */
export interface HostEvent {
  readonly event: string
  readonly payload: unknown
}

export type HostEventHandler = (frame: HostEvent) => void

const handlersByType = new Map<string, Set<HostEventHandler>>()

/**
 * Subscribe to one event type. Many handlers per type: one-per-type would let
 * a plugin evict another plugin's handler by registering after it. Returns the
 * unsubscribe, which is the whole reason this is a set and not a map of one.
 */
export function onHostEvent(type: string, handler: HostEventHandler): () => void {
  const handlers = handlersByType.get(type) ?? new Set<HostEventHandler>()
  handlers.add(handler)
  handlersByType.set(type, handlers)
  return () => {
    handlers.delete(handler)
    if (handlers.size === 0) handlersByType.delete(type)
  }
}

/** App-side writer: every frame off the app's one gateway socket arrives here. */
export function dispatchHostEvent(frame: HostEvent): void {
  const handlers = handlersByType.get(frame.event)
  if (!handlers) return
  notifyEach(handlers, frame, `host.onEvent("${frame.event}")`)
}

/** Test-only: drop every subscription so a suite starts from a known state. */
export function resetHostEvents(): void {
  handlersByType.clear()
}
