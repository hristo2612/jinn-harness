import { createContext, useCallback, useContext, useMemo, useRef, useState } from "react"
import type { SearchKind } from "@/lib/search-api"

export interface SearchOverlayRequest {
  /** Distinct per call, so asking twice in a row opens twice. */
  id: number
  scope?: SearchKind
  query?: string
}

interface SearchOverlay {
  request: SearchOverlayRequest | null
  openSearch: (options?: { scope?: SearchKind; query?: string }) => void
}

/** A surface rendered outside the app shell has no overlay to reach, so the
 *  default opener is a no-op — unit-rendering a filter row is not a bug. */
const SearchOverlayContext = createContext<SearchOverlay>({ request: null, openSearch: () => {} })

/** Opening the palette with a scope needs a typed call: a synthesized Cmd-K
 *  keydown, the only channel that existed before, cannot carry one. */
export function SearchOverlayProvider({ children }: { children: React.ReactNode }) {
  const [request, setRequest] = useState<SearchOverlayRequest | null>(null)
  const lastId = useRef(0)
  const openSearch = useCallback((options?: { scope?: SearchKind; query?: string }) => {
    lastId.current += 1
    setRequest({ id: lastId.current, scope: options?.scope, query: options?.query })
  }, [])
  const value = useMemo(() => ({ request, openSearch }), [request, openSearch])
  return <SearchOverlayContext.Provider value={value}>{children}</SearchOverlayContext.Provider>
}

export function useSearchOverlay(): SearchOverlay {
  return useContext(SearchOverlayContext)
}
