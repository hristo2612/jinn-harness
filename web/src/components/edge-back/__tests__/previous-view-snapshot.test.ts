import { describe, expect, it } from 'vitest'
import { forgetViewsOverBudget, RETAINED_NODES, type RetainedView } from '../previous-view-snapshot'

const NODES = 100

/** The size of a route the shipped budget was measured against, and ten of them
 *  is a history a phone reaches in a minute — more than the budget can hold,
 *  which is the only case retention decides anything in. */
const VIEW_NODES = 1_503
const ENTRIES = 10

/** How many photographs are alive at once. Eviction fires on a total that is
 *  already over and runs a frame before the view standing here is photographed,
 *  so the set settles at what fits and peaks one above it. */
const CAPACITY = Math.floor(RETAINED_NODES / VIEW_NODES) + 1

/** Photographs of the given sizes, in the order they were taken. */
function trail(sizes: number[]): Map<string, RetainedView> {
  const views = new Map<string, RetainedView>()
  sizes.forEach((nodes, index) => views.set(`k${index}`, { clone: document.createElement('div'), nodes }))
  return views
}

const keysOf = (views: Map<string, RetainedView>) => [...views.keys()]

/**
 * One stop of the hook's lifecycle, in the order it runs there: retention
 * against where the cursor now stands, then the destination is read, then a
 * frame later the view standing here is photographed.
 *
 * Doing all three is the point. Filling the map once and evicting once hides
 * the failure this covers, because it never lets the cursor move after an
 * eviction has already happened.
 */
function stopAt(
  views: Map<string, RetainedView>,
  stack: string[],
  at: number,
  budget: number,
  nodes: number,
): string | null {
  forgetViewsOverBudget(views, stack, at, budget)
  const destination = at > 0 && views.has(stack[at - 1]) ? stack[at - 1] : null
  views.set(stack[at], { clone: document.createElement('div'), nodes })
  return destination
}

/** A whole session against one budget: out to the far end of the stack and then
 *  all the way back, as the destination each stop found waiting behind it. */
function walkOutAndBack(stack: string[], budget: number, nodes: number) {
  const views = new Map<string, RetainedView>()
  const out: (string | null)[] = []
  for (let at = 0; at < stack.length; at += 1) {
    const destination = stopAt(views, stack, at, budget, nodes)
    if (at > 0) out.push(destination)
  }

  const back: (string | null)[] = []
  for (let at = stack.length - 2; at >= 1; at -= 1) back.push(stopAt(views, stack, at, budget, nodes))
  return { out, back }
}

describe('forgetViewsOverBudget', () => {
  it('keeps every photograph while the total fits the budget', () => {
    const views = trail([100, 100, 100])

    forgetViewsOverBudget(views, ['k0', 'k1', 'k2'], 2, 500)

    expect(keysOf(views)).toEqual(['k0', 'k1', 'k2'])
  })

  it('counts nodes rather than entries, and drops what the gesture would want last', () => {
    const views = trail([100, 100, 100, 100])

    forgetViewsOverBudget(views, ['k0', 'k1', 'k2', 'k3'], 3, 250)

    // `k3` is the live view, whose copy the current drag cannot reveal, and `k0`
    // is four gestures away. `k2` is the destination and `k1` the stop after it.
    expect(keysOf(views)).toEqual(['k1', 'k2'])
  })

  it('never drops the view the drag would reveal, however small the budget', () => {
    const views = trail(Array.from({ length: 10 }, () => 100))

    forgetViewsOverBudget(views, keysOf(views), 2, 1)

    expect(keysOf(views)).toEqual(['k1'])
  })

  it('keeps every stop of a walk back photographed while the trail is one view over budget', () => {
    // Nine views against a budget that settles at eight: one photograph has to
    // go, and the order sends the one ahead of the cursor, which no stop of the
    // walk back was ever going to reveal. So this trail costs nothing.
    const stack = Array.from({ length: 9 }, (_, index) => `k${index}`)

    const { back } = walkOutAndBack(stack, (stack.length - 1) * NODES, NODES)

    expect(back).toEqual(['k6', 'k5', 'k4', 'k3', 'k2', 'k1', 'k0'])
  })

  it('gives every stop the reveal the budget affords, and never blinds the way out', () => {
    // Ten views this size are two more than the budget holds, and a view that is
    // no longer rendered cannot be photographed a second time. So part of the
    // walk back has to go without a reveal; what is pinned down here is how much
    // and where, because eviction beyond what the budget forces is the bug.
    const stack = Array.from({ length: ENTRIES }, (_, index) => `k${index}`)

    const afforded = walkOutAndBack(stack, RETAINED_NODES, VIEW_NODES)
    const squeezed = walkOutAndBack(stack, 1, VIEW_NODES)

    // The way out is photographed at every stop under both, because the entry
    // the drag would reveal is not a candidate whatever the budget says.
    expect(afforded.out).toEqual(stack.slice(0, -1))
    expect(squeezed.out).toEqual(stack.slice(0, -1))

    // The way back is where the budget shows: the stops nearest the cursor keep
    // their photograph, and the deepest ones — exactly the entries that did not
    // fit — drag against a backdrop instead.
    expect(afforded.back).toEqual(['k7', 'k6', 'k5', 'k4', 'k3', 'k2', null, null])
    expect(afforded.back.filter(destination => destination === null)).toHaveLength(ENTRIES - CAPACITY)
  })
})
