import { notifyEach } from './listeners'

export type GatewayStatus = 'connected' | 'disconnected'

/** The readonly tier of the host API: what the app is currently showing. */
export interface HostState {
  readonly activeSession: string | null
  readonly gatewayStatus: GatewayStatus
}

const INITIAL: HostState = { activeSession: null, gatewayStatus: 'disconnected' }

let snapshot: HostState = Object.freeze({ ...INITIAL })
const listeners = new Set<(state: HostState) => void>()

/**
 * The current state. Identity is stable between publishes on purpose:
 * `useSyncExternalStore` compares snapshots by reference and re-renders forever
 * if a read hands back a fresh object.
 */
export function getHostState(): HostState {
  return snapshot
}

export function subscribeHostState(listener: (state: HostState) => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/** Walks the keys rather than naming them, so a field added later cannot be
 *  forgotten here and publish silently. */
function differsFromSnapshot(next: HostState): boolean {
  return (Object.keys(next) as (keyof HostState)[]).some((key) => next[key] !== snapshot[key])
}

/**
 * App-side writer; the bridge component is its only caller. A publish that
 * changes nothing keeps the existing snapshot and tells nobody — the app
 * republishes on every render pass of its sources, and a subscriber that woke
 * for each of those would be worse than no subscription at all.
 */
export function publishHostState(partial: Partial<HostState>): void {
  const next: HostState = { ...snapshot, ...partial }
  if (!differsFromSnapshot(next)) return
  snapshot = Object.freeze(next)
  notifyEach(listeners, snapshot, 'host.state')
}

/** Test-only: drop back to the nothing-mounted state so a suite starts known. */
export function resetHostState(): void {
  snapshot = Object.freeze({ ...INITIAL })
  listeners.clear()
}
