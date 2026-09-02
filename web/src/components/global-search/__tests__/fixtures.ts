import type {
  GlobalSearchResultWire, GlobalSearchWire, QueryFacetWire, SearchKind, SearchMatchReasonWire,
} from "@/lib/search-api"

/** Names here are invented: nothing under `packages/` may carry a real one. */
export function searchResult(
  over: Partial<GlobalSearchResultWire> & { kind: SearchKind; id: string },
): GlobalSearchResultWire {
  const url = `/${over.kind}/${over.id}`
  return {
    title: `${over.id} row`,
    url,
    reason: [{ field: "body", snippet: "a <mark>match</mark> here" }],
    preview: { title: `${over.id} row`, excerpt: "a match here", url },
    ...over,
  }
}

export function searchResponse(over: Partial<GlobalSearchWire> = {}): GlobalSearchWire {
  return {
    query: "match",
    parsed: { facets: [], freeText: "match", literal: false },
    results: [],
    counts: { todo: 0, session: 0, note: 0, employee: 0, cron: 0, skill: 0, page: 0 },
    truncated: [],
    ...over,
  }
}

export function facet(over: Partial<QueryFacetWire> & { span: QueryFacetWire["span"] }): QueryFacetWire {
  return { kind: "status", value: "blocked", origin: "inferred", ...over }
}

export function reason(over: Partial<SearchMatchReasonWire> = {}): SearchMatchReasonWire {
  return { field: "body", snippet: "a <mark>match</mark> here", ...over }
}
