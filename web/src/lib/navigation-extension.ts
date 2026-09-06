import type { ProvidedNavigation, ProvidedNavItem } from "./nav-provided"

export const NAVIGATION_TOPIC = "jinn:ui/after-build-navigation"
export const NAVIGATION_ENTRY = "ext-navigation"
export const NAVIGATION_QUERY_KEY = ["navigation-moment"] as const

// Stored source, executed only by Boa. There is no preset branch in the shell.
export const TOOLS_FIRST_SOURCE = `p => {
  const focus = xs => xs.filter(x => x.provided)
    .map(x => ({...x, label: x.id === '/settings/plugins' ? 'My tools' : x.label}))
    .sort((a,b) => Number(b.id === '/settings/plugins') - Number(a.id === '/settings/plugins'));
  return {...p, items: focus(p.items), mobileItems: focus(p.mobileItems)};
}`

export function navigationPayload(base: ProvidedNavigation) {
  const descriptors = (items: ProvidedNavItem[]) => items.map(({ href, label, provided }) => ({ id: href, label, provided }))
  return { items: descriptors(base.items), mobileItems: descriptors(base.mobileItems) }
}

function foldItems(input: ProvidedNavItem[], output: unknown): ProvidedNavItem[] {
  if (!Array.isArray(output)) throw new Error("Navigation result must contain destination lists")
  const seen = new Set<string>()
  const items = output.map((value: unknown) => {
    if (!value || typeof value !== "object") throw new Error("Invalid navigation destination")
    const { id, label } = value as { id?: unknown; label?: unknown }
    const original = input.find(item => item.href === id)
    if (!original || seen.has(original.href)) throw new Error("Unknown or repeated navigation destination")
    if (typeof label !== "string" || !label.trim() || [...label].length > 40 || /[<>\p{Cc}\p{Cf}]/u.test(label)) {
      throw new Error("Navigation labels must be plain text, 1–40 characters")
    }
    seen.add(original.href)
    return { ...original, label }
  })
  for (const recovery of ["/settings", "/settings/plugins"]) {
    if (input.some(item => item.href === recovery) && !seen.has(recovery)) throw new Error("Navigation must keep Settings and Plugins reachable")
  }
  return items
}

export function foldNavigation(base: ProvidedNavigation, output: unknown): ProvidedNavigation {
  if (!output || typeof output !== "object") throw new Error("Invalid navigation result")
  const result = output as { items?: unknown; mobileItems?: unknown }
  return { ...base, items: foldItems(base.items, result.items), mobileItems: foldItems(base.mobileItems, result.mobileItems) }
}

export function navigationDifference(base: ProvidedNavigation, result: ProvidedNavigation): string {
  if (JSON.stringify(navigationPayload(base)) === JSON.stringify(navigationPayload(result))) return "No navigation changes returned."
  return `Desktop: ${result.items.map(x => x.label).join(" → ")}. Phone: ${result.mobileItems.map(x => x.label).join(" → ")}.`
}
