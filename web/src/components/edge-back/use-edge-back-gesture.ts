import { useEffect, useRef, useState } from "react"

/** How wide the strip on the left of the screen is that a back drag may start
 *  in. Measured from the physical edge rather than from `--safe-left`: the
 *  inset moves the *content* in on a notched phone, but the finger still
 *  arrives at the glass. */
export const EDGE_GUTTER_PX = 24

/** The same distance the chat row swipes use. Below it nothing moves, and the
 *  two gestures have to decide "this is a scroll" at the same moment or one of
 *  them nudges the screen while the other lets it scroll. */
export const AXIS_LOCK_PX = 8

/** Fraction of the viewport the drag has to cross for a release to go back. */
export const COMMIT_RATIO = 0.35

/** px/ms at release that goes back regardless of distance — the flick. */
export const COMMIT_VELOCITY = 0.5

/** Shorter than a frame, a sample is noise: one stray pixel over a fraction of
 *  a millisecond reads as a flick nobody performed. */
const MIN_SAMPLE_MS = 8

export interface EdgeBackView {
  width: number
}

export interface EdgeBackState {
  /** How far the live view has been dragged rightward, in px. 0 at rest. */
  offset: number
  /** null until the gesture has travelled far enough to commit to an axis. */
  axis: "horizontal" | "vertical" | null
  /** Where the finger went down. null = no gesture in progress. */
  origin: { x: number; y: number } | null
  /** Most recent accepted sample, so a release can read the speed it ended at. */
  sample: { x: number; at: number } | null
  /** Horizontal speed in px/ms, measured from the last accepted sample. */
  velocity: number
  /** What the finished gesture asked for. Set by `release`, never by a move. */
  outcome: "commit" | "cancel" | null
}

export type EdgeBackEvent =
  | { kind: "down"; x: number; y: number; at: number }
  | { kind: "move"; x: number; y: number; at: number }
  | { kind: "release"; x: number; at: number }

export const AT_REST: EdgeBackState = {
  offset: 0,
  axis: null,
  origin: null,
  sample: null,
  velocity: 0,
  outcome: null,
}

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max)

/** Which way the gesture is going, or null while it is still too small to tell.
 *  A vertical lock is final for the rest of the gesture: a screen that starts
 *  sliding halfway through a scroll is exactly what makes a web app feel unlike
 *  a native one. */
function lockAxis(dx: number, dy: number): EdgeBackState["axis"] {
  if (Math.max(Math.abs(dx), Math.abs(dy)) < AXIS_LOCK_PX) return null
  return Math.abs(dx) > Math.abs(dy) ? "horizontal" : "vertical"
}

function track(state: EdgeBackState, event: { x: number; y: number; at: number }, view: EdgeBackView): EdgeBackState {
  const { origin, sample } = state
  if (!origin || !sample) return state
  const axis = state.axis ?? lockAxis(event.x - origin.x, event.y - origin.y)
  if (axis !== "horizontal") return axis === state.axis ? state : { ...state, axis }

  const elapsed = event.at - sample.at
  const sampled = elapsed >= MIN_SAMPLE_MS
  return {
    ...state,
    axis,
    offset: clamp(event.x - origin.x, 0, view.width),
    sample: sampled ? { x: event.x, at: event.at } : sample,
    velocity: sampled ? (event.x - sample.x) / elapsed : state.velocity,
  }
}

/** Whether the finger lifted having asked to go back. Distance is the deliberate
 *  drag; velocity is the flick that never travels far but clearly means it.
 *
 *  The lift is a sample like any other: a pointerup normally follows its last
 *  pointermove within a millisecond or two, too close together to measure, and
 *  the speed the drag was already carrying stands. Far enough apart and the
 *  finger has been resting — a drag that stopped dead half a second ago is not a
 *  flick, however fast it was travelling before it stopped. */
function settle(state: EdgeBackState, event: { x: number; at: number }, view: EdgeBackView): EdgeBackState {
  const { sample } = state
  if (state.axis !== "horizontal" || !sample) return AT_REST

  const elapsed = event.at - sample.at
  const velocity = elapsed >= MIN_SAMPLE_MS ? (event.x - sample.x) / elapsed : state.velocity
  const committed = state.offset >= view.width * COMMIT_RATIO || velocity >= COMMIT_VELOCITY
  return { ...state, axis: null, origin: null, sample: null, velocity, outcome: committed ? "commit" : "cancel" }
}

