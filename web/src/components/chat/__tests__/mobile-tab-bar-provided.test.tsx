import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { readFileSync } from "node:fs"
import { describe, it, expect } from "vitest"
import { render, screen, fireEvent, within } from "@testing-library/react"
import { MemoryRouter, useLocation } from "react-router-dom"
import { MobileTabBar } from "../mobile-tab-bar"
import { NOT_IN_PROFILE } from "@/lib/nav-provided"

/** UI-1 arc §9.7 amendment 10 (adaptation 15): the mobile bar at the SHIPPED
 *  route table — the absent primaries disabled, the provided overflow surfaces
 *  as direct tabs in the More slot, no tab redirecting anywhere. */

/** The palette's one home is `routes/globals.css`; the contrast below is
 *  computed from the shipped tokens, not a fixture. Dark is the
 *  `:root, [data-theme="dark"]` block, light the `[data-theme="light"]` one. */
const GLOBALS_CSS = readFileSync("src/routes/globals.css", "utf8")
const THEMES = [
  ["dark", ':root, [data-theme="dark"]'],
  ["light", '[data-theme="light"]'],
] as const

function themeTokens(selector: string): Record<string, string> {
  const start = GLOBALS_CSS.indexOf(`${selector} {`)
  if (start < 0) throw new Error(`no ${selector} block in globals.css`)
  const block = GLOBALS_CSS.slice(start, GLOBALS_CSS.indexOf("}", start))
  return Object.fromEntries([...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]))
}

type Rgba = [number, number, number, number]

function parseColor(value: string): Rgba {
  const hex = value.match(/^#([0-9a-f]{6})$/i)
  if (hex) return [parseInt(hex[1].slice(0, 2), 16), parseInt(hex[1].slice(2, 4), 16), parseInt(hex[1].slice(4, 6), 16), 1]
  const rgba = value.match(/^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([\d.]+))?\s*\)$/)
  if (!rgba) throw new Error(`unparsed colour ${value}`)
  return [Number(rgba[1]), Number(rgba[2]), Number(rgba[3]), rgba[4] === undefined ? 1 : Number(rgba[4])]
}

function over(fg: Rgba, bg: Rgba): Rgba {
  const a = fg[3]
  return [bg[0] + a * (fg[0] - bg[0]), bg[1] + a * (fg[1] - bg[1]), bg[2] + a * (fg[2] - bg[2]), 1]
}

