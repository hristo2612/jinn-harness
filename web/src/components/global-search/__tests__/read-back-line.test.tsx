import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { GlobalSearchWire } from "@/lib/search-api"
import { ReadBackLine } from "../read-back-line"
import { facet } from "./fixtures"

function renderLine(parsed: GlobalSearchWire["parsed"]) {
  const onRemoveFacet = vi.fn()
  const onToggleLiteral = vi.fn()
  const view = render(<ReadBackLine parsed={parsed} onRemoveFacet={onRemoveFacet} onToggleLiteral={onToggleLiteral} />)
  return { ...view, onRemoveFacet, onToggleLiteral }
}

describe("ReadBackLine", () => {
  it("renders a guessed facet as a chip the operator can drop", () => {
    const guessed = facet({ span: { start: 14, end: 21, text: "blocked" } })
    const { onRemoveFacet } = renderLine({ facets: [guessed], freeText: "opens search", literal: false })

    const chip = screen.getByTestId("search-facet-status")
    expect(chip.getAttribute("data-origin")).toBe("inferred")
    expect(chip.textContent).toContain("blocked")
    expect(chip.textContent).toContain("×")

    fireEvent.click(chip)
    expect(onRemoveFacet).toHaveBeenCalledWith(guessed)
  })

  it("renders a typed token as committed, and not as something to click away", () => {
    renderLine({
      facets: [facet({ kind: "assignee", value: "a-lead", origin: "token", span: { start: 0, end: 7, text: "@a-lead" } })],
      freeText: "",
      literal: false,
    })

    const chip = screen.getByTestId("search-facet-assignee")
    expect(chip.getAttribute("data-origin")).toBe("token")
    expect(chip.tagName).toBe("SPAN")
    expect(chip.textContent).not.toContain("×")
  })

  it("tells the two origins apart visually", () => {
    renderLine({
      facets: [
        facet({ span: { start: 0, end: 7, text: "blocked" } }),
        facet({ kind: "assignee", value: "a-lead", origin: "token", span: { start: 8, end: 15, text: "@a-lead" } }),
      ],
      freeText: "",
      literal: false,
    })

    const inferred = screen.getByTestId("search-facet-status").className
    const committed = screen.getByTestId("search-facet-assignee").className
    expect(inferred).toContain("--fill-tertiary")
    expect(committed).toContain("--accent-fill")
  })

  it("reads back what the free text was, once the facets are taken out", () => {
    renderLine({ facets: [facet({ span: { start: 14, end: 21, text: "blocked" } })], freeText: "opens search", literal: false })

    expect(screen.getByTestId("search-readback").textContent).toContain("opens search")
  })

  it("reflects the gateway's literal verdict and drops the chips with it", () => {
    renderLine({ facets: [], freeText: "is:nonsense", literal: true })

    expect(screen.getByTestId("search-readback-literal").textContent).toBe("Read as literal text")
    expect(screen.queryByTestId("search-facet-status")).toBeNull()
    expect(screen.getByTestId("search-literal-toggle").getAttribute("aria-pressed")).toBe("true")
  })

  it("offers the literal override as an affordance, not only as a shortcut", () => {
    const { onToggleLiteral } = renderLine({ facets: [], freeText: "opens", literal: false })

    const toggle = screen.getByTestId("search-literal-toggle")
    expect(toggle.getAttribute("aria-pressed")).toBe("false")
    expect(toggle.textContent).toContain("⌘⏎")

    fireEvent.click(toggle)
    expect(onToggleLiteral).toHaveBeenCalledTimes(1)
  })
})
