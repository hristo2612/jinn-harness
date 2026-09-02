import { useEffect, useRef, useState, type RefObject } from "react"
import { SILENT_ENERGY, type OrbEnergy, type OrbIntensity, type OrbState, type OrbVariant } from "./orb-motion"
import { paintOrb, readPalette } from "./orb-painters"
import { orbScene } from "./orb-scene"
import { usePrefersReducedMotion } from "./use-reduced-motion"

/**
 * Four geometries painted from one pure scene model. A 2D canvas rather than a
 * CSS filter stack keeps the 64px control cheap while the page scrolls, and it
 * is the only way the layered material composites the way it needs to.
 */

/**
 * The palette hangs off `data-theme` on the root, and that attribute is the one
 * thing every theme path touches. Picking a theme writes it; so does an OS
 * scheme flip while the setting sits on "system" — and that second path changes
 * no React state at all, so a repaint keyed on the theme *setting* never fires
 * and the sphere keeps the palette it was born with.
 */
function useThemeAttribute(): string {
  const [attribute, setAttribute] = useState(() => document.documentElement.dataset.theme ?? "")
  useEffect(() => {
    const root = document.documentElement
    const read = () => setAttribute(root.dataset.theme ?? "")
    const observer = new MutationObserver(read)
    observer.observe(root, { attributes: true, attributeFilter: ["data-theme"] })
    read()
    return () => observer.disconnect()
  }, [])
  return attribute
}

interface OrbCanvasProps {
  state: OrbState
  variant?: OrbVariant
  /** Live 0..1 amplitude per channel, read once per frame. React state here
   *  would re-render the whole app on every audio frame. */
  energyRef: RefObject<OrbEnergy>
  /** CSS size in px. The sphere fills the square. */
  size: number
  /** Comparison surfaces paint one deterministic frame even when motion is allowed. */
  motion?: "live" | "still"
  /** Taste, not accessibility: how far the orb may move. */
  intensity?: OrbIntensity
}

export function OrbCanvas({ state, variant = "mist", energyRef, size, motion = "live", intensity = "standard" }: OrbCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const reduce = usePrefersReducedMotion()
  const themeAttribute = useThemeAttribute()

  useEffect(() => {
    const canvas = canvasRef.current
    const ctx = canvas?.getContext("2d")
    if (!canvas || !ctx) return
    const dpr = window.devicePixelRatio || 1
    canvas.width = Math.round(size * dpr)
    canvas.height = Math.round(size * dpr)
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    const palette = readPalette(canvas)

    // Still, not dead: one frame per state, in that state's own geometry.
    if (reduce || motion === "still") {
      paintOrb(ctx, { size, palette, scene: orbScene(variant, state, SILENT_ENERGY, 0, intensity) })
      return
    }

    let frame = 0
    const draw = (now: number) => {
      frame = requestAnimationFrame(draw)
      paintOrb(ctx, { size, palette, scene: orbScene(variant, state, energyRef.current, now / 1000, intensity) })
    }
    frame = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(frame)
    // `themeAttribute` is not read here — it re-reads the palette when the theme flips.
  }, [state, variant, size, reduce, motion, themeAttribute, energyRef, intensity])

  return (
    <canvas
      ref={canvasRef}
      aria-hidden
      data-orb-canvas-variant={variant}
      style={{ width: size, height: size, display: "block" }}
    />
  )
}
