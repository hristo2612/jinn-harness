import { describe, expect, it } from "vitest"
import {
  ORB_STATES,
  ORB_VARIANTS,
  SILENT_ENERGY,
  lobeCentres,
  orbParams,
  stateEnergy,
  type OrbState,
} from "../orb-motion"
import { orbScene } from "../orb-scene"

/** The operator talking, the assistant talking, and both at once. */
const MIC = { input: 1, output: 0 }
const PLAYBACK = { input: 0, output: 1 }
const BOTH = { input: 1, output: 1 }

/** The four dimensions a viewer reads off the sphere without any label. */
function signature(state: OrbState) {
  const { radius, softness, brightness, rotationSign } = orbParams(state)
  return { radius, softness, brightness, rotationSign }
}

describe("orbParams", () => {
  it("includes a visible error state in Aurora's motion vocabulary", () => {
    expect(ORB_STATES).toContain("error")
  })

  it("gives every state a distinct signature", () => {
    for (const a of ORB_STATES) {
      for (const b of ORB_STATES) {
        if (a === b) continue
        expect(signature(a), `${a} vs ${b}`).not.toEqual(signature(b))
      }
    }
  })

  it("separates the states on radius, softness and brightness individually", () => {
    for (const key of ["radius", "softness", "brightness"] as const) {
      const values = ORB_STATES.map((state) => signature(state)[key])
      expect(new Set(values).size).toBe(ORB_STATES.length)
    }
  })

  it("keeps the whole scene distinct per state at rest, not just the numbers", () => {
    // The signature a viewer actually reads is the painted scene, not the
    // params behind it. At silence — which is how a reduced-motion frame and a
    // screenshot both see the orb — no two states may paint the same thing.
    for (const variant of ORB_VARIANTS) {
      const scenes = ORB_STATES.map(
        (state) => [state, JSON.stringify(orbScene(variant, state, SILENT_ENERGY, 0))] as const,
      )
      expect(new Set(scenes.map(([, scene]) => scene)).size, variant).toBe(ORB_STATES.length)
    }
  })

  it("gives the two speakers different hues, which is the fastest thing read", () => {
    const toneOf = (state: OrbState) => orbScene("mist", state)[0]!.tone
    expect(toneOf("user_speaking")).toBe("warm")
    expect(toneOf("assistant_speaking")).toBe("violet")
    expect(toneOf("error")).toBe("alert")
    // Everything else stays on the shared family.
    for (const state of ["idle", "listening", "thinking", "interrupted"] as const) {
      expect(toneOf(state), state).toBe("mixed")
    }
  })

  it("counter-rotates only while thinking", () => {
    expect(orbParams("thinking").rotationSign).toBe(-1)
    for (const state of ORB_STATES.filter((s) => s !== "thinking")) {
      expect(orbParams(state).rotationSign).toBe(1)
    }
  })

  it("keeps lobe softness under the merge threshold in every state", () => {
    for (const state of ORB_STATES) {
      expect(orbParams(state).softness).toBeLessThanOrEqual(0.09)
    }
  })

  it("pushes the listening lobes outward and brighter as amplitude rises", () => {
    const quiet = orbParams("listening", SILENT_ENERGY)
    const loud = orbParams("listening", MIC)
    expect(loud.radius).toBeGreaterThan(quiet.radius)
    expect(loud.orbit).toBeGreaterThan(quiet.orbit)
    expect(loud.brightness).toBeGreaterThan(quiet.brightness)
  })

  it("rides brightness on the output envelope while the assistant speaks", () => {
    expect(orbParams("assistant_speaking", PLAYBACK).brightness)
      .toBeGreaterThan(orbParams("assistant_speaking", SILENT_ENERGY).brightness)
  })

  it("ignores amplitude in the states nothing is driving", () => {
    expect(orbParams("idle", BOTH)).toEqual(orbParams("idle", SILENT_ENERGY))
    expect(orbParams("thinking", BOTH)).toEqual(orbParams("thinking", SILENT_ENERGY))
    expect(orbParams("interrupted", BOTH)).toEqual(orbParams("interrupted", SILENT_ENERGY))
    expect(orbParams("error", BOTH)).toEqual(orbParams("error", SILENT_ENERGY))
  })

  it("clamps amplitude out of range instead of exploding the sphere", () => {
    expect(orbParams("assistant_speaking", { input: 0, output: 4 }))
      .toEqual(orbParams("assistant_speaking", PLAYBACK))
    expect(orbParams("assistant_speaking", { input: 0, output: -2 }))
      .toEqual(orbParams("assistant_speaking", SILENT_ENERGY))
    expect(orbParams("assistant_speaking", { input: 0, output: Number.NaN }))
      .toEqual(orbParams("assistant_speaking", SILENT_ENERGY))
  })
})

