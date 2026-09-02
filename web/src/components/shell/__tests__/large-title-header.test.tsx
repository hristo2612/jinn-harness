import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { LargeTitleHeader } from "../large-title-header"
import { PageScaffold } from "../page-scaffold"

describe("LargeTitleHeader", () => {
  it("renders the large title in the scroll flow and the inline title in the sticky bar", () => {
    render(
      <PageScaffold header={<LargeTitleHeader title="Settings" subtitle="Portal, gateway and connectors" />}>
        <p>body</p>
      </PageScaffold>,
    )

    const large = document.querySelector(".jinn-large-title")
    const inline = document.querySelector(".jinn-inline-title")
    expect(large).toBeTruthy()
    expect(inline).toBeTruthy()
    expect(large?.textContent).toContain("Settings")
    expect(inline?.textContent).toContain("Settings")
    expect(screen.getByText("Portal, gateway and connectors")).toBeTruthy()
    expect(document.querySelector(".jinn-inline-title")?.contains(screen.getByText("Portal, gateway and connectors"))).toBe(false)
  })

  it("accepts a node title so a control can occupy the title slot", () => {
    render(
      <LargeTitleHeader title={<button type="button" data-testid="title-control">Home</button>} />,
    )
    expect(screen.getByTestId("title-control").textContent).toBe("Home")
    expect(document.querySelector(".jinn-large-title")).toBeTruthy()
  })

  it("renders trailing in the sticky bar", () => {
    render(
      <LargeTitleHeader title="Plugins" trailing={<button type="button" aria-label="Rescan">go</button>} />,
    )
    expect(screen.getByRole("button", { name: "Rescan" })).toBeTruthy()
  })

  // Non-collapse is the rule for an externally scrolled route, not a shortfall
  // of one — see the `scroll="external"` clause in
  // docs/design/jinn-shell-contract.md.
  it("scroll=external keeps the large title: no inline bar, no scaffold scroll box", () => {
    const { container } = render(
      <PageScaffold scroll="external" header={<LargeTitleHeader title="Todos" />}>
        <div />
      </PageScaffold>,
    )
    expect(document.querySelector(".jinn-large-title")).toBeTruthy()
    expect(document.querySelector(".jinn-inline-title")).toBeNull()
    expect(container.querySelector("[data-scrollable]")).toBeNull()
  })

  it("sticky bar uses material with no hairline class", () => {
    const { container } = render(
      <PageScaffold header={<LargeTitleHeader title="More" />}>
        <p>body</p>
      </PageScaffold>,
    )
    const bar = container.querySelector("[data-slot='large-title-bar']")
    expect(bar?.className).toContain("--material-thick")
    expect(bar?.className).not.toMatch(/border/)
  })
})
