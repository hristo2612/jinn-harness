import { afterEach, describe, expect, it } from "vitest"
import { contributions } from "@/contrib/registry"
import { AREAS } from "@/contrib/types"
import { APP_ROUTES, type AppRouteDescriptor } from "@/lib/app-routes"
import { NAV_ITEMS } from "@/lib/nav"
import { NOT_IN_PROFILE, PLUGINS_NAV_ITEM, providedNavigationFor } from "@/lib/nav-provided"

/** UI-1 arc §9.7 amendment 10 (adaptation 15): the rail derives what is live
 *  from the route table, never from a list of its own. */

const disposers: (() => void)[] = []
afterEach(() => {
  for (const dispose of disposers.splice(0)) dispose()
})

/** The old gateway's table: every rail destination rendered, More included. */
const FULL_TABLE: AppRouteDescriptor[] = [
  ...NAV_ITEMS.map((item) => ({ id: item.label.toLowerCase(), path: item.href, availability: "always" as const, surface: item.label.toLowerCase() })),
  { id: "more", path: "/more", availability: "always", surface: "more" },
  { id: "settings-plugins", path: "/settings/plugins", availability: "always", surface: "settings-plugins" },
]

function providedHrefs(routes: readonly AppRouteDescriptor[]): string[] {
  return providedNavigationFor(false, routes).items.filter((item) => item.provided).map((item) => item.href)
}

describe("providedNavigationFor at the shipped route table", () => {
  it("lists every ported destination plus Plugins, right after Settings", () => {
    const hrefs = providedNavigationFor(false).items.map((item) => item.href)
    expect(hrefs).toEqual([...NAV_ITEMS.map((item) => item.href).slice(0, -1), "/settings", PLUGINS_NAV_ITEM.href])
    expect(PLUGINS_NAV_ITEM).toMatchObject({ href: "/settings/plugins", label: "Plugins" })
  })

  it("marks exactly Settings and Plugins provided; a redirect at / never provides Chat", () => {
    expect(APP_ROUTES.some((route) => route.path === "/" && route.id === "root-redirect")).toBe(true)
    expect(providedHrefs(APP_ROUTES)).toEqual(["/settings", "/settings/plugins"])
  })

  it("names the reason an absent destination shows", () => {
    expect(NOT_IN_PROFILE).toBe("not in this profile")
  })

  it("puts the provided overflow surfaces in the More slot on mobile, absent primaries disabled", () => {
    const mobile = providedNavigationFor(false).mobileItems
    expect(mobile.map((item) => [item.href, item.provided])).toEqual([
      ["/", false],
      ["/todos", false],
      ["/workflow", false],
      ["/settings", true],
      ["/settings/plugins", true],
    ])
  })
})

describe("providedNavigationFor derives from the table it is given (the mutant: a hardcoded list goes red here)", () => {
  it("provides one more destination when the table renders one more surface", () => {
    const withTodos: AppRouteDescriptor[] = [
      ...APP_ROUTES,
      { id: "todos", path: "/todos", availability: "always", surface: "todos" },
    ]
    expect(providedHrefs(withTodos)).toEqual(["/todos", "/settings", "/settings/plugins"])
  })

  it("keeps Plugins listed but not provided when the table does not render it", () => {
    const withoutPlugins = APP_ROUTES.filter((route) => route.id !== "settings-plugins")
    const items = providedNavigationFor(false, withoutPlugins).items
    expect(items.find((item) => item.href === "/settings/plugins")?.provided).toBe(false)
    expect(providedHrefs(withoutPlugins)).toEqual(["/settings"])
  })

  it("never provides a surface through the plugin splat", () => {
    expect(APP_ROUTES.some((route) => route.path === "/*")).toBe(true)
    expect(providedHrefs(APP_ROUTES)).not.toContain("/todos")
  })

  it("gives the verbatim mobile bar back for a table that renders More", () => {
    const mobile = providedNavigationFor(false, FULL_TABLE).mobileItems
    expect(mobile.map((item) => item.href)).toEqual(["/", "/todos", "/workflow", "/more"])
    expect(mobile.every((item) => item.provided)).toBe(true)
  })

  it("treats a contributed row as provided by the plugin that contributed it", () => {
    disposers.push(contributions.register({ id: "inbox-demo:nav", area: AREAS.sidebarNav, data: { href: "/inbox-demo", label: "Inbox Demo" } }, "plugin:inbox-demo"))
    expect(providedHrefs(APP_ROUTES)).toEqual(["/settings", "/settings/plugins", "/inbox-demo"])
  })
})
