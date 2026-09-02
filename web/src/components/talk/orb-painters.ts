/**
 * The only place pixels are produced.
 *
 * `orb-scene.ts` decides what the orb is made of; this decides how each of
 * those materials meets a 2D context. Colours arrive only as token values read
 * off the element — nothing here names one, which is what lets the same code
 * paint both themes.
 */
import type { OrbPrimitive, OrbTone } from "./orb-scene"

export const PALETTE_TOKENS = [
  "--orb-core",
  "--orb-base",
  "--orb-bloom",
  "--orb-lobe-a",
  "--orb-lobe-b",
  "--orb-lobe-c",
  "--orb-shadow",
  "--orb-specular",
  "--orb-rim",
  "--system-red",
] as const

export type OrbPalette = Record<(typeof PALETTE_TOKENS)[number], string>

/** Custom properties inherit, so the sphere's own computed style carries the theme. */
export function readPalette(element: Element): OrbPalette {
  const style = getComputedStyle(element)
  const palette = {} as OrbPalette
  for (const token of PALETTE_TOKENS) palette[token] = style.getPropertyValue(token).trim()
  return palette
}

export interface Frame {
  /** CSS size of the square the sphere fills. */
  size: number
  palette: OrbPalette
  scene: readonly OrbPrimitive[]
}

interface ToneColors {
  /** The light itself, at the gradient's focus. */
  hot: string
  /** The lit mass between the highlight and the terminator. */
  mid: string
  /** The limb the light never reaches. */
  deep: string
}

function toneColors(palette: OrbPalette, tone: OrbTone): ToneColors {
  if (tone === "warm") {
    return { hot: palette["--orb-core"], mid: palette["--orb-lobe-a"], deep: palette["--orb-base"] }
  }
  if (tone === "violet") {
    return { hot: palette["--orb-bloom"], mid: palette["--orb-lobe-b"], deep: palette["--orb-lobe-c"] }
  }
  if (tone === "alert") {
    return { hot: palette["--orb-core"], mid: palette["--system-red"], deep: palette["--orb-base"] }
  }
  return { hot: palette["--orb-core"], mid: palette["--orb-bloom"], deep: palette["--orb-lobe-c"] }
}

/** The empty stop. A keyword, so no colour is spelled out in this file. */
const CLEAR = "transparent"

interface Geometry {
  x: number
  y: number
  rx: number
  ry: number
  /** Where the gradient is focused, which is not always the centre. */
  lightX: number
  lightY: number
  radius: number
}

function geometry(frame: Frame, primitive: OrbPrimitive): Geometry {
  const x = primitive.x * frame.size
  const y = primitive.y * frame.size
  const rx = primitive.rx * frame.size
  const ry = primitive.ry * frame.size
  return {
    x,
    y,
    rx,
    ry,
    lightX: x + (primitive.lightX ?? 0) * rx,
    lightY: y + (primitive.lightY ?? 0) * ry,
    radius: Math.max(rx, ry, 1),
  }
}

/** Every kind is one gradient recipe; this is the table of them. */
function fillFor(
  ctx: CanvasRenderingContext2D,
  frame: Frame,
  primitive: OrbPrimitive,
  at: Geometry,
): string | CanvasGradient {
  const colors = toneColors(frame.palette, primitive.tone)
  const feather = primitive.feather ?? 0.5

  if (primitive.kind === "shade") return colors.mid

  if (primitive.kind === "rim") {
    // Fresnel: empty until close to the limb, then bright right at it.
    const gradient = ctx.createRadialGradient(at.x, at.y, 0, at.x, at.y, at.radius)
    gradient.addColorStop(0, CLEAR)
    gradient.addColorStop(primitive.inner ?? 0.72, CLEAR)
    gradient.addColorStop(0.94, frame.palette["--orb-rim"])
    gradient.addColorStop(1, CLEAR)
    return gradient
  }

  if (primitive.kind === "specular") {
    const gradient = ctx.createRadialGradient(at.x, at.y, 0, at.x, at.y, at.radius)
    gradient.addColorStop(0, frame.palette["--orb-specular"])
    gradient.addColorStop(feather, frame.palette["--orb-core"])
    gradient.addColorStop(1, CLEAR)
    return gradient
  }

  if (primitive.kind === "caustic" || primitive.kind === "core") {
    const gradient = ctx.createRadialGradient(at.lightX, at.lightY, 0, at.x, at.y, at.radius)
    gradient.addColorStop(0, colors.hot)
    gradient.addColorStop(feather, colors.mid)
    gradient.addColorStop(1, CLEAR)
    return gradient
  }

  // body and ring: lit from the fixed light, falling away to the dark limb.
  const gradient = ctx.createRadialGradient(at.lightX, at.lightY, 0, at.x, at.y, at.radius * 1.25)
  gradient.addColorStop(0, colors.hot)
  gradient.addColorStop(0.34, colors.mid)
  gradient.addColorStop(0.78, colors.deep)
  gradient.addColorStop(1, frame.palette["--orb-shadow"])
  return gradient
}

function paintPrimitive(ctx: CanvasRenderingContext2D, frame: Frame, primitive: OrbPrimitive): void {
  const at = geometry(frame, primitive)
  ctx.globalCompositeOperation = primitive.add ? "lighter" : "source-over"
  ctx.fillStyle = fillFor(ctx, frame, primitive, at)
  ctx.globalAlpha = primitive.alpha
  ctx.beginPath()
  ctx.ellipse(at.x, at.y, at.rx, at.ry, 0, 0, Math.PI * 2)
  if (primitive.kind === "ring") {
    const inner = primitive.inner ?? 0.66
    ctx.ellipse(at.x, at.y, at.rx * inner, at.ry * inner, 0, 0, Math.PI * 2, true)
    ctx.fill("evenodd")
  } else ctx.fill()
}

export function paintOrb(ctx: CanvasRenderingContext2D, frame: Frame): void {
  ctx.globalCompositeOperation = "source-over"
  ctx.globalAlpha = 1
  ctx.clearRect(0, 0, frame.size, frame.size)
  for (const primitive of frame.scene) paintPrimitive(ctx, frame, primitive)
  ctx.globalCompositeOperation = "source-over"
  ctx.globalAlpha = 1
}
