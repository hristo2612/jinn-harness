import { act, renderHook } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import {
  AT_REST,
  AXIS_LOCK_PX,
  COMMIT_RATIO,
  COMMIT_VELOCITY,
  EDGE_GUTTER_PX,
  reduceEdgeBack,
  useEdgeBackGesture,
  type EdgeBackState,
  type EdgeBackView,
} from '../use-edge-back-gesture'

const VIEW: EdgeBackView = { width: 390 }

/** Replay a whole gesture, one sample every 16ms, so a test reads as the drag it
 *  describes. Points are `[x, y]`; the finger lifts at the end. */
function drag(points: Array<[number, number]>, stepMs = 16, view = VIEW): EdgeBackState {
  const [start, ...rest] = points
  let at = 0
  let state = reduceEdgeBack(AT_REST, { kind: 'down', x: start[0], y: start[1], at }, view)
  for (const [x, y] of rest) {
    at += stepMs
    state = reduceEdgeBack(state, { kind: 'move', x, y, at }, view)
  }
  // The finger lifts where it last was, a millisecond later — which is how a
  // real pointerup follows the pointermove before it.
  const [lastX] = points[points.length - 1]
  return reduceEdgeBack(state, { kind: 'release', x: lastX, at: at + 1 }, view)
}

describe('reduceEdgeBack', () => {
  it('never arms from a touch that starts past the edge gutter', () => {
    const mid = reduceEdgeBack(AT_REST, { kind: 'down', x: EDGE_GUTTER_PX + 1, y: 400, at: 0 }, VIEW)
    expect(mid.origin).toBeNull()

    const state = drag([[EDGE_GUTTER_PX + 1, 400], [320, 400]])
    expect(state.offset).toBe(0)
    expect(state.outcome).toBeNull()
  })

  it('follows the finger once a drag from the gutter commits to the horizontal axis', () => {
    const state = reduceEdgeBack(
      reduceEdgeBack(AT_REST, { kind: 'down', x: 6, y: 400, at: 0 }, VIEW),
      { kind: 'move', x: 126, y: 400, at: 16 },
      VIEW,
    )
    expect(state.axis).toBe('horizontal')
    expect(state.offset).toBe(120)
  })

  it('does not move at all until the finger has travelled the lock distance', () => {
    const state = reduceEdgeBack(
      reduceEdgeBack(AT_REST, { kind: 'down', x: 6, y: 400, at: 0 }, VIEW),
      { kind: 'move', x: 6 + AXIS_LOCK_PX - 1, y: 400, at: 16 },
      VIEW,
    )
    expect(state.axis).toBeNull()
    expect(state.offset).toBe(0)
  })

  it('ignores a drag that goes vertical first, so the list underneath keeps scrolling', () => {
    const state = drag([[6, 400], [6, 400 - AXIS_LOCK_PX * 2], [200, 200]])
    expect(state.offset).toBe(0)
    expect(state.outcome).toBeNull()
  })

  it('cannot be dragged left off its own edge, or right past the viewport', () => {
    const view = VIEW
    const back = reduceEdgeBack(
      reduceEdgeBack(AT_REST, { kind: 'down', x: 20, y: 400, at: 0 }, view),
      { kind: 'move', x: 0, y: 400, at: 16 },
      view,
    )
    expect(back.offset).toBe(0)

    const far = reduceEdgeBack(
      reduceEdgeBack(AT_REST, { kind: 'down', x: 6, y: 400, at: 0 }, view),
      { kind: 'move', x: view.width * 3, y: 400, at: 16 },
      view,
    )
    expect(far.offset).toBe(view.width)
  })

  it('commits when the finger is lifted past the distance threshold', () => {
    const past = VIEW.width * COMMIT_RATIO + 1
    expect(drag([[6, 400], [6 + past, 400]]).outcome).toBe('commit')
  })

  it('commits a short drag that was fast enough to be a flick', () => {
    // Two frames apart, so the last leg travels well above the velocity threshold.
    const short = VIEW.width * COMMIT_RATIO - 40
    const step = 16
    const state = drag([[6, 400], [6 + short / 2, 400], [6 + short, 400]], step)
    expect(state.offset).toBeLessThan(VIEW.width * COMMIT_RATIO)
    expect(short / 2 / step).toBeGreaterThan(COMMIT_VELOCITY)
    expect(state.outcome).toBe('commit')
  })

  it('cancels a drag that is both short and slow', () => {
    const short = VIEW.width * COMMIT_RATIO - 40
    const step = 400
    expect(short / 2 / step).toBeLessThan(COMMIT_VELOCITY)
    const state = drag([[6, 400], [6 + short / 2, 400], [6 + short, 400]], step)
    expect(state.outcome).toBe('cancel')
  })

  it('cancels a short drag that stopped dead before the finger lifted', () => {
    const travelled = AXIS_LOCK_PX * 5
    const moved = reduceEdgeBack(
      reduceEdgeBack(AT_REST, { kind: 'down', x: 6, y: 400, at: 0 }, VIEW),
      { kind: 'move', x: 6 + travelled, y: 400, at: 16 },
      VIEW,
    )
    // It arrived moving fast enough to be a flick, and short of the distance.
    expect(moved.velocity).toBeGreaterThan(COMMIT_VELOCITY)
    expect(moved.offset).toBeLessThan(VIEW.width * COMMIT_RATIO)

    const rested = reduceEdgeBack(moved, { kind: 'release', x: 6 + travelled, at: 2016 }, VIEW)
    expect(rested.outcome).toBe('cancel')
  })

  it('reports a release only once, so a cancelled pointer cannot navigate twice', () => {
    const past = VIEW.width * COMMIT_RATIO + 1
    const committed = drag([[6, 400], [6 + past, 400]])
    expect(committed.outcome).toBe('commit')

    const again = reduceEdgeBack(committed, { kind: 'release', x: 6 + past, at: 32 }, VIEW)
    expect(again).toBe(committed)
  })

  it('ignores movement that arrives without a gesture in progress', () => {
    expect(reduceEdgeBack(AT_REST, { kind: 'move', x: 200, y: 400, at: 16 }, VIEW)).toBe(AT_REST)
  })
})

