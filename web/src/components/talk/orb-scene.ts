/**
 * How each variant stacks the material into a scene.
 *
 * `orb-materials.ts` says what a body, a caustic, a specular and a fresnel rim
 * are; this says which of them a given variant uses and in what order. Pure, so
 * every arrangement can be held to its promise by a test rather than by eye.
 */
import {
  energyGain,
  intensityGain,
  isDriven,
  lobeCentres,
  orbParams,
  stateEnergy,
  SILENT_ENERGY,
  type OrbEnergy,
  type OrbIntensity,
  type OrbState,
  type OrbVariant,
} from "./orb-motion"
import {
  LIGHT_X,
  LIGHT_Y,
  SPHERE,
  body,
  caustics,
  core,
  rim,
  specular,
  type OrbPrimitive,
  type OrbTone,
  type SceneEnergy,
  type SceneInput,
} from "./orb-materials"

export type { OrbPrimitive, OrbPrimitiveKind, OrbTone } from "./orb-materials"

const STATE_ENERGY: Record<OrbState, SceneEnergy> = {
  idle: { scale: 0.86, alpha: 0.72, flatten: 1 },
  listening: { scale: 1, alpha: 0.94, flatten: 1.04 },
  user_speaking: { scale: 1.08, alpha: 0.97, flatten: 1.06 },
  thinking: { scale: 0.8, alpha: 0.66, flatten: 0.92 },
  assistant_speaking: { scale: 1.12, alpha: 1, flatten: 1 },
  interrupted: { scale: 0.74, alpha: 0.78, flatten: 0.72 },
  error: { scale: 0.88, alpha: 0.86, flatten: 0.84 },
}

/**
 * Who the orb is currently being.
 *
 * Hue is the fastest thing a viewer reads, and the pair that most needs telling
 * apart is the two speakers — so the operator's turn is warm and the
 * assistant's is violet. They are never the primary state at the same moment,
 * so the two never compete. Everything else stays on the shared mixed tone;
 * only `error` leaves the family.
 */
const STATE_TONE: Record<OrbState, OrbTone> = {
  idle: "mixed",
  listening: "mixed",
  user_speaking: "warm",
  thinking: "mixed",
  assistant_speaking: "violet",
  interrupted: "mixed",
  error: "alert",
}

function sceneEnergy(state: OrbState, energy: OrbEnergy): SceneEnergy {
  const base = STATE_ENERGY[state]
  const envelope = stateEnergy(state, energy)
  return {
    ...base,
    scale: base.scale + envelope * energyGain(state),
    alpha: Math.min(1, base.alpha + envelope * 0.06),
  }
}

/** The cloud sphere: the full material, lobes and all. */
function mistScene(input: SceneInput): readonly OrbPrimitive[] {
  const { energy, tone, brightness, feather } = input
  return [
    body(energy, tone, SPHERE),
    ...caustics(input),
    core(energy, tone, SPHERE, brightness, feather),
    specular(energy, brightness, SPHERE),
    rim(energy, tone, SPHERE, brightness),
  ]
}

/** Machined: a lit body under one flat face, so it reads as a struck disc. */
function coinScene({ energy, tone, brightness, feather }: SceneInput): readonly OrbPrimitive[] {
  return [
    body(energy, tone, SPHERE),
    {
      kind: "shade",
      x: 0.5,
      y: 0.5,
      rx: SPHERE * 0.74 * energy.scale,
      ry: SPHERE * 0.74 * energy.scale * energy.flatten,
      alpha: energy.alpha * 0.42,
      tone: "violet",
    },
    core(energy, tone, SPHERE * 0.8, brightness, feather),
    specular(energy, brightness, SPHERE),
    rim(energy, tone, SPHERE, brightness),
  ]
}

/** Rim-lit torus: the band carries the light, the middle stays open. */
function ringScene({ state, energy, tone, brightness, feather }: SceneInput): readonly OrbPrimitive[] {
  const hole = isDriven(state) ? 0.54 : state === "interrupted" ? 0.72 : 0.64
  return [
    {
      kind: "ring",
      x: 0.5,
      y: 0.5,
      rx: SPHERE * energy.scale,
      ry: SPHERE * energy.scale * energy.flatten,
      inner: hole,
      alpha: energy.alpha,
      tone,
      lightX: LIGHT_X,
      lightY: LIGHT_Y,
      feather,
    },
    core(energy, tone, SPHERE * hole, brightness * 0.5, feather),
    specular(energy, brightness * 0.8, SPHERE),
    rim(energy, tone, SPHERE, brightness),
  ]
}

/** Concentric: three bands the energy travels out through. */
function pulseScene({ state, energy, tone, brightness, feather }: SceneInput): readonly OrbPrimitive[] {
  const radii = state === "interrupted" ? [0.34, 0.58, 0.8] : [0.38, 0.64, 0.92]
  const bands = radii.map((fraction, index) => ({
    kind: "ring" as const,
    x: 0.5,
    y: 0.5,
    rx: SPHERE * fraction * energy.scale,
    ry: SPHERE * fraction * 0.86 * energy.scale * energy.flatten,
    inner: isDriven(state) ? 0.7 : 0.8,
    alpha: energy.alpha * (index === 1 ? 1 : 0.58),
    tone: index === 2 ? "violet" : tone,
    lightX: LIGHT_X,
    lightY: LIGHT_Y,
    feather,
  }))
  return [
    ...bands,
    core(energy, tone, SPHERE * 0.44, brightness, feather),
    specular(energy, brightness * 0.7, SPHERE * 0.5),
  ]
}

const SCENE_BUILDERS: Record<OrbVariant, (input: SceneInput) => readonly OrbPrimitive[]> = {
  mist: mistScene,
  coin: coinScene,
  ring: ringScene,
  pulse: pulseScene,
}

/**
 * `softness` is an edge falloff in sphere diameters; the canvas wants a 0..1
 * gradient stop. Softer states reach half-strength earlier, which is what makes
 * thinking read as diffuse and interrupted as hard-edged.
 */
function featherOf(softness: number): number {
  return Math.max(0.16, Math.min(0.92, 1 - softness * 7))
}

/** One scene vocabulary hides all four paint strategies from the canvas. */
export function orbScene(
  variant: OrbVariant,
  state: OrbState,
  energy: OrbEnergy = SILENT_ENERGY,
  seconds = 0,
  intensity: OrbIntensity = "standard",
): readonly OrbPrimitive[] {
  // Interruption is a held breath: time stops, and so does the audio it would
  // otherwise ride.
  const frozen = state === "interrupted"
  const gain = intensityGain(intensity)
  const heard = frozen ? SILENT_ENERGY : scaleEnergy(energy, gain)
  const params = orbParams(state, heard)
  return SCENE_BUILDERS[variant]({
    state,
    energy: sceneEnergy(state, heard),
    lobes: lobeCentres(params, frozen ? 0 : seconds * gain),
    feather: featherOf(params.softness),
    brightness: params.brightness,
    tone: STATE_TONE[state],
  })
}

/** Taste scales how far audio may push the sphere, never which channel it
 *  reads: a quieter orb is still the same orb answering the same voice. */
function scaleEnergy(energy: OrbEnergy, gain: number): OrbEnergy {
  if (gain === 1) return energy
  return { input: energy.input * gain, output: energy.output * gain }
}
