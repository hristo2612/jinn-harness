/**
 * The orb says what it is doing with motion alone — no text, ever. This module
 * is the whole vocabulary: state in, lobe geometry out. Pure, so the states can
 * be held apart by a test rather than by eye.
 */

export type OrbVariant = "mist" | "coin" | "ring" | "pulse"

export const ORB_VARIANTS: readonly OrbVariant[] = ["mist", "coin", "ring", "pulse"]

export function isOrbVariant(value: unknown): value is OrbVariant {
  return typeof value === "string" && ORB_VARIANTS.includes(value as OrbVariant)
}

export type OrbState =
  | "idle"
  | "listening"
  | "user_speaking"
  | "thinking"
  | "assistant_speaking"
  | "interrupted"
  | "error"

/** Product order: the arc of one turn, then the two exceptional states. */
export const ORB_STATES: readonly OrbState[] = [
  "idle",
  "listening",
  "user_speaking",
  "thinking",
  "assistant_speaking",
  "interrupted",
  "error",
]

/**
 * The two things the orb can hear, kept apart.
 *
 * One shared number cannot answer "pulsate as we talk": while the operator
 * speaks the orb has to ride the microphone, and while the assistant speaks it
 * has to ride the voice coming out of the speakers. Those are two streams that
 * are live at the same moment — a barge-in has both — so they are two channels
 * rather than one that something has to remember to switch.
 */
export interface OrbEnergy {
  /** 0..1 loudness of the operator's own microphone. */
  input: number
  /** 0..1 loudness of the assistant's voice, as it is being played. */
  output: number
}

export const SILENT_ENERGY: OrbEnergy = { input: 0, output: 0 }

/**
 * How much the orb is allowed to move, as taste rather than as accessibility.
 *
 * Separate from `prefers-reduced-motion`, which is not a preference to be
 * blended: that one still paints exactly one frame per state. This scales drift
 * speed and how far audio may push the sphere, for an operator who wants the
 * control quieter (or livelier) than the default without turning it off.
 */
export const ORB_INTENSITIES = ["calm", "standard", "lively"] as const

export type OrbIntensity = (typeof ORB_INTENSITIES)[number]

export function isOrbIntensity(value: unknown): value is OrbIntensity {
  return typeof value === "string" && ORB_INTENSITIES.includes(value as OrbIntensity)
}

const INTENSITY_GAIN: Record<OrbIntensity, number> = {
  calm: 0.55,
  standard: 1,
  lively: 1.45,
}

/** The multiplier an intensity applies to drift and to audio response. */
export function intensityGain(intensity: OrbIntensity = "standard"): number {
  return INTENSITY_GAIN[intensity] ?? 1
}

export interface OrbParams {
  /** Lobe radius, as a fraction of the sphere radius. */
  radius: number
  /** Edge falloff, as a fraction of the sphere diameter. Past ~0.09 the three lobes merge into a flat disc. */
  softness: number
  /** Brightness multiplier for every lobe. */
  brightness: number
  /** Orbit direction: 1 drifts, -1 counter-rotates. */
  rotationSign: 1 | -1
  /** Lobe centre distance from the sphere centre, as a fraction of the sphere radius. */
  orbit: number
  /** Seconds per orbit, one per lobe. Co-prime-ish so the three never re-align. */
  periods: readonly [number, number, number]
}

/**
 * Each state's resting shape. The four dimensions a viewer reads without a label
 * — size, softness, brightness, direction — are pairwise distinct on purpose.
 */
