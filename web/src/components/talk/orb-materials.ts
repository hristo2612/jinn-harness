/**
 * The layers a lit sphere is made of, and the primitive vocabulary that
 * describes them.
 *
 * Shared by all four variants: they differ in which layers they use and how
 * they are stacked, not in what a specular or a fresnel rim is. Keeping the
 * layers here is what stops "the material" being re-invented per variant.
 */
import type { OrbState } from "./orb-motion"

export type OrbTone = "warm" | "violet" | "mixed" | "alert"

/**
 * How a primitive is lit. The canvas keeps one painter per kind, so a new
 * material is a new kind rather than another boolean on every primitive.
 */
export type OrbPrimitiveKind =
  /** The sphere's lit mass: focused toward the light, falling to the dark limb. */
  | "body"
  /** A soft internal glow that drifts. Added, so overlaps brighten. */
  | "caustic"
  /** The hot nucleus. Added. */
  | "core"
  /** One tight highlight where the light strikes. Added. */
  | "specular"
  /** Fresnel: nothing in the middle, brightest exactly at the edge. */
  | "rim"
  /** A hollow band. */
  | "ring"
  /** One flat token fill, for a face that is meant to read as machined. */
  | "shade"

export interface OrbPrimitive {
  kind: OrbPrimitiveKind
  /** Normalized canvas coordinates. */
  x: number
  y: number
  rx: number
  ry: number
  /** Ring hole as a fraction of the outer radii. */
  inner?: number
  alpha: number
  tone: OrbTone
  /** Where the light sits inside this primitive, in its own radii from centre.
   *  The offset between the body's light and the sphere's centre is the depth. */
  lightX?: number
  lightY?: number
  /** Fraction of the radius at which the fill is half gone. Lower reads softer. */
  feather?: number
  /** Added rather than laid over. */
  add?: boolean
}

/**
 * The sphere's radius as a fraction of the canvas.
 *
 * Sized so the loudest state still fits: the body is scaled by the state's
 * energy and then again by its flatten, and a sphere clipped by its own square
 * is worse than a slightly smaller one. The margin also gives the bloomed rim
 * somewhere to go.
 */
export const SPHERE = 0.36

/** The light is fixed at upper-left. Every layer agrees on it, which is what
 *  stops the orb reading as a flat disc with a gradient on it. */
export const LIGHT_X = -0.34
export const LIGHT_Y = -0.38

/** Everything a variant needs to lay its layers out. */
export interface SceneEnergy {
  scale: number
  alpha: number
  flatten: number
}

export interface SceneInput {
  state: OrbState
  energy: SceneEnergy
  /** The state's own lobe geometry at this instant, in sphere radii. */
  lobes: readonly { x: number; y: number; radius: number }[]
  /** Edge falloff for this state, already in 0..1 gradient terms. */
  feather: number
  brightness: number
  tone: OrbTone
}

/** The lit mass every variant is built on. */
export function body(energy: SceneEnergy, tone: OrbTone, radius: number): OrbPrimitive {
  return {
    kind: "body",
    x: 0.5,
    y: 0.5,
    rx: radius * energy.scale,
    ry: radius * energy.scale * energy.flatten,
    alpha: energy.alpha,
    tone,
    lightX: LIGHT_X,
    lightY: LIGHT_Y,
  }
}

/** Nothing in the middle, brightest exactly at the limb. */
export function rim(energy: SceneEnergy, tone: OrbTone, radius: number, brightness: number): OrbPrimitive {
  return {
    kind: "rim",
    x: 0.5,
    y: 0.5,
    rx: radius * energy.scale,
    ry: radius * energy.scale * energy.flatten,
    inner: 0.72,
    alpha: Math.min(1, energy.alpha * brightness * 0.9),
    tone,
    add: true,
  }
}

/** One highlight, where a real light would put it. */
export function specular(energy: SceneEnergy, brightness: number, radius: number): OrbPrimitive {
  return {
    kind: "specular",
    x: 0.5 + LIGHT_X * radius * 0.82,
    y: 0.5 + LIGHT_Y * radius * 0.82 * energy.flatten,
    rx: radius * 0.3 * energy.scale,
    ry: radius * 0.22 * energy.scale,
    alpha: Math.min(0.62, 0.4 * brightness),
    tone: "warm",
    feather: 0.24,
    add: true,
  }
}

export function core(energy: SceneEnergy, tone: OrbTone, radius: number, brightness: number, feather: number): OrbPrimitive {
  return {
    kind: "core",
    x: 0.5 + LIGHT_X * radius * 0.22,
    y: 0.5 + LIGHT_Y * radius * 0.22,
    rx: radius * 0.42 * energy.scale,
    ry: radius * 0.42 * energy.scale * energy.flatten,
    alpha: Math.min(0.72, 0.48 * brightness),
    tone,
    feather,
    add: true,
  }
}

/** The three drifting lobes, as glow inside the body rather than as the body. */
export function caustics({ lobes, energy, feather, brightness, tone }: SceneInput): OrbPrimitive[] {
  const tones: OrbTone[] = tone === "alert" ? ["alert", "alert", "alert"] : ["warm", "mixed", "violet"]
  return lobes.map((lobe, index) => ({
    kind: "caustic" as const,
    x: 0.5 + lobe.x * SPHERE,
    y: 0.5 + lobe.y * SPHERE * energy.flatten,
    rx: lobe.radius * SPHERE * energy.scale,
    ry: lobe.radius * SPHERE * energy.scale * energy.flatten,
    alpha: Math.min(1, energy.alpha * brightness * (index === 1 ? 0.46 : 0.34)),
    tone: tones[index],
    feather,
    add: true,
  }))
}
