import { readFileSync, readdirSync } from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen } from "@testing-library/react"
import { contributions } from "../registry"
import { Slot } from "../slot"

let areaCounter = 0
function freshArea(): string {
  areaCounter += 1
  return `slot.area.${areaCounter}`
}

// React logs every error a boundary catches. Silencing it keeps the run
// readable, and the captured first arguments double as the assertion that the
// boundary tagged the failure with its contribution id.
const logged: string[] = []

beforeEach(() => {
  vi.spyOn(console, "error").mockImplementation((...args: unknown[]) => {
    if (typeof args[0] === "string") logged.push(args[0])
  })
})

afterEach(() => {
  vi.restoreAllMocks()
  logged.length = 0
})

describe("Slot error isolation", () => {
  it("contains a render() that throws when it is called", () => {
    const area = freshArea()
    const dispose = contributions.registerMany([
      {
        id: "explodes",
        area,
        order: 0,
        render: () => {
          throw new Error("threw before returning anything")
        },
      },
      { id: "survivor", area, order: 1, render: () => <span>still here</span> },
    ])

    render(<Slot area={area} variant="chip" />)

    expect(screen.getByText("still here")).toBeTruthy()
    expect(screen.getByLabelText("Retry explodes")).toBeTruthy()
    expect(logged).toContain("[contrib:explodes]")

    dispose()
  })

  it("contains a tree that throws while rendering, and names the failure", () => {
    const area = freshArea()
    function Broken(): never {
      throw new Error("the element tree gave up")
    }
    const dispose = contributions.registerMany([
      { id: "broken-tree", area, order: 0, render: () => <Broken /> },
      { id: "neighbour", area, order: 1, render: () => <span>unaffected</span> },
    ])

    render(<Slot area={area} />)

    expect(screen.getByText("unaffected")).toBeTruthy()
    expect(screen.getByText("broken-tree failed to render")).toBeTruthy()
    expect(screen.getByText("the element tree gave up")).toBeTruthy()

    dispose()
  })

  it("re-renders a contribution that has stopped throwing when Retry is clicked", () => {
    const area = freshArea()
    let failing = true
    const dispose = contributions.register({
      id: "flaky",
      area,
      render: () => {
        if (failing) throw new Error("not yet")
        return <span>recovered</span>
      },
    })

    render(<Slot area={area} />)
    expect(screen.getByText("flaky failed to render")).toBeTruthy()

    // Still failing: Retry re-runs it and the fallback comes straight back.
    fireEvent.click(screen.getByRole("button", { name: "Retry" }))
    expect(screen.getByText("flaky failed to render")).toBeTruthy()

    failing = false
    fireEvent.click(screen.getByRole("button", { name: "Retry" }))
    expect(screen.getByText("recovered")).toBeTruthy()
    expect(screen.queryByText("flaky failed to render")).toBeNull()

    dispose()
  })

  it("retries from the chip fallback, which carries the id and the reason", () => {
    const area = freshArea()
    let failing = true
    const dispose = contributions.register({
      id: "chip-fail",
      area,
      render: () => {
        if (failing) throw new Error("chip reason")
        return <span>chip back</span>
      },
    })

    render(<Slot area={area} variant="chip" />)

    const chip = screen.getByLabelText("Retry chip-fail")
    expect(chip.textContent).toContain("chip-fail")
    expect(chip.getAttribute("title")).toBe("chip-fail: chip reason")

    failing = false
    fireEvent.click(chip)
    expect(screen.getByText("chip back")).toBeTruthy()

    dispose()
  })
})

describe("Slot rendering", () => {
  it("renders nothing for an area with no contributions", () => {
    const { container } = render(<Slot area={freshArea()} />)
    expect(container.innerHTML).toBe("")
  })
})

describe("contribution surfaces are token-only", () => {
  // Colour cannot be resolved at runtime in jsdom (no stylesheet is applied),
  // so the only place this invariant is visible is the source. Scoped to the
  // files this subsystem added — it is not a rule for the rest of the tree.
  const here = path.dirname(fileURLToPath(import.meta.url))
  const contribDir = path.join(here, "..")
  const files: [string, string][] = [
    ...readdirSync(contribDir)
      .filter((name) => name.endsWith(".ts") || name.endsWith(".tsx"))
      .map((name): [string, string] => [`contrib/${name}`, path.join(contribDir, name)]),
    ["components/status-bar.tsx", path.join(contribDir, "..", "components", "status-bar.tsx")],
  ]

  const HEX = /#[0-9a-fA-F]{3,8}\b/
  const FUNCTIONAL = /\b(?:rgba?|hsla?|oklch|color-mix)\(/
  const PALETTE =
    /\b(?:bg|text|border|fill|stroke|ring|shadow|from|via|to|decoration|outline|caret|accent)-(?:white|black|slate|gray|grey|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)\b/

  it.each(files)("%s resolves every colour through a token", (_name, file) => {
    const source = readFileSync(file, "utf8")
    expect(HEX.exec(source)).toBeNull()
    expect(FUNCTIONAL.exec(source)).toBeNull()
    expect(PALETTE.exec(source)).toBeNull()
  })
})
