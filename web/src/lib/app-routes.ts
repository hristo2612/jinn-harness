import { matchPath } from "react-router-dom"

export type AppRouteAvailability = "always" | "notes-enabled" | "development"

export interface AppRouteDescriptor {
  id: string
  path: string
  availability: AppRouteAvailability
  /** The semantic surface an operator names. Redirects name their destination. */
  surface: string
}

/**
 * The paths the shipped router can render. Route elements stay in `main.tsx`,
 * but their identities live here so Talk coverage and the router consume the
 * same list instead of maintaining two uncheckable copies.
 */
export const APP_ROUTES = [
  // UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 5): Settings, Plugins, the
  // plugin splat, and two router-level redirects. Every other route is absent.
  { id: "root-redirect", path: "/", availability: "always", surface: "settings" },
  { id: "more-redirect", path: "/more", availability: "always", surface: "settings" },
  { id: "settings-plugins", path: "/settings/plugins", availability: "always", surface: "settings-plugins" },
  { id: "settings", path: "/settings", availability: "always", surface: "settings" },
  { id: "plugin-contributed", path: "/*", availability: "always", surface: "plugin" },
] as const satisfies readonly AppRouteDescriptor[]

export type AppRouteId = (typeof APP_ROUTES)[number]["id"]

/** First concrete match wins; the plugin splat is intentionally last. */
export function matchAppRoute(pathname: string): (typeof APP_ROUTES)[number] | undefined {
  return APP_ROUTES.find((route) => matchPath({ path: route.path, end: true }, pathname) !== null)
}
