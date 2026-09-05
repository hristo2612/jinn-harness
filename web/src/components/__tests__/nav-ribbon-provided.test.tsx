import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { describe, it, expect, vi } from "vitest"
import { render, screen, fireEvent } from "@testing-library/react"
import { MemoryRouter, useLocation } from "react-router-dom"
import { NavRibbon } from "../pill-nav"
import { NOT_IN_PROFILE } from "@/lib/nav-provided"

const prefetchRoute = vi.fn()
vi.mock("@/lib/route-prefetch", () => ({ prefetchRoute: (...args: unknown[]) => prefetchRoute(...args) }))

/** UI-1 arc §9.7 amendment 10 (adaptation 15): the desktop rail at the SHIPPED
 *  route table — Settings and Plugins live, every other destination disabled
 *  with its reason, and no click on one goes anywhere. */

function Location() {
  return <output data-testid="location">{useLocation().pathname}</output>
}

function renderAt(path: string) {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MemoryRouter initialEntries={[path]}>
      <NavRibbon />
      <Location />
    </MemoryRouter></QueryClientProvider>,
  )
}

describe("NavRibbon at the shipped route table", () => {
  it("renders Settings and Plugins as links, Plugins to /settings/plugins", () => {
    renderAt("/settings")
    expect(screen.getByRole("link", { name: "Settings" }).getAttribute("href")).toBe("/settings")
    expect(screen.getByRole("link", { name: "Plugins" }).getAttribute("href")).toBe("/settings/plugins")
  })

  it("renders an absent destination disabled, with the reason as its title and in its label pill, and no href", () => {
    renderAt("/settings")
    const todos = screen.getByRole("link", { name: "Todos" })
    expect(todos.getAttribute("aria-disabled")).toBe("true")
    expect(todos.getAttribute("href")).toBeNull()
    expect(todos.getAttribute("title")).toBe(NOT_IN_PROFILE)
    expect(todos.textContent).toContain(NOT_IN_PROFILE)
    expect(screen.getByRole("link", { name: "Settings" }).getAttribute("aria-disabled")).toBeNull()
  })

  it("navigates nowhere on a click or a hover of an absent destination", () => {
    renderAt("/settings")
    const chat = screen.getByRole("link", { name: "Chat" })
    fireEvent.pointerEnter(chat)
    fireEvent.click(chat)
    expect(screen.getByTestId("location").textContent).toBe("/settings")
    expect(prefetchRoute).not.toHaveBeenCalledWith("/")
  })

  it("lights Plugins alone on /settings/plugins", () => {
    renderAt("/settings/plugins")
    expect(screen.getByRole("link", { name: "Plugins" }).getAttribute("aria-current")).toBe("page")
    expect(screen.getByRole("link", { name: "Settings" }).getAttribute("aria-current")).toBeNull()
  })

  it("keeps the 44px row for a disabled destination (the Taste floor is 34)", () => {
    renderAt("/settings")
    expect(screen.getByRole("link", { name: "Todos" }).className).toContain("size-11")
  })
})