/**
 * The routing this whole feature turns on. `user_speaking` must ride the
 * microphone and `assistant_speaking` must ride the playback: swap them and the
 * orb goes still exactly when the operator is talking to it, which is the bug
 * this slice exists to fix. Asserted on both the params and the scene, because
 * the canvas only ever reads the scene.
 */
describe("which channel a state listens to", () => {
  it("rides the microphone while the operator speaks and ignores the playback", () => {
    expect(stateEnergy("user_speaking", MIC)).toBe(1)
    expect(stateEnergy("user_speaking", PLAYBACK)).toBe(0)
    expect(orbParams("user_speaking", MIC)).not.toEqual(orbParams("user_speaking", SILENT_ENERGY))
    expect(orbParams("user_speaking", PLAYBACK)).toEqual(orbParams("user_speaking", SILENT_ENERGY))
    expect(orbParams("user_speaking", BOTH)).toEqual(orbParams("user_speaking", MIC))
  })

  it("rides the playback while the assistant speaks and ignores the microphone", () => {
    expect(stateEnergy("assistant_speaking", PLAYBACK)).toBe(1)
    expect(stateEnergy("assistant_speaking", MIC)).toBe(0)
    expect(orbParams("assistant_speaking", PLAYBACK))
      .not.toEqual(orbParams("assistant_speaking", SILENT_ENERGY))
    expect(orbParams("assistant_speaking", MIC)).toEqual(orbParams("assistant_speaking", SILENT_ENERGY))
    expect(orbParams("assistant_speaking", BOTH)).toEqual(orbParams("assistant_speaking", PLAYBACK))
  })

  it("paints the same split, so the canvas cannot read the other channel", () => {
    for (const variant of ORB_VARIANTS) {
      expect(orbScene(variant, "user_speaking", MIC))
        .not.toEqual(orbScene(variant, "user_speaking", SILENT_ENERGY))
      expect(orbScene(variant, "user_speaking", PLAYBACK))
        .toEqual(orbScene(variant, "user_speaking", SILENT_ENERGY))

      expect(orbScene(variant, "assistant_speaking", PLAYBACK))
        .not.toEqual(orbScene(variant, "assistant_speaking", SILENT_ENERGY))
      expect(orbScene(variant, "assistant_speaking", MIC))
        .toEqual(orbScene(variant, "assistant_speaking", SILENT_ENERGY))
    }
  })

  it("leaves the states with nothing behind them deaf to both channels", () => {
    for (const state of ["idle", "thinking", "interrupted", "error"] as const) {
      expect(stateEnergy(state, BOTH)).toBe(0)
    }
  })
})

describe("lobeCentres", () => {
  it("returns three lobes inside the sphere", () => {
    for (const state of ORB_STATES) {
      const lobes = lobeCentres(orbParams(state), 3.7)
      expect(lobes).toHaveLength(3)
      for (const lobe of lobes) {
        expect(Math.hypot(lobe.x, lobe.y) + lobe.radius).toBeLessThanOrEqual(1)
      }
    }
  })

  it("drifts the lobes over time", () => {
    const params = orbParams("idle")
    expect(lobeCentres(params, 2)).not.toEqual(lobeCentres(params, 0))
  })

  it("mirrors the drift direction when the rotation sign flips", () => {
    const forward = orbParams("idle")
    const reversed = { ...forward, rotationSign: -1 as const }
    const [a] = lobeCentres(forward, 1)
    const [b] = lobeCentres(reversed, 1)
    expect(a.x).toBeCloseTo(b.x, 10)
    expect(a.y).toBeCloseTo(-b.y, 10)
  })

  it("gives the three lobes different sizes so they read as cloud, not as a disc", () => {
    const radii = lobeCentres(orbParams("idle"), 0).map((lobe) => lobe.radius)
    expect(new Set(radii).size).toBe(3)
  })
})
