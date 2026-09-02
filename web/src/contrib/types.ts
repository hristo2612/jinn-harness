import type { ReactNode } from "react"

/**
 * Where a contribution came from. The host stamps it — `"core"` for the app's
 * own surfaces, `plugin:<id>` for a loaded plugin — so a contribution cannot
 * claim provenance it does not have. It is a tag for display and for the
 * capability gate that comes later, not a gate today.
 */
export type ContributionSource = "core" | `plugin:${string}`

/**
 * One thing added to one place in the UI.
 *
 * There is deliberately no `enabled` field: `when` already expresses a soft
 * hide, and plugin-level enablement belongs to the gateway, decided in
 * `config.yaml`. One mechanism, not two.
 */
export interface Contribution {
  /** Unique within its area — re-registering the same id replaces that entry. */
  id: string
  /** Dotted area id, from {@link AREAS}. */
  area: string
  /** Ascending sort key. Ties keep insertion order. */
  order?: number
  /**
   * Visibility predicate, evaluated when the area's snapshot is rebuilt — that
   * is, on a register or a removal IN THAT AREA, never reactively. Visibility
   * that follows external state belongs in a component that returns null.
   */
  when?: () => boolean
  /** UI contributions. Called inside the contribution's own error boundary. */
  render?: () => ReactNode
  /** Declarative payload for data contributions; the shape is defined per area. */
  data?: unknown
}

/** A contribution as the registry hands it back, with its provenance stamped on. */
export interface ResolvedContribution extends Contribution {
  source: ContributionSource
}

/**
 * The v1 area ids. `todoDetailActions` and `todoDetailSections` are the two ends
 * of one page-scoped surface, a header and a body.
 *
 * The ids live here so the two sides cannot spell them differently; the SDK
 * declares the same values for plugins, and `sdk-contract.test.ts` holds the
 * two declarations to the same set.
 */
export const AREAS = {
  routes: "routes",
  sidebarNav: "sidebar.nav",
  statusbarRight: "statusbar.right",
  todoDetailActions: "todo.detail.actions",
  todoDetailSections: "todo.detail.sections",
  chatComposer: "chat.composer",
  homeWidgets: "home.widgets",
} as const
