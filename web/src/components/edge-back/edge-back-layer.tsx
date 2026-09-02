import { useCallback, useEffect, useRef, useState, type RefObject } from "react"
import { useLocation, useNavigate } from "react-router-dom"
import { cn } from "@/lib/utils"
import { usePrefersReducedMotion } from "../talk/use-reduced-motion"
import { usePreviousViewSnapshot } from "./previous-view-snapshot"
import { EDGE_GUTTER_PX, useEdgeBackGesture } from "./use-edge-back-gesture"

/** How far the previous view sits behind the live one at rest, as a fraction of
 *  the screen. It closes that gap as the finger crosses, which is the parallax
 *  that reads as one view lying under another rather than beside it. */
const PARALLAX = 0.25

const TRANSITION = "var(--duration-base) var(--ease-smooth)"

interface Layers {
  content: HTMLElement | null
  host: HTMLElement | null
  scrim: HTMLElement | null
}

/**
 * How long the settle takes, read from the shipped motion tokens so the CSS that
 * runs it and the navigation that has to wait for it cannot drift apart. Null
 * when no stylesheet has been applied — there is then no animation to wait for,
 * and the caller commits straight away rather than guessing a duration.
 */
function settleMs(): number | null {
  const declared = getComputedStyle(document.documentElement).getPropertyValue("--duration-base")
  const ms = Number.parseFloat(declared)
  return Number.isFinite(ms) ? ms : null
}

/** Both layers, at one point in the drag. Written straight to the elements: a
 *  state update per pointer frame is the difference between following the
 *  finger and lagging behind it. */
function paint({ content, host, scrim }: Layers, offset: number, animated: boolean): void {
  if (!content || !host || !scrim) return
  const progress = Math.min(offset / (window.innerWidth || 1), 1)
  const motion = animated ? `transform ${TRANSITION}` : "none"
  content.style.transition = motion
  content.style.transform = `translate3d(${offset}px, 0, 0)`
  content.style.zIndex = "1"
  host.style.transition = motion
  host.style.transform = `translate3d(${(progress - 1) * PARALLAX * 100}%, 0, 0)`
  scrim.style.transition = animated ? `opacity ${TRANSITION}` : "none"
  scrim.style.opacity = String(1 - progress)
}

/** Hands the live view back everything the drag borrowed from it. */
function clearDragStyles(content: HTMLElement | null): void {
  if (!content) return
  content.style.transition = ""
  content.style.transform = ""
  content.style.zIndex = ""
}

/**
 * Hands a tap back to whatever the gutter is covering.
 *
 * The gutter has to be hit-testable for its `touch-action` to count — an element
 * the browser skips during hit testing contributes nothing — so it necessarily
 * intercepts the taps that land in it, and has to pass them on.
 */
function forwardTap(event: React.MouseEvent<HTMLElement>): void {
  const gutter = event.currentTarget
  gutter.style.pointerEvents = "none"
  const covered = document.elementFromPoint(event.clientX, event.clientY)
  gutter.style.pointerEvents = ""
  if (covered instanceof HTMLElement) covered.click()
}

/** Holds the copy of the previous view inside the layer for as long as there is
 *  one. It is detached DOM, so React can only put it there by hand. */
function useMountedSnapshot(mountRef: RefObject<HTMLElement | null>, snapshot: HTMLElement | null): void {
  useEffect(() => {
    const mount = mountRef.current
    if (!mount || !snapshot) return
    mount.replaceChildren(snapshot)
    return () => mount.replaceChildren()
  }, [mountRef, snapshot])
}

/** The one pending settle. Whatever it was about to do is dropped as soon as a
 *  new drag starts or the shell goes away — a timer that outlived its router
 *  would navigate an app that is no longer listening. */
function useSettleTimer() {
  const timer = useRef<number | null>(null)

  const cancel = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current)
    timer.current = null
  }, [])

  const after = useCallback((ms: number, then: () => void) => {
    cancel()
    timer.current = window.setTimeout(then, ms)
  }, [cancel])

  useEffect(() => cancel, [cancel])
  return { after, cancel }
}

/** The gutter's click: a tap to hand on, unless it is the click a finished drag
 *  leaves behind on whatever sat under the finger. */
function useGutterTap() {
  const dragged = useRef(false)
  const onGutterTap = useCallback((event: React.MouseEvent<HTMLElement>) => {
    if (!dragged.current) return forwardTap(event)
    dragged.current = false
  }, [])
  return { dragged, onGutterTap }
}

