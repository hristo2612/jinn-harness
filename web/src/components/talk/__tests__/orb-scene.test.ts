import { describe, expect, it } from "vitest"
import { ORB_INTENSITIES, ORB_STATES, ORB_VARIANTS, SILENT_ENERGY } from "../orb-motion"
import { orbScene } from "../orb-scene"

/** The operator talking, the assistant talking, and both at once. */
const PLAYBACK = { input: 0, output: 1 }
const BOTH = { input: 1, output: 1 }

/** Every variant crossed with every state, flattened so a check over the whole
 *  matrix is one loop rather than three. */
function everyScene(energy = SILENT_ENERGY, seconds = 0) {
  return ORB_VARIANTS.flatMap((variant) =>
    ORB_STATES.map((state) => [`${variant}/${state}`, orbScene(variant, state, energy, seconds)] as const),
  )
}

function expectBoundedScene(scene: ReturnType<typeof orbScene>) {
  expect(scene.length).toBeGreaterThan(0)
  for (const primitive of scene) {
    for (const value of [primitive.x, primitive.y, primitive.rx, primitive.ry, primitive.alpha]) {
      expect(Number.isFinite(value)).toBe(true)
    }
    expect(primitive.x - primitive.rx).toBeGreaterThanOrEqual(0)
    expect(primitive.x + primitive.rx).toBeLessThanOrEqual(1)
    expect(primitive.y - primitive.ry).toBeGreaterThanOrEqual(0)
    expect(primitive.y + primitive.ry).toBeLessThanOrEqual(1)
  }
}

describe("orbScene", () => {
  it("keeps every variant/state scene finite and inside the canvas", () => {
    for (const [, scene] of everyScene(BOTH, 4.2)) expectBoundedScene(scene)
  })

  it("gives the four styles different geometry, not only different names", () => {
    const signatures = ORB_VARIANTS.map((variant) => JSON.stringify(orbScene(variant, "idle")))
    expect(new Set(signatures).size).toBe(ORB_VARIANTS.length)
  })

  it("locks each named paint strategy to its promised geometry", () => {
    const kinds = (variant: Parameters<typeof orbScene>[0]) =>
      orbScene(variant, "idle").map((shape) => shape.kind)

    // The cloud sphere carries the whole material, lobes included.
    expect(kinds("mist")).toEqual([
      "body", "caustic", "caustic", "caustic", "core", "specular", "rim",
    ])

    // Machined: a lit body under one flat face.
    expect(kinds("coin")).toEqual(["body", "shade", "core", "specular", "rim"])

    // Rim-lit torus: the band carries the light and the middle stays open.
    expect(kinds("ring")).toEqual(["ring", "core", "specular", "rim"])

    // Concentric bands, smallest first.
    const pulse = orbScene("pulse", "idle")
    expect(pulse.map((shape) => shape.kind)).toEqual(["ring", "ring", "ring", "core", "specular"])
    const bands = pulse.filter((shape) => shape.kind === "ring")
    expect(bands.map((shape) => shape.rx)).toEqual([...bands].map((s) => s.rx).sort((a, b) => a - b))
  })

  /**
   * The bug this slice exists to kill: the old `mist` was one faded ellipse, so
   * every state of the default variant was a blurry blob with a different
   * alpha. Depth means layers that disagree about where the light is.
   */
  it("never renders a state as one faded ellipse", () => {
    for (const [label, scene] of everyScene(BOTH, 2.5)) {
      expect(scene.length, label).toBeGreaterThan(2)
      // Something is lit off-centre, and something composites additively.
      expect(scene.some((shape) => shape.lightX !== undefined), `${label} lit`).toBe(true)
      expect(scene.some((shape) => shape.add), `${label} glow`).toBe(true)
    }
  })

  it("gives every state a specular and an edge, so none of them reads as a sticker", () => {
    for (const [label, scene] of everyScene()) {
      const kinds = new Set(scene.map((shape) => shape.kind))
      expect(kinds.has("specular"), label).toBe(true)
      expect(kinds.has("rim") || kinds.has("ring"), label).toBe(true)
    }
  })

  it("drifts the caustic lobes over time, and only for the states that move", () => {
    const at = (seconds: number) => JSON.stringify(orbScene("mist", "listening", BOTH, seconds))
    expect(at(0)).not.toEqual(at(2.5))
  })

  it("holds interruption still even if time and audio continue", () => {
    for (const variant of ORB_VARIANTS) {
      expect(orbScene(variant, "interrupted", SILENT_ENERGY, 0)).toEqual(
        orbScene(variant, "interrupted", BOTH, 99),
      )
    }
  })
})

describe("motion intensity", () => {
  it("scales how far audio may push the sphere without changing the channel", () => {
    // A normal speaking level, not a shout: the channel is 0..1 and clamped, so
    // at full input every intensity is already pinned to the same ceiling and
    // there is nothing left for taste to scale.
    const HALF = { input: 0.5, output: 0 }
    const calm = orbScene("mist", "user_speaking", HALF, 0, "calm")
    const standard = orbScene("mist", "user_speaking", HALF, 0, "standard")
    const lively = orbScene("mist", "user_speaking", HALF, 0, "lively")
    const width = (scene: typeof calm) => scene[0]!.rx

    expect(width(calm)).toBeLessThan(width(standard))
    expect(width(lively)).toBeGreaterThan(width(standard))
    // Still deaf to the other channel at every intensity.
    for (const intensity of ORB_INTENSITIES) {
      expect(orbScene("mist", "user_speaking", PLAYBACK, 0, intensity))
        .toEqual(orbScene("mist", "user_speaking", SILENT_ENERGY, 0, intensity))
    }
  })

  it("scales drift speed, so a calm orb is the same orb moving less", () => {
    const calm = JSON.stringify(orbScene("mist", "listening", SILENT_ENERGY, 3, "calm"))
    const lively = JSON.stringify(orbScene("mist", "listening", SILENT_ENERGY, 3, "lively"))
    expect(calm).not.toEqual(lively)
  })

  it("leaves interruption frozen at every intensity", () => {
    for (const intensity of ORB_INTENSITIES) {
      expect(orbScene("mist", "interrupted", BOTH, 99, intensity))
        .toEqual(orbScene("mist", "interrupted", SILENT_ENERGY, 0, intensity))
    }
  })
})
