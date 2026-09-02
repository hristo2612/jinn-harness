import { useEffect, useRef, useState } from "react"
import { keepPreviousData, useQuery } from "@tanstack/react-query"
import { searchGlobal, type GlobalSearchWire, type SearchKind } from "@/lib/search-api"

/** Matches the Todos filter bar, so the two search boxes feel like one thing. */
const DEBOUNCE_MS = 250

/** The typed query settles before it is sent: a fast typist produces one
 *  request, not one per keystroke. */
export function useDebouncedQuery(value: string, delayMs: number = DEBOUNCE_MS): string {
  const [settled, setSettled] = useState(value)
  const timer = useRef<number | null>(null)
  useEffect(() => {
    if (timer.current != null) window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => {
      timer.current = null
      setSettled(value)
    }, delayMs)
    return () => {
      if (timer.current != null) window.clearTimeout(timer.current)
    }
  }, [value, delayMs])
  return settled
}

export interface GlobalSearchRequest {
  query: string
  scope?: SearchKind
  literal: boolean
}

/**
 * The overlay's one read. An empty or whitespace query asks for nothing, and a
 * refetch keeps the last payload on screen so the panes never collapse to their
 * loading state mid-typing.
 */
export function useGlobalSearch({ query, scope, literal }: GlobalSearchRequest) {
  const settled = useDebouncedQuery(query)
  const text = settled.trim()
  return useQuery<GlobalSearchWire>({
    queryKey: ["global-search", text, scope ?? null, literal],
    queryFn: ({ signal }) => searchGlobal({ q: text, ...(scope ? { scope } : {}), literal }, signal),
    enabled: text.length > 0,
    placeholderData: keepPreviousData,
    // A query the grammar rejects is a 400 the operator has to fix, not a
    // transient failure worth asking again for.
    retry: false,
  })
}