/** The whole gesture, as a pure transition — so the thresholds are testable
 *  without a browser, a pointer, or a rendered shell. */
export function reduceEdgeBack(state: EdgeBackState, event: EdgeBackEvent, view: EdgeBackView): EdgeBackState {
  switch (event.kind) {
    case "down":
      // Everything past the gutter belongs to the page, not to the shell.
      if (event.x > EDGE_GUTTER_PX) return AT_REST
      return { ...AT_REST, origin: { x: event.x, y: event.y }, sample: { x: event.x, at: event.at } }
    case "move":
      return track(state, event, view)
    case "release":
      return state.origin ? settle(state, event, view) : state
  }
}

export interface EdgeBackHandlers {
  /** Every horizontal frame of the drag, in px from rest. */
  onMove: (offset: number) => void
  /** Once per gesture. The layer settles from wherever the last move left it. */
  onRelease: (outcome: "commit" | "cancel") => void
}

/**
 * Watches the window for a back drag until the returned teardown is called. The
 * move and release listeners are attached only for as long as a gesture lasts:
 * a permanently attached non-passive `pointermove` would tax every scroll on the
 * page for a gesture that starts in 24px of it.
 */
function watchForEdgeDrag(sink: { current: EdgeBackHandlers }): () => void {
  let state = AT_REST
  let detach: (() => void) | null = null

  const reduce = (event: EdgeBackEvent) => {
    state = reduceEdgeBack(state, event, { width: window.innerWidth })
    return state
  }

  const onMove = (event: PointerEvent) => {
    const before = state
    const next = reduce({ kind: "move", x: event.clientX, y: event.clientY, at: event.timeStamp })
    if (next.axis !== "horizontal") return
    // Only now, with the drag unambiguously horizontal: suppressing before the
    // axis lock would cost the page underneath its vertical scroll.
    if (event.cancelable) event.preventDefault()
    if (next.offset !== before.offset) sink.current.onMove(next.offset)
  }

  const onRelease = (event: PointerEvent) => {
    const outcome = reduce({ kind: "release", x: event.clientX, at: event.timeStamp }).outcome
    detach?.()
    detach = null
    state = AT_REST
    if (outcome) sink.current.onRelease(outcome)
  }

  const onDown = (event: PointerEvent) => {
    // A second finger joins the gesture already running; it does not start one.
    if (detach) return
    if (!reduce({ kind: "down", x: event.clientX, y: event.clientY, at: event.timeStamp }).origin) return
    window.addEventListener("pointermove", onMove, { passive: false })
    window.addEventListener("pointerup", onRelease)
    window.addEventListener("pointercancel", onRelease)
    detach = () => {
      window.removeEventListener("pointermove", onMove)
      window.removeEventListener("pointerup", onRelease)
      window.removeEventListener("pointercancel", onRelease)
    }
  }

  window.addEventListener("pointerdown", onDown)
  return () => {
    window.removeEventListener("pointerdown", onDown)
    detach?.()
  }
}

/** Pointer plumbing for the shell's back drag, armed only while `enabled`. */
export function useEdgeBackGesture(enabled: boolean, handlers: EdgeBackHandlers): void {
  const sink = useRef(handlers)
  sink.current = handlers

  useEffect(() => (enabled ? watchForEdgeDrag(sink) : undefined), [enabled])
}

const COARSE_POINTER = "(pointer: coarse)"

/**
 * Whether the primary input is a finger. Read on the first render rather than in
 * an effect, so a phone never spends a frame with the gesture unarmed and a
 * mouse never spends one with it armed.
 */
export function useCoarsePointer(): boolean {
  const [coarse, setCoarse] = useState(() => window.matchMedia?.(COARSE_POINTER).matches === true)
  useEffect(() => {
    const query = window.matchMedia?.(COARSE_POINTER)
    if (!query) return
    const on = () => setCoarse(query.matches)
    on()
    query.addEventListener("change", on)
    return () => query.removeEventListener("change", on)
  }, [])
  return coarse
}
