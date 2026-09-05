import { Puzzle } from "lucide-react"
import { contributions } from "@/contrib/registry"
import { AREAS } from "@/contrib/types"
import { APP_ROUTES, type AppRouteDescriptor } from "./app-routes"
import { MORE_NAV_ITEM, navigationFor, type NavItem } from "./nav"

/**
 * The rail, derived (docs/plans/ui-malleability-arc.md §9.7 amendment 10,
 * adaptation 15). `navigationFor` stays the port's inventory of destinations;
 * this module says which of them THIS bundle renders, reading the route table
 * — the bundle's own statement of its surfaces — never a list of its own. An
 * absent destination is kept and marked, so the rail shows the operator what
 * a fuller profile would offer without pretending to offer it.
 */

/** The reason an absent destination shows: as its title and in its label. */
export const NOT_IN_PROFILE = "not in this profile"

/** The plugins page as a rail item of its own, rather than a link below
 *  Settings' fold — the second surface this bundle actually renders. */
export const PLUGINS_NAV_ITEM: NavItem = { href: "/settings/plugins", label: "Plugins", icon: Puzzle }

export interface ProvidedNavItem extends NavItem {
  provided: boolean
}

export interface ProvidedNavigation {
  items: ProvidedNavItem[]
  mobileItems: ProvidedNavItem[]
  overflowHrefs: string[]
}

/** A redirect names another route's surface (the descriptor's own rule). */
function isRedirect(route: AppRouteDescriptor, routes: readonly AppRouteDescriptor[]): boolean {
  return routes.some((other) => other.id !== route.id && other.id === route.surface)
}

/** The paths the router renders as surfaces: neither a redirect nor the plugin splat. */
export function renderedPaths(routes: readonly AppRouteDescriptor[]): Set<string> {
  return new Set(routes.filter((route) => !isRedirect(route, routes) && !route.path.endsWith("*")).map((route) => route.path))
}

function contributedHrefs(): Set<string> {
  return new Set(
    contributions.getArea(AREAS.sidebarNav).flatMap((contribution) => {
      const href = (contribution.data as { href?: unknown } | undefined)?.href
      return typeof href === "string" ? [href] : []
    }),
  )
}

/**
 * `navigationFor`, each destination marked provided or not by `routes`, with
 * Plugins after Settings. A contributed row is provided by the plugin that
 * contributed it. On mobile the absent primaries stay in their slots,
 * disabled; the More slot is More when the router renders the More screen and
 * the provided overflow surfaces as direct tabs when it does not.
 */
export function providedNavigationFor(notesEnabled: boolean, routes: readonly AppRouteDescriptor[] = APP_ROUTES): ProvidedNavigation {
  const rendered = renderedPaths(routes)
  const contributed = contributedHrefs()
  const mark = (item: NavItem): ProvidedNavItem => ({ ...item, provided: rendered.has(item.href) || contributed.has(item.href) })
  const navigation = navigationFor(notesEnabled)
  const items = navigation.items.flatMap((item) => (item.href === "/settings" ? [mark(item), mark(PLUGINS_NAV_ITEM)] : [mark(item)]))
  const primaries = navigation.mobileItems.filter((item) => item.href !== MORE_NAV_ITEM.href).map(mark)
  const moreSlot = rendered.has(MORE_NAV_ITEM.href)
    ? [mark(MORE_NAV_ITEM)]
    : items.filter((item) => item.provided && !primaries.some((primary) => primary.href === item.href))
  return { items, mobileItems: [...primaries, ...moreSlot], overflowHrefs: navigation.overflowHrefs }
}
