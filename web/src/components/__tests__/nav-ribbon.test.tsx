import { describe, it, expect, vi } from "vitest"
import { render, screen, fireEvent, within } from "@testing-library/react"
import { MemoryRouter } from "react-router-dom"
import { NavRibbon } from "../pill-nav"
import { NAV_ITEMS } from "@/lib/nav"

const prefetchRoute = vi.fn()
vi.mock("@/lib/route-prefetch", () => ({ prefetchRoute: (...args: unknown[]) => prefetchRoute(...args) }))

// Adaptation 15 (docs/plans/ui-malleability-arc.md §9.7 amendment 10): these
// assertions describe the old gateway's rail, where every destination is
// rendered; the route table is pinned to that world so they stand unchanged.
// The shipped table's rail is `nav-ribbon-provided.test.tsx`.
vi.mock("@/lib/app-routes", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/app-routes")>()
  const surface = (path: string) => ({ id: path.slice(1) || "chat", path, availability: "always", surface: path.slice(1) || "chat" })
  return { ...actual, APP_ROUTES: ["/", "/todos", "/notes", "/workflow", "/experiments", "/org", "/cron", "/limits", "/logs", "/skills", "/settings", "/more"].map(surface) }
})

function renderRibbon(props: { listOpen: boolean; path?: string }) {
  return render(
    <MemoryRouter initialEntries={[props.path ?? "/"]}>
      <NavRibbon listOpen={props.listOpen} onToggleList={vi.fn()} />
    </MemoryRouter>,
  )
}

describe("NavRibbon", () => {
  it("renders a brand-only top slot (no fold toggle) when mounted without list props", () => {
    const { container } = render(
      <MemoryRouter initialEntries={["/org"]}>
        <NavRibbon />
      </MemoryRouter>,
    )
    // The global (non-chat) rail has no list to fold → no toggle button.
    expect(screen.queryByLabelText("Show chats")).toBeNull()
    expect(screen.queryByLabelText("Hide chats")).toBeNull()
    // The top slot is a brand mark that links home.
    expect(container.querySelector('a[href="/"]')).toBeTruthy()
  })

  it("renders the toggle with a state-aware label", () => {
    const { rerender } = renderRibbon({ listOpen: true })
    expect(screen.getByLabelText("Hide chats")).toBeTruthy()
    rerender(
      <MemoryRouter initialEntries={["/"]}>
        <NavRibbon listOpen={false} onToggleList={vi.fn()} />
      </MemoryRouter>,
    )
    const toggle = screen.getByLabelText("Show chats")
    expect(toggle.getAttribute("aria-expanded")).toBe("false")
  })

  it("renders every nav item as a labelled link", () => {
    renderRibbon({ listOpen: true })
    for (const item of NAV_ITEMS) {
      const link = screen.getByLabelText(item.label)
      expect(link.getAttribute("href")).toBe(item.href)
    }
  })

  it("prefetches a route when its nav link is hovered", () => {
    renderRibbon({ listOpen: true })
    fireEvent.pointerEnter(screen.getByLabelText("Todos"))
    expect(prefetchRoute).toHaveBeenCalledWith("/todos")
  })

  it("marks the active route with aria-current and a non-accent fill", () => {
    renderRibbon({ listOpen: true, path: "/org" })
    const active = screen.getByLabelText("Organization")
    expect(active.getAttribute("aria-current")).toBe("page")
    // Selection is accent-independent: a soft --fill-secondary, never --accent.
    expect(active.className).toContain("fill-secondary")
    expect(active.className).not.toContain("--accent")
    // A non-active item carries no aria-current.
    expect(screen.getByLabelText("Cron").getAttribute("aria-current")).toBeNull()
  })

  it("leaves the workspace and theme controls to the status bar", () => {
    renderRibbon({ listOpen: true })
    const nav = screen.getByRole("navigation", { name: "Primary" })
    expect(within(nav).queryByLabelText("Switch workspace")).toBeNull()
    expect(within(nav).queryByLabelText(/Theme:/)).toBeNull()
  })

  // Chat icon is OPEN-ONLY: reveals a collapsed list while already on /chat,
  // navigates otherwise, never closes.
  describe("Chat icon open-only behavior", () => {
    function renderWith(opts: { listOpen: boolean; path: string; onToggleList: () => void }) {
      render(
        <MemoryRouter initialEntries={[opts.path]}>
          <NavRibbon listOpen={opts.listOpen} onToggleList={opts.onToggleList} />
        </MemoryRouter>,
      )
      return screen.getByLabelText("Chat")
    }

    it("reveals the list when on /chat with the list hidden", () => {
      const onToggleList = vi.fn()
      const chat = renderWith({ listOpen: false, path: "/", onToggleList })
      fireEvent.click(chat)
      expect(onToggleList).toHaveBeenCalledTimes(1)
    })

    it("is a no-op when the list is already open on /chat", () => {
      const onToggleList = vi.fn()
      const chat = renderWith({ listOpen: true, path: "/", onToggleList })
      fireEvent.click(chat)
      expect(onToggleList).not.toHaveBeenCalled()
    })

    it("navigates (never toggles) when not on /chat", () => {
      const onToggleList = vi.fn()
      const chat = renderWith({ listOpen: false, path: "/org", onToggleList })
      fireEvent.click(chat)
      expect(onToggleList).not.toHaveBeenCalled()
    })

    it("does not hijack modified clicks (new tab / window)", () => {
      const onToggleList = vi.fn()
      const chat = renderWith({ listOpen: false, path: "/", onToggleList })
      fireEvent.click(chat, { metaKey: true })
      expect(onToggleList).not.toHaveBeenCalled()
    })
  })
})
