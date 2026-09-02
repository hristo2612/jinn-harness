/**
 * What a contributed page's path captured, handed down rather than looked up.
 *
 * A plugin reading `window.location` itself would be parsing a URL the host
 * already parsed, against a path grammar only the host knows — and it would be
 * wrong the moment the app is served from anywhere but the root.
 */
import { createContext, useContext, type ReactNode } from 'react'

/** Nothing was captured. Shared so the default context value is stable, and so
 *  a contribution rendered outside a route reads an empty object rather than
 *  a fresh one on every call. */
const NO_PARAMS: Record<string, string> = {}

const RouteParamsContext = createContext<Record<string, string>>(NO_PARAMS)

/** Publishes one contributed page's captured parameters to it. */
export function RouteParamsProvider({
  params,
  children,
}: {
  params: Record<string, string>
  children: ReactNode
}) {
  return <RouteParamsContext.Provider value={params}>{children}</RouteParamsContext.Provider>
}

/** The parameters the current contributed route captured, keyed by the names
 *  its path declared. Empty outside one, so a status-bar chip asking the same
 *  question gets an answer instead of a throw. */
export function useRouteParams(): Record<string, string> {
  return useContext(RouteParamsContext)
}