function luminance([r, g, b]: Rgba): number {
  const lin = (c: number) => {
    const s = c / 255
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

function contrast(fg: Rgba, bg: Rgba): number {
  const [hi, lo] = [luminance(fg), luminance(bg)].sort((a, b) => b - a)
  return (hi + 0.05) / (lo + 0.05)
}

const TEXT_TOKEN = /(?:^|\s)text-\[var\(--(text-\w+)\)\]/

/** What the caption composites to on the bar, as the browser paints it: its
 *  own text token (else the tab's inherited one) with its alpha multiplied by
 *  every `opacity-40` between it and the nav, over the bar's opaque material
 *  (the coarse-pointer bar at 390 px). WCAG 2 relative luminance. */
function captionContrast(caption: HTMLElement, tab: HTMLElement, theme: Record<string, string>): number {
  const token = (caption.className.match(TEXT_TOKEN) ?? tab.className.match(TEXT_TOKEN))?.[1]
  if (!token) throw new Error("the caption has no text token")
  let color = parseColor(theme[token])
  for (let el: HTMLElement | null = caption; el && el !== tab.parentElement; el = el.parentElement) {
    if (/(?:^|\s)opacity-40(?:\s|$)/.test(el.className)) color = [color[0], color[1], color[2], color[3] * 0.4]
  }
  const bar = parseColor(theme["material-thick-opaque"])
  return contrast(over(color, bar), bar)
}

function Location() {
  return <output data-testid="location">{useLocation().pathname}</output>
}

function renderAt(path: string) {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MemoryRouter initialEntries={[path]}>
      <MobileTabBar />
      <Location />
    </MemoryRouter></QueryClientProvider>,
  )
}

describe("MobileTabBar at the shipped route table", () => {
  it("carries Chat, Todos and Workflows disabled and Settings, Plugins as links; no More", () => {
    renderAt("/settings")
    expect(screen.getAllByRole("link").map((tab) => tab.getAttribute("aria-label"))).toEqual(["Chat", "Todos", "Workflows", "Settings", "Plugins"])
    for (const name of ["Chat", "Todos", "Workflows"]) {
      const tab = screen.getByRole("link", { name })
      expect(tab.getAttribute("aria-disabled")).toBe("true")
      expect(tab.getAttribute("href")).toBeNull()
      expect(tab.getAttribute("title")).toBe(NOT_IN_PROFILE)
    }
    expect(screen.getByRole("link", { name: "Plugins" }).getAttribute("href")).toBe("/settings/plugins")
    expect(screen.queryByRole("link", { name: "More" })).toBeNull()
  })

  it("navigates nowhere when a disabled tab is tapped", () => {
    renderAt("/settings")
    fireEvent.click(screen.getByRole("link", { name: "Todos" }))
    expect(screen.getByTestId("location").textContent).toBe("/settings")
  })

  it("lights Plugins alone on /settings/plugins and keeps the ≥49px target", () => {
    renderAt("/settings/plugins")
    const plugins = screen.getByRole("link", { name: "Plugins" })
    expect(plugins.getAttribute("aria-current")).toBe("page")
    expect(screen.getByRole("link", { name: "Settings" }).getAttribute("aria-current")).toBeNull()
    expect(plugins.className).toContain("min-h-[49px]")
  })

  it("shows an absent tab's reason as visible text a finger can reach, focusable, and a tap stays put (390 px)", () => {
    // Taste §2: mobile is first-class — a `title` is a hover-ism no touch
    // ever sees, so it is never the reason on this surface.
    window.innerWidth = 390
    renderAt("/settings")
    const todos = screen.getByRole("link", { name: "Todos" })
    const reason = within(todos).getByText(NOT_IN_PROFILE)
    expect(reason.getAttribute("aria-hidden")).toBeNull()
    expect(reason.id).not.toBe("")
    // The visible text is also the control's accessible description.
    expect(todos.getAttribute("aria-describedby")).toBe(reason.id)
    // Disabled yet focusable: a keyboard or a switch reaches the reason too.
    expect(todos.getAttribute("aria-disabled")).toBe("true")
    todos.focus()
    expect(document.activeElement).toBe(todos)
    // A tap navigates nowhere; the target keeps its height.
    fireEvent.click(todos)
    expect(screen.getByTestId("location").textContent).toBe("/settings")
    expect(todos.className).toContain("min-h-[49px]")
    // Adaptation 16: live labels make agent-authored renames visible on touch.
    expect(screen.getByRole("link", { name: "Settings" }).textContent).toBe("Settings")
    // A reason a finger can reach but cannot read is not delivered: composited
    // on the bar it clears AA text (≥ 4.5:1) in BOTH themes. Round 2 shipped it
    // at 1.46:1 dark / 1.58:1 light — text-tertiary under the tab's opacity-40.
    for (const [name, selector] of THEMES) {
      expect(captionContrast(reason, todos, themeTokens(selector)), `${name} caption contrast`).toBeGreaterThanOrEqual(4.5)
    }
    // Structurally: only the glyph dims. The caption sits outside every
    // opacity-reduced ancestor and carries the secondary token itself.
    for (let el: HTMLElement | null = reason; el && el !== todos.parentElement; el = el.parentElement) {
      expect(el.className, `opacity on <${el.tagName.toLowerCase()} id=${el.id}>`).not.toMatch(/(?:^|\s)opacity-/)
    }
    expect(reason.className).toMatch(/(?:^|\s)text-\[var\(--text-secondary\)\]/)
  })
})