describe('useEdgeBackGesture', () => {
  const handlers = () => ({ onMove: vi.fn(), onRelease: vi.fn() })

  const down = (x: number, y = 400) =>
    window.dispatchEvent(new PointerEvent('pointerdown', { clientX: x, clientY: y }))

  const move = (x: number, y = 400) => {
    const event = new PointerEvent('pointermove', { clientX: x, clientY: y, cancelable: true })
    window.dispatchEvent(event)
    return event
  }

  /** Past the distance threshold for jsdom's window, whose width is fixed. */
  const past = () => window.innerWidth * COMMIT_RATIO + 20

  it('follows a drag that starts in the gutter and commits when it is let go past the threshold', () => {
    const sink = handlers()
    renderHook(() => useEdgeBackGesture(true, sink))

    act(() => {
      down(6)
      move(6 + past())
      window.dispatchEvent(new PointerEvent('pointerup'))
    })

    expect(sink.onMove).toHaveBeenCalledWith(past())
    expect(sink.onRelease).toHaveBeenCalledWith('commit')
  })

  it('never hears a drag that starts mid-screen', () => {
    const sink = handlers()
    renderHook(() => useEdgeBackGesture(true, sink))

    act(() => {
      down(EDGE_GUTTER_PX + 1)
      move(EDGE_GUTTER_PX + 1 + past())
      window.dispatchEvent(new PointerEvent('pointerup'))
    })

    expect(sink.onMove).not.toHaveBeenCalled()
    expect(sink.onRelease).not.toHaveBeenCalled()
  })

  it('suppresses the page scroll only once the drag has locked horizontal', () => {
    renderHook(() => useEdgeBackGesture(true, handlers()))

    let undecided!: PointerEvent
    let locked!: PointerEvent
    act(() => {
      down(6)
      undecided = move(6 + AXIS_LOCK_PX - 1)
      locked = move(6 + 120)
    })

    expect(undecided.defaultPrevented).toBe(false)
    expect(locked.defaultPrevented).toBe(true)
  })

  it('leaves a vertical drag alone, so the list underneath still scrolls', () => {
    const sink = handlers()
    renderHook(() => useEdgeBackGesture(true, sink))

    let scrolling!: PointerEvent
    act(() => {
      down(6)
      scrolling = move(6, 400 - AXIS_LOCK_PX * 4)
      move(200, 100)
      window.dispatchEvent(new PointerEvent('pointerup'))
    })

    expect(scrolling.defaultPrevented).toBe(false)
    expect(sink.onMove).not.toHaveBeenCalled()
    expect(sink.onRelease).not.toHaveBeenCalled()
  })

  it('reports one release per gesture, even when the pointer is cancelled after it lifts', () => {
    const sink = handlers()
    renderHook(() => useEdgeBackGesture(true, sink))

    act(() => {
      down(6)
      move(6 + past())
      window.dispatchEvent(new PointerEvent('pointerup'))
      window.dispatchEvent(new PointerEvent('pointercancel'))
    })

    expect(sink.onRelease).toHaveBeenCalledTimes(1)
  })

  it('listens to nothing at all while it is disarmed', () => {
    const sink = handlers()
    renderHook(() => useEdgeBackGesture(false, sink))

    act(() => {
      down(6)
      move(6 + past())
      window.dispatchEvent(new PointerEvent('pointerup'))
    })

    expect(sink.onMove).not.toHaveBeenCalled()
    expect(sink.onRelease).not.toHaveBeenCalled()
  })
})
