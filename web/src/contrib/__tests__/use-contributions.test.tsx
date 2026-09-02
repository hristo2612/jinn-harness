import { describe, expect, it } from "vitest"
import { act, render } from "@testing-library/react"
import { contributions } from "../registry"
import { useContributions } from "../use-contributions"

let areaCounter = 0
function freshArea(): string {
  areaCounter += 1
  return `hook.area.${areaCounter}`
}

/** Counts its own renders, so a snapshot that is not referentially stable shows
 *  up as a render count that keeps climbing rather than as a hung test. */
function Reader({ area, renders }: { area: string; renders: { count: number } }) {
  const items = useContributions(area)
  renders.count += 1
  return <span data-testid="ids">{items.map((c) => c.id).join(",")}</span>
}

describe("useContributions", () => {
  it("renders once for a registry that is not moving", () => {
    const renders = { count: 0 }
    const area = freshArea()
    const dispose = contributions.register({ id: "stable", area })

    const { getByTestId } = render(<Reader area={area} renders={renders} />)

    expect(getByTestId("ids").textContent).toBe("stable")
    expect(renders.count).toBe(1)

    dispose()
  })

  it("re-renders exactly once for a change in its own area, and not at all for another", () => {
    const renders = { count: 0 }
    const [mine, theirs] = [freshArea(), freshArea()]

    render(<Reader area={mine} renders={renders} />)
    expect(renders.count).toBe(1)

    let disposeMine = () => {}
    act(() => {
      disposeMine = contributions.register({ id: "mine", area: mine })
    })
    expect(renders.count).toBe(2)

    let disposeTheirs = () => {}
    act(() => {
      disposeTheirs = contributions.register({ id: "theirs", area: theirs })
    })
    expect(renders.count).toBe(2)

    disposeMine()
    disposeTheirs()
  })

  it("unsubscribes on unmount", () => {
    const renders = { count: 0 }
    const area = freshArea()

    const { unmount } = render(<Reader area={area} renders={renders} />)
    unmount()

    // A live subscription here would notify an unmounted tree, which React
    // reports as an act() warning rather than as a failure — so assert on the
    // count instead, which cannot move once the listener is gone.
    const dispose = contributions.register({ id: "after-unmount", area })
    expect(renders.count).toBe(1)

    dispose()
  })
})
