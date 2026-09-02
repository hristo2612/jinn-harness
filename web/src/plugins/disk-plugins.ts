/**
 * The disk door: what turns `~/.jinn/plugins/<id>/` into loaded plugins.
 *
 * The gateway does the discovery — it lists every directory it found and serves
 * the client half of the ones the operator enabled — so this side is a
 * reconciliation, not a second scanner. One pass reads what is served, unloads
 * what is no longer served, and loads the rest. `.plans/plugins.md` §7
 * enumerates the hazards each branch below exists for; every one of them has
 * bitten somebody.
 *
 * Enablement is not decided here or held here. `config.yaml` decides it, and
 * the served list is that decision arriving.
 *
 * UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 12): the daemon serves no
 * client halves — a plugin's shape is a profile entry and the operator API
 * writes config only (FINDINGS #37 / KG-1, PLA-348) — so the door resolves
 * EMPTY client-side and issues no request. The reconcile that read the old
 * gateway's plugin listing is gone with the route; what stays is the fact the
 * contributed route waits on: a pass SETTLES. The user-facing extension tier
 * that replaces this door is the arc's later phase (§3), not this packet.
 */

/**
 * Whether the first reconcile has finished.
 *
 * A deep link to a contributed page is rendered before any plugin has loaded,
 * and a host that answered "no such route" in that window would bounce every
 * plugin bookmark to chat. This is what lets it wait instead.
 */
let settled = false
const settledListeners = new Set<() => void>()

export function diskPluginsSettled(): boolean {
  return settled
}

export function subscribeDiskPluginsSettled(listener: () => void): () => void {
  settledListeners.add(listener)
  return () => void settledListeners.delete(listener)
}

/** Announced once. A pass that fails still settles: "we looked" is the fact the
 *  waiting side needs, not "we found something". */
function markSettled(): void {
  if (settled) return
  settled = true
  for (const listener of [...settledListeners]) listener()
}

/** One pass. Nothing is served, so nothing is loaded and nothing is unloaded;
 *  absent is zero plugins, and the pass settles. */
export function scanDiskPlugins(): Promise<void> {
  markSettled()
  return Promise.resolve()
}
