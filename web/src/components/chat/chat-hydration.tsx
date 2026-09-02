import { useEffect, useState } from 'react'

// Loading is a threshold, not a default: a transcript that arrives inside this
// window should never have announced itself at all.
const SPINNER_DELAY_MS = 250

/** How long after the route-level spinner leaves the screen a pane still counts
 *  as its continuation. One commit, with room for a slow frame. */
const HANDOFF_WINDOW_MS = 100

// The threshold above is measured from when the READER started waiting, not from
// when the pane mounted. A cold direct open waits at the route-level fallback
// first, so paying the threshold again after that fallback goes away splits one
// continuous wait into two loading states with a blank beat between them. These
// two module values are how the route's spinner hands the wait to the pane's.
let routeLoadingCount = 0
let routeLoadingEndedAt = Number.NEGATIVE_INFINITY

/** Registers the route-level loading fallback for as long as it is on screen. */
export function useRouteLoadingPresence(): void {
  useEffect(() => {
    routeLoadingCount += 1
    return () => {
      routeLoadingCount -= 1
      if (routeLoadingCount === 0) routeLoadingEndedAt = Date.now()
    }
  }, [])
}

export function __resetRouteLoadingHandoffForTests() {
  routeLoadingCount = 0
  routeLoadingEndedAt = Number.NEGATIVE_INFINITY
}

/** True when this pane is taking over a wait the route was already announcing. */
function continuesRouteLoading(): boolean {
  return routeLoadingCount > 0 || Date.now() - routeLoadingEndedAt <= HANDOFF_WINDOW_MS
}

/** True once a pending load has run long enough to be worth showing. */
export function useHydrationSpinner(pending: boolean): boolean {
  // Seeded rather than only set in the effect: an effect runs after paint, which
  // would leave a blank frame between the route spinner leaving and this one
  // arriving — the gap that made one wait read as two loading states.
  const [elapsed, setElapsed] = useState(() => pending && continuesRouteLoading())
  useEffect(() => {
    if (!pending) {
      setElapsed(false)
      return
    }
    if (continuesRouteLoading()) {
      setElapsed(true)
      return
    }
    const timer = window.setTimeout(() => setElapsed(true), SPINNER_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [pending])
  return pending && elapsed
}

/**
 * Sits over the transcript instead of replacing it — unmounting the transcript
 * to show a spinner is what blanked the chat on the first message.
 *
 * The z-index is load-bearing: the transcript is a later, positioned, opaque
 * sibling, so at `z-index: auto` it paints over this and the spinner is
 * invisible even though it is in the tree. Stays under the drop-zone overlay.
 */
export function ChatHydrationOverlay() {
  return (
    <div
      className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center"
      role="status"
      aria-label="Loading chat"
    >
      <div className="size-5 animate-spin rounded-full border-2 border-[var(--fill-tertiary)] border-t-[var(--accent)]" />
    </div>
  )
}