const BASE: Record<OrbState, OrbParams> = {
  /** A resting ember: small, dim, lobes drawn in and barely moving. */
  idle: {
    radius: 0.44,
    softness: 0.052,
    brightness: 0.7,
    rotationSign: 1,
    orbit: 0.22,
    periods: [9, 13, 17],
  },
  /** Open and receptive — the widest lobe spread of any state, so listening
   *  reads as the orb holding itself open rather than as a brighter idle. */
  listening: {
    radius: 0.54,
    softness: 0.07,
    brightness: 0.9,
    rotationSign: 1,
    orbit: 0.4,
    periods: [5, 7, 9],
  },
  /** The operator has the floor: large, warm, and gathered — the lobes pull in
   *  and the sphere carries their voice.
   *  Direction stays forward; counter-rotation is thinking's one signature, and
   *  two states sharing it would cost the viewer that axis. */
  user_speaking: {
    radius: 0.62,
    softness: 0.058,
    brightness: 1,
    rotationSign: 1,
    orbit: 0.26,
    periods: [3.6, 4.8, 6.2],
  },
  /** Turned inward: the smallest orbit but the softest edge, counter-rotating.
   *  Diffuse where interrupted is hard, which is what keeps the two small,
   *  dim states apart. */
  thinking: {
    radius: 0.34,
    softness: 0.088,
    brightness: 0.58,
    rotationSign: -1,
    orbit: 0.16,
    periods: [3.2, 4.4, 5.6],
  },
  /** The assistant has the floor: the largest sphere, one coherent glow rather
   *  than three lobes, and the crispest edge. */
  assistant_speaking: {
    radius: 0.68,
    softness: 0.04,
    brightness: 1.12,
    rotationSign: 1,
    orbit: 0.1,
    periods: [2.4, 3.4, 4.4],
  },
  /** A held breath: small, hard-edged, flattened, and stopped. */
  interrupted: {
    radius: 0.3,
    softness: 0.024,
    brightness: 0.52,
    rotationSign: 1,
    orbit: 0.06,
    periods: [11, 14, 19],
  },
  /** Scattered and fast, in the alert tone — the only state that changes hue. */
  error: {
    radius: 0.46,
    softness: 0.03,
    brightness: 0.62,
    rotationSign: 1,
    orbit: 0.44,
    periods: [0.9, 1.1, 1.3],
  },
}

/**
 * Which channel a state rides, and how hard.
 *
 * This table is the whole routing decision, in one place: `user_speaking` reads
 * the microphone and `assistant_speaking` reads the playback, and a state with
 * no entry is not audio-driven at all — thinking must not twitch because the
 * room is noisy.
 */
interface EnergyDrive {
  channel: keyof OrbEnergy
  /** How much of the sphere's size the channel is allowed to add. */
  scale: number
}

const ENERGY_DRIVE: Record<OrbState, EnergyDrive | null> = {
  idle: null,
  listening: { channel: "input", scale: 0.08 },
  user_speaking: { channel: "input", scale: 0.14 },
  thinking: null,
  assistant_speaking: { channel: "output", scale: 0.12 },
  interrupted: null,
  error: null,
}

/** The 0..1 envelope a state is actually driven by, having picked its channel. */
export function stateEnergy(state: OrbState, energy: OrbEnergy): number {
  const drive = ENERGY_DRIVE[state]
  return drive ? clamp01(energy[drive.channel]) : 0
}

/** How much of the sphere's size that state's channel may add. */
export function energyGain(state: OrbState): number {
  return ENERGY_DRIVE[state]?.scale ?? 0
}

/** True while the state has a live stream behind it. */
export function isDriven(state: OrbState): boolean {
  return ENERGY_DRIVE[state] !== null
}

/** Relative lobe sizes, so the three read as cloud rather than as one painted
 *  disc. The cool lobe leads: amber wins on a warm base without any help. */
const LOBE_SCALES = [0.88, 1, 0.76] as const

const BASE_ANGLES = [0, (2 * Math.PI) / 3, (4 * Math.PI) / 3] as const

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.max(0, Math.min(1, value))
}

/**
 * The lobe parameters for a state at a given amplitude. The state picks which
 * channel of `energy` it rides; the states with nothing driving them ignore
 * both.
 */
export function orbParams(state: OrbState, energy: OrbEnergy = SILENT_ENERGY): OrbParams {
  const base = BASE[state]
  const amplitude = stateEnergy(state, energy)
  if (amplitude === 0) return base
  if (state === "assistant_speaking") {
    return {
      ...base,
      radius: base.radius + 0.08 * amplitude,
      brightness: base.brightness + 0.34 * amplitude,
    }
  }
  return {
    ...base,
    radius: base.radius + 0.12 * amplitude,
    orbit: base.orbit + 0.1 * amplitude,
    brightness: base.brightness + 0.28 * amplitude,
  }
}

export interface Lobe {
  /** Offset from the sphere centre, in sphere radii. */
  x: number
  y: number
  /** Radius, in sphere radii. */
  radius: number
}

/**
 * Where the three lobes sit at `seconds`. Coordinates are in sphere radii from
 * the centre, so the canvas can scale them to whatever size it is painting at.
 */
export function lobeCentres(params: OrbParams, seconds = 0): Lobe[] {
  return BASE_ANGLES.map((baseAngle, index) => {
    const turns = seconds / params.periods[index]
    const angle = baseAngle + params.rotationSign * 2 * Math.PI * turns
    return {
      x: Math.cos(angle) * params.orbit,
      y: Math.sin(angle) * params.orbit,
      radius: params.radius * LOBE_SCALES[index],
    }
  })
}
