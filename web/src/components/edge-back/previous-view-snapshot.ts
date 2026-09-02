import { useEffect, useState, type RefObject } from "react"
import { NavigationType, useLocation, useNavigationType } from "react-router-dom"

export interface RetainedView {
  clone: HTMLElement
  /** Size of the detached tree, measured once when it is taken. */
  nodes: number
}

/**
 * Copies of the views the shell has shown, so the back gesture can reveal where
 * it is going instead of dragging the screen off a void. As many of them as the
 * budget holds, which on a long history is not all of them.
 *
 * The copies are held at module scope rather than in a ref because React
 * unmounts the whole page tree between routes — including the shell that owns
 * the gesture — so anything kept inside it would be thrown away on the very
 * navigation the snapshot exists to cover.
 *
 * They are keyed by history entry rather than kept as a single "the last view",
 * because the last view is not always the next destination: after a back
 * navigation the view that was on screen a moment ago is the one *ahead* in
 * history, and revealing it would promise a destination `navigate(-1)` is not
 * going to reach. `entries` mirrors the browser's stack so the reveal and the
 * navigation always name the same view, and so the eviction can tell an entry
 * the gesture is walking towards from one it has already left behind.
 */
const snapshots = new Map<string, RetainedView>()
let entries: string[] = []
let cursor = 0
let counted: number | null = null

/** How much detached DOM the photographs may hold between them, in nodes —
 *  roughly eight average routes. A budget in entries would treat a settings page
 *  and a long transcript as the same size, and a budget in bytes cannot be read
 *  off a detached tree at all. */
export const RETAINED_NODES = 12_000

/** Test-only: forget every retained view so a suite starts from a known state. */
export function clearPreviousViewSnapshot(): void {
  snapshots.clear()
  entries = []
  cursor = 0
  counted = null
}

/** A photograph of a view that is no longer running: nothing in it can be
 *  focused, typed into, read out, or found twice by an id lookup. */
function photograph(source: HTMLElement): RetainedView {
  const clone = source.cloneNode(true) as HTMLElement
  clone.removeAttribute("id")
  for (const element of clone.querySelectorAll("[id]")) element.removeAttribute("id")
  clone.setAttribute("aria-hidden", "true")
  clone.setAttribute("inert", "")
  clone.style.width = "100%"
  clone.style.height = "100%"
  return { clone, nodes: clone.querySelectorAll("*").length + 1 }
}

/**
 * Reads the browser's own count of where it is in its history, and reports
 * whether it went up — which is what tells a brand new entry apart from a
 * rewrite of the one being stood on.
 *
 * React Router's navigation type is not enough on its own: a route that
 * redirects on arrival pushes and then replaces inside a single commit, so by
 * the time this runs the type reads REPLACE although the stack did grow. Memory
 * routing keeps no browser history at all, and there the type is all there is.
 */
function steppedForward(navigation: NavigationType): boolean {
  const idx = window.history.state?.idx
  const now = typeof idx === "number" ? idx : null
  const forward = now !== null && counted !== null ? now > counted : navigation !== NavigationType.Replace
  counted = now
  return forward
}

/**
 * Where the history now stands, as an index into the entries we have seen. A key
 * we already know is one that was popped back or forward to. A new key either
 * opens the next position — discarding whatever was ahead of it, exactly as the
 * browser does — or rewrites the current one, in which case the view standing
 * there keeps its photograph until a fresher one is taken a frame from now.
 */
function locate(key: string, navigation: NavigationType): number {
  const forward = steppedForward(navigation)
  const known = entries.indexOf(key)
  if (known >= 0) return known
  if (entries.length === 0) {
    entries = [key]
    return 0
  }

  const index = forward ? cursor + 1 : cursor
  const standing = forward ? undefined : snapshots.get(entries[index])
  for (const dropped of entries.splice(index)) snapshots.delete(dropped)
  entries[index] = key
  if (standing) snapshots.set(key, standing)
  return index
}

/** Every entry the drag could afford to lose, least missed first: the far side
 *  of the forward half, then the trail behind the cursor read from its far end
 *  in. The view `navigate(-1)` lands on is in neither half. */
function evictionOrder(trail: readonly string[], at: number): string[] {
  return [...trail.slice(at).reverse(), ...trail.slice(0, Math.max(at - 1, 0))]
}

/**
 * Brings the retained photographs back under the budget, dropping whichever the
 * back gesture would want last.
 *
 * The gesture only ever runs the trail backwards, so an entry ahead of the
 * cursor is not on the way anywhere and goes first, and the entries behind it go
 * from the far end in, because those are the stops the finger reaches last.
 * Least-recently-visited reads as the same order and is not: walking a long
 * trail back re-photographs the view at every stop, which floats the entire
 * forward half above the predecessors the gesture is about to walk into, and the
 * trail goes dark from its far end just before the finger gets there.
 *
 * The view `navigate(-1)` lands on is never a candidate here, whatever the
 * budget says: evicting the one the drag is already revealing is what turned a
 * full history into a dead reveal.
 *
 * What that buys is a capacity rule, not a promise. The budget holds a fixed
 * number of photographs, a history longer than that cannot be held whole, and a
 * view that is no longer rendered cannot be photographed a second time. So the
 * stops nearest the cursor keep their reveal and the deepest ones lose it — by
 * arithmetic, not by fault. A stop that lost its copy drags against a plain
 * backdrop and navigates like any other.
 */
export function forgetViewsOverBudget(
  views: Map<string, RetainedView>,
  trail: readonly string[],
  at: number,
  budget = RETAINED_NODES,
): void {
  let total = 0
  for (const view of views.values()) total += view.nodes

  for (const key of evictionOrder(trail, at)) {
    if (total <= budget) return
    const view = views.get(key)
    if (!view) continue
    views.delete(key)
    total -= view.nodes
  }
}

/**
 * Whether `navigate(-1)` has anywhere to go. Read from the browser's own history
 * index where there is one, because that is the authority on what a back
 * navigation will do — and it stays right when a photograph has been evicted,
 * which is the whole point: a missing copy degrades the reveal, it does not take
 * the gesture with it. Memory routing keeps no browser history, so there the
 * entries we have watched go by are all there is.
 */
function canStepBack(): boolean {
  const idx = window.history.state?.idx
  return typeof idx === "number" ? idx > 0 : cursor > 0
}

export interface PreviousView {
  /** A copy of the view `navigate(-1)` lands on, or null when none was kept. */
  previous: HTMLElement | null
  canGoBack: boolean
}

/**
 * What the back gesture has to work with: whether it can go back at all, and a
 * photograph of where it is going when one was retained.
 *
 * Capture happens a frame after the navigation commits so the clone is of the
 * painted route rather than of the DOM mid-swap, and so the cost lands after
 * the new view is on screen rather than in front of it.
 */
export function usePreviousViewSnapshot(content: RefObject<HTMLElement | null>): PreviousView {
  const { key } = useLocation()
  const navigation = useNavigationType()
  const [view, setView] = useState<PreviousView>({ previous: null, canGoBack: false })

  useEffect(() => {
    cursor = locate(key, navigation)
    forgetViewsOverBudget(snapshots, entries, cursor)
    const behind = cursor > 0 ? snapshots.get(entries[cursor - 1]) : undefined
    setView({ previous: behind?.clone ?? null, canGoBack: canStepBack() })
    const frame = requestAnimationFrame(() => {
      if (content.current) snapshots.set(key, photograph(content.current))
    })
    return () => cancelAnimationFrame(frame)
  }, [key, navigation, content])

  return view
}
