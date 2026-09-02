import { cleanup, render } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { OrbCanvas } from "../orb-canvas"
import * as motion from "../orb-motion"

const VARIANTS = ["mist", "coin", "ring", "pulse"] as const
const STATES = [
  "idle",
  "listening",
  "user_speaking",
  "thinking",
  "assistant_speaking",
  "interrupted",
  "error",
] as const

type Variant = (typeof VARIANTS)[number]

function fakeContext() {
  const trace: string[] = []
  return {
    trace,
    setTransform: vi.fn(),
    clearRect: vi.fn(),
    fillRect: vi.fn((...args: number[]) => trace.push(`rect:${args.join(",")}`)),
    beginPath: vi.fn(() => trace.push("path")),
    arc: vi.fn((...args: number[]) => trace.push(`arc:${args.join(",")}`)),
    ellipse: vi.fn((...args: number[]) => trace.push(`ellipse:${args.join(",")}`)),
    fill: vi.fn(() => trace.push("fill")),
    createRadialGradient: vi.fn((...args: number[]) => {
      trace.push(`gradient:${args.join(",")}`)
      return { addColorStop: (at: number, color: string) => trace.push(`stop:${at}:${color}`) }
    }),
    globalCompositeOperation: "source-over",
    globalAlpha: 1,
    fillStyle: "",
  }
}

function renderStill(variant: Variant): string {
  const ctx = fakeContext()
  HTMLCanvasElement.prototype.getContext = vi.fn(() => ctx) as never
  const props = { state: "assistant_speaking", variant, energyRef: { current: { input: 0, output: 1 } }, size: 64 }
  render(<OrbCanvas {...(props as Parameters<typeof OrbCanvas>[0])} />)
  return ctx.trace.join("|")
}

let originalGetContext: HTMLCanvasElement["getContext"]

beforeEach(() => {
  originalGetContext = HTMLCanvasElement.prototype.getContext
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === "(prefers-reduced-motion: reduce)",
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }))
})

afterEach(() => {
  cleanup()
  HTMLCanvasElement.prototype.getContext = originalGetContext
  vi.unstubAllGlobals()
})

describe("the calm Talk orb catalog", () => {
  it("publishes the four named paint strategies in product order", () => {
    const catalog = (motion as unknown as { ORB_VARIANTS?: readonly string[] }).ORB_VARIANTS

    expect(catalog).toEqual(VARIANTS)
  })

  it("publishes the complete preview vocabulary, both speakers named", () => {
    expect(motion.ORB_STATES).toEqual(STATES)
  })

  it("keeps every variant static under reduced motion while preserving distinct geometry", () => {
    const raf = vi.fn(() => 1)
    vi.stubGlobal("requestAnimationFrame", raf)

    const signatures = VARIANTS.map(renderStill)

    expect(raf).not.toHaveBeenCalled()
    expect(new Set(signatures).size).toBe(VARIANTS.length)
  })
})