/** The elements the drag writes to, and a reading of whatever they are now. */
function useLayerRefs(contentRef: RefObject<HTMLElement | null>) {
  const hostRef = useRef<HTMLDivElement>(null)
  const cloneRef = useRef<HTMLDivElement>(null)
  const scrimRef = useRef<HTMLDivElement>(null)
  const layers = useCallback(
    (): Layers => ({ content: contentRef.current, host: hostRef.current, scrim: scrimRef.current }),
    [contentRef],
  )
  return { hostRef, cloneRef, scrimRef, layers }
}

/** Turns the gesture's two outcomes into pixels and, for a commit, into the
 *  navigation that follows the animation rather than racing it. */
function useEdgeBackDrag(contentRef: RefObject<HTMLElement | null>) {
  const navigate = useNavigate()
  const { key } = useLocation()
  const reduced = usePrefersReducedMotion()
  const { previous, canGoBack } = usePreviousViewSnapshot(contentRef)
  const { hostRef, cloneRef, scrimRef, layers } = useLayerRefs(contentRef)
  const { after: settleAfter, cancel: cancelSettle } = useSettleTimer()
  const { dragged, onGutterTap } = useGutterTap()
  const [active, setActive] = useState(false)

  const stand = useCallback(() => {
    cancelSettle()
    clearDragStyles(contentRef.current)
    setActive(false)
  }, [cancelSettle, contentRef])

  const onMove = useCallback((offset: number) => {
    dragged.current = true
    // Reduced motion asked for no drag animation, so nothing moves: the gesture
    // still tracks the finger and still decides, it just does it invisibly.
    if (reduced) return
    cancelSettle()
    setActive(true)
    paint(layers(), offset, false)
  }, [cancelSettle, dragged, layers, reduced])

  const onRelease = useCallback((outcome: "commit" | "cancel") => {
    // The layer covers the screen at the moment of the swap, so the navigation
    // happens behind it: behind a picture of the destination where one was kept,
    // behind a plain backdrop where it was not.
    const commit = () => void navigate(-1)
    const ms = reduced ? null : settleMs()
    if (ms === null) {
      stand()
      if (outcome === "commit") commit()
      return
    }
    paint(layers(), outcome === "commit" ? window.innerWidth : 0, true)
    settleAfter(ms, outcome === "commit" ? commit : stand)
  }, [layers, navigate, reduced, settleAfter, stand])

  useEdgeBackGesture(canGoBack, { onMove, onRelease })
  useMountedSnapshot(cloneRef, previous)

  // A committed gesture ends here: the route underneath has swapped.
  useEffect(() => stand(), [key, stand])

  return { active, canGoBack, hostRef, cloneRef, scrimRef, onGutterTap }
}

/**
 * The shell's back gesture: a drag from the left edge that carries the live view
 * off to the right and reveals the previous one underneath, going back when it
 * is let go past the threshold and rubber-banding home when it is not.
 *
 * It arms on there being somewhere to go back to, not on there being a
 * photograph of it. With no copy to hold, the layer is a plain backdrop and the
 * drag still navigates: a view that was never captured, or whose capture has
 * since been evicted, costs the reveal and nothing else.
 */
export function EdgeBackLayer({ contentRef }: { contentRef: RefObject<HTMLElement | null> }) {
  const { active, canGoBack, hostRef, cloneRef, scrimRef, onGutterTap } = useEdgeBackDrag(contentRef)
  if (!canGoBack) return null

  return (
    <>
      <div
        ref={hostRef}
        aria-hidden
        data-testid="edge-back-layer"
        className={cn("pointer-events-none fixed inset-0 z-0 bg-background", !active && "hidden")}
      >
        <div ref={cloneRef} className="size-full overflow-hidden" />
        <div ref={scrimRef} className="absolute inset-0 bg-[var(--scrim)]" />
      </div>
      {/* The gesture lives or dies on this strip. Without an element declaring
          `touch-action` over the gutter, the browser claims the drag as a pan a
          few pixels in and cancels the pointer stream mid-gesture. `pan-y`
          concedes exactly the scroll and keeps the horizontal, and it covers
          only the gutter, so nothing mid-screen changes. */}
      <div
        aria-hidden
        data-testid="edge-back-gutter"
        onClick={onGutterTap}
        style={{ width: EDGE_GUTTER_PX }}
        className="fixed bottom-0 left-0 top-0 z-10 touch-pan-y"
      />
    </>
  )
}
