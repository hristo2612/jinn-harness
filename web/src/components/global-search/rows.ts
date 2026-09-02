import type { GlobalSearchResultWire, SearchKind } from "@/lib/search-api"
import { KIND_META } from "./kind-meta"
import type { RecentItem } from "./recents"

/**
 * One selectable line in the list pane. The two things the list ever shows — a
 * live result set, and the recents an empty query falls back to — share this
 * shape, so ↑↓, ⏎ and the preview never ask which state is on screen.
 */
export type SearchRow =
  | { key: string; group: string; kind: SearchKind; result: GlobalSearchResultWire }
  | { key: string; group: string; kind: "recent"; recent: RecentItem }

/** `results` is flat and already ranked; a group is a run of one kind. */
export function resultRows(results: readonly GlobalSearchResultWire[] | undefined): SearchRow[] {
  return (results ?? []).map(result => ({
    key: `${result.kind}:${result.id}`,
    group: KIND_META[result.kind].plural,
    kind: result.kind,
    result,
  }))
}

export function recentRows(items: readonly RecentItem[]): SearchRow[] {
  return items.map(item => ({ key: `recent:${item.id}`, group: "Recent", kind: "recent" as const, recent: item }))
}

/** What ⏎ opens, and what gets written back to recents. */
export function rowTarget(row: SearchRow): RecentItem {
  return row.kind === "recent"
    ? row.recent
    : { id: `${row.kind}-${row.result.id}`, label: row.result.title, href: row.result.url, type: row.kind }
}
