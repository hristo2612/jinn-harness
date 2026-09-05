import { describe, it, expect } from "vitest"
import { render, screen, fireEvent } from "@testing-library/react"
import { MemoryRouter, useLocation } from "react-router-dom"
import { MobileTabBar } from "../mobile-tab-bar"
import { NOT_IN_PROFILE } from "@/lib/nav-provided"

/** UI-1 arc §9.7 amendment 10 (adaptation 15): the mobile bar at the SHIPPED
 *  route table — the absent primaries disabled, the provided overflow surfaces
 *  as direct tabs in the More slot, no tab redirecting anywhere. */

function Location() {
  return <output data-testid="location">{useLocation().pathname}</output>
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <MobileTabBar />
      <Location />
    </MemoryRouter>,
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
})
