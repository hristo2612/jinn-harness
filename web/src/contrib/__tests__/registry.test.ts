import { describe, expect, it, vi } from "vitest"
import { NO_CONTRIBUTIONS, contributions } from "../registry"
import type { Contribution } from "../types"

// The registry is a singleton, so every test invents its own area ids and
// disposes what it registered. Sharing an area between tests would let one
// leak into the next exactly the way a stale registration leaks in production.
let areaCounter = 0
function freshArea(): string {
  areaCounter += 1
  return `test.area.${areaCounter}`
}

describe("ContributionRegistry snapshots", () => {
  it("returns the same reference until the area mutates", () => {
    const area = freshArea()
    const dispose = contributions.register({ id: "a", area })

    const first = contributions.getArea(area)
    expect(contributions.getArea(area)).toBe(first)

    const second = contributions.register({ id: "b", area })
    expect(contributions.getArea(area)).not.toBe(first)

    dispose()
    second()
  })

  it("answers an empty area with one shared frozen array", () => {
    expect(contributions.getArea(freshArea())).toBe(NO_CONTRIBUTIONS)
    expect(contributions.getArea(freshArea())).toBe(NO_CONTRIBUTIONS)
    expect(Object.isFrozen(NO_CONTRIBUTIONS)).toBe(true)
  })

  it("keeps one area's snapshot and subscribers untouched when another mutates", () => {
    const [a, b] = [freshArea(), freshArea()]
    const onA = vi.fn()
    const onB = vi.fn()
    const dispose = contributions.register({ id: "seed", area: a })
    const unsubA = contributions.subscribeArea(a, onA)
    const unsubB = contributions.subscribeArea(b, onB)

    const snapshotOfA = contributions.getArea(a)
    const disposeB = contributions.register({ id: "elsewhere", area: b })

    expect(onA).not.toHaveBeenCalled()
    expect(contributions.getArea(a)).toBe(snapshotOfA)
    expect(onB).toHaveBeenCalledTimes(1)

    unsubA()
    unsubB()
    dispose()
    disposeB()
  })
})

describe("ContributionRegistry resolution", () => {
  it("orders by `order ?? 0` and keeps insertion order for ties", () => {
    const area = freshArea()
    const dispose = contributions.registerMany([
      { id: "last", area, order: 5 },
      { id: "unordered-first", area },
      { id: "unordered-second", area },
      { id: "first", area, order: -1 },
    ])

    expect(contributions.getArea(area).map((c) => c.id)).toEqual([
      "first",
      "unordered-first",
      "unordered-second",
      "last",
    ])

    dispose()
  })

  it("replaces an entry re-registered under the same id rather than duplicating it", () => {
    const area = freshArea()
    const first = contributions.register({ id: "same", area, data: "original" })
    const second = contributions.register({ id: "same", area, data: "replacement" })

    const resolved = contributions.getArea(area)
    expect(resolved).toHaveLength(1)
    expect(resolved[0].data).toBe("replacement")

    first()
    second()
  })

  it("excludes a contribution whose `when` says no", () => {
    const area = freshArea()
    const dispose = contributions.registerMany([
      { id: "hidden", area, when: () => false },
      { id: "shown", area, when: () => true },
      { id: "default", area },
    ])

    expect(contributions.getArea(area).map((c) => c.id)).toEqual(["shown", "default"])

    dispose()
  })

  it("stamps provenance instead of accepting it", () => {
    const area = freshArea()
    // An author cannot type `source`, but a plugin is untyped JavaScript by the
    // time it reaches here, so the cast is the honest shape of the attack. The
    // value has to be one the registry would never stamp itself, or a registry
    // that passed the author's object straight through would pass this too.
    const dispose = contributions.register({
      id: "liar",
      area,
      source: "plugin:evil",
    } as Contribution)

    expect(contributions.getArea(area)[0].source).toBe("core")

    dispose()
  })
})

describe("ContributionRegistry disposal", () => {
  it("removes exactly its own entries", () => {
    const area = freshArea()
    const disposeBatch = contributions.registerMany([
      { id: "mine-a", area },
      { id: "mine-b", area },
    ])
    const disposeOther = contributions.register({ id: "theirs", area })

    disposeBatch()

    expect(contributions.getArea(area).map((c) => c.id)).toEqual(["theirs"])

    disposeOther()
  })

  it("leaves a replacement alone when the disposer it replaced runs", () => {
    const area = freshArea()
    const disposeOriginal = contributions.register({ id: "same", area, data: "original" })
    const disposeReplacement = contributions.register({ id: "same", area, data: "replacement" })

    disposeOriginal()

    expect(contributions.getArea(area).map((c) => c.data)).toEqual(["replacement"])

    disposeReplacement()
  })

  it("notifies nobody when a disposer runs a second time", () => {
    const area = freshArea()
    const onChange = vi.fn()
    const dispose = contributions.register({ id: "once", area })
    const unsubscribe = contributions.subscribeArea(area, onChange)

    dispose()
    expect(onChange).toHaveBeenCalledTimes(1)

    dispose()
    expect(onChange).toHaveBeenCalledTimes(1)

    unsubscribe()
  })

  it("stops notifying an unsubscribed listener", () => {
    const area = freshArea()
    const onChange = vi.fn()
    contributions.subscribeArea(area, onChange)()

    const dispose = contributions.register({ id: "after", area })
    expect(onChange).not.toHaveBeenCalled()

    dispose()
  })
})
