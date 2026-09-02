import type { Contribution, ContributionSource, ResolvedContribution } from "./types"

type Listener = () => void

/** One frozen array for every empty area, so an empty read is stable too. */
export const NO_CONTRIBUTIONS: readonly ResolvedContribution[] = Object.freeze([])

/**
 * The one registry every area reads from, keyed by dotted area id.
 *
 * Snapshots are cached per area because a read has to be referentially stable:
 * hand `useSyncExternalStore` a fresh array on every read and React re-renders
 * forever. Invalidation is area-scoped for a related reason — registering a
 * status-bar chip must not re-render the routes area — which makes it a
 * requirement rather than an optimization.
 *
 * The shape follows `components/chat/tts-controller.ts:60`: a class with
 * arrow-field methods and a module singleton, so the methods can be handed
 * straight to a hook.
 */
class ContributionRegistry {
  private readonly byArea = new Map<string, ResolvedContribution[]>()
  private readonly snapshots = new Map<string, readonly ResolvedContribution[]>()
  private readonly areaListeners = new Map<string, Set<Listener>>()

  /**
   * Register one contribution. Returns a disposer that removes it.
   *
   * `source` is what the caller is, not what the contribution claims: the app's
   * own surfaces leave it at `"core"`, and a plugin never calls this directly —
   * it gets a context whose `contribute` supplies its own tag.
   */
  register = (
    contribution: Contribution,
    source: ContributionSource = "core",
  ): (() => void) => this.registerMany([contribution], source)

  /**
   * Register several at once. Returns a disposer that removes all of them.
   * A batch notifies each area it touched exactly once, not once per entry.
   */
  registerMany = (
    contributions: readonly Contribution[],
    source: ContributionSource = "core",
  ): (() => void) => {
    // The disposer closes over the entries themselves, not their ids: an id that
    // has since been re-registered belongs to somebody else's entry, and a stale
    // disposer that removed it would take down the live registration instead.
    const registered = contributions.map((contribution) => this.put(contribution, source))
    this.invalidate(contributions.map((contribution) => contribution.area))

    return () => this.remove(registered)
  }

  /** Visible, ordered entries for an area. The same reference until that area mutates. */
  getArea = (area: string): readonly ResolvedContribution[] => {
    const cached = this.snapshots.get(area)
    if (cached) return cached

    const registered = this.byArea.get(area) ?? []
    const visible = registered
      .filter((contribution) => (contribution.when ? contribution.when() : true))
      .sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
    const resolved = visible.length > 0 ? Object.freeze(visible) : NO_CONTRIBUTIONS

    this.snapshots.set(area, resolved)
    return resolved
  }

  /** Subscribe to mutations of ONE area, so a slot re-renders only for its own. */
  subscribeArea = (area: string, listener: Listener): (() => void) => {
    const listeners = this.areaListeners.get(area) ?? new Set<Listener>()
    listeners.add(listener)
    this.areaListeners.set(area, listeners)

    return () => {
      listeners.delete(listener)
      if (listeners.size === 0) this.areaListeners.delete(area)
    }
  }

  /** Insert or replace one entry, without notifying — `registerMany` batches that. */
  private put(contribution: Contribution, source: ContributionSource): ResolvedContribution {
    const registered = this.byArea.get(contribution.area) ?? []
    // Provenance is stamped here and never read off the author's object: a
    // contribution that could name its own source could claim to be core.
    const stamped: ResolvedContribution = { ...contribution, source }
    this.byArea.set(contribution.area, [
      ...registered.filter((entry) => entry.id !== contribution.id),
      stamped,
    ])
    return stamped
  }

  private remove(entries: readonly ResolvedContribution[]): void {
    const emptied: string[] = []
    for (const entry of entries) {
      if (this.take(entry)) emptied.push(entry.area)
    }
    // Nothing left to take means a second disposal, which notifies nobody.
    if (emptied.length > 0) this.invalidate(emptied)
  }

  /** Drop one entry without notifying. Answers whether it was there to drop. */
  private take(entry: ResolvedContribution): boolean {
    const registered = this.byArea.get(entry.area)
    if (!registered) return false

    const remaining = registered.filter((candidate) => candidate !== entry)
    if (remaining.length === registered.length) return false

    if (remaining.length > 0) this.byArea.set(entry.area, remaining)
    else this.byArea.delete(entry.area)
    return true
  }

  private invalidate(areas: readonly string[]): void {
    for (const area of new Set(areas)) {
      this.snapshots.delete(area)

      const listeners = this.areaListeners.get(area)
      if (!listeners) continue
      // Copied, because a listener may unsubscribe itself while being notified.
      for (const listener of [...listeners]) listener()
    }
  }
}

export const contributions = new ContributionRegistry()
