import { act, render, screen } from '@testing-library/react'
import { BrowserRouter, MemoryRouter, Navigate, Route, Routes } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { clearPreviousViewSnapshot, RETAINED_NODES } from '../previous-view-snapshot'
import {
  edgeBack,
  edgeDrag,
  goForward,
  labelAt,
  paint,
  past,
  renderShell,
  Shell,
  short,
} from './edge-back-harness'

const liveView = () => screen.getByText('second view').parentElement as HTMLElement

/** Every `navigate(-1)` the layer asked for. Recorded through the module the
 *  layer calls, so a gesture that moves the view without navigating — or that
 *  navigates twice for one drag — reads differently from one that works. */
const stepsTaken = vi.hoisted(() => [] as number[])

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return {
    ...actual,
    useNavigate: () => {
      const navigate = actual.useNavigate() as (to: unknown, options?: unknown) => void
      return (to: unknown, options?: unknown) => {
        if (typeof to === 'number') stepsTaken.push(to)
        navigate(to, options)
      }
    },
  }
})

/** A trail of nine that cannot be retained whole, so the walk back has to keep
 *  working through evictions rather than only through a cache that never fills. */
const OVER_BUDGET_PADDING = Math.ceil(RETAINED_NODES / 8)

beforeEach(() => {
  stepsTaken.length = 0
  clearPreviousViewSnapshot()
  // The browser history is shared across a file; leaving one test's entries
  // behind would have the next one reading a count nothing is advancing.
  window.history.replaceState(null, '', '/')
})

afterEach(() => {
  vi.restoreAllMocks()
  Reflect.deleteProperty(window, 'matchMedia')
  document.documentElement.style.removeProperty('--duration-base')
})

describe('EdgeBackLayer', () => {
  it('has nothing to reveal, and so nothing to drag, on the first view', async () => {
    renderShell()
    await paint()

    edgeDrag(past())

    expect(screen.queryByTestId('edge-back-layer')).toBeNull()
    expect(screen.getByText('first view')).toBeTruthy()
  })

  it('carries the live view with the finger and reveals the previous one underneath', async () => {
    renderShell()
    await goForward()

    edgeDrag(120)

    const layer = screen.getByTestId('edge-back-layer')
    expect(layer.className).not.toContain('hidden')
    expect(liveView().style.transform).toBe('translate3d(120px, 0, 0)')
    // The layer holds a copy of the view that was on screen before this one.
    expect(layer.textContent).toContain('first view')
  })

  it('reveals the view history will go back to, not the one it just left', async () => {
    renderShell()
    await goForward()
    await goForward()

    edgeDrag(past())
    await act(async () => {
      window.dispatchEvent(new PointerEvent('pointerup'))
    })
    await paint()
    expect(screen.getByText('second view')).toBeTruthy()

    edgeDrag(120)

    // Back from here is the first view. The third one is ahead in history now,
    // and showing it would advertise a destination the gesture cannot reach.
    const layer = screen.getByTestId('edge-back-layer')
    expect(layer.textContent).toContain('first view')
    expect(layer.textContent).not.toContain('third view')
  })

  it('treats a route that redirects on arrival as a new view, not a rewrite of this one', async () => {
    // The push and the redirect's replace land in one commit, so all the router
    // reports by the time the shell looks is REPLACE — and a rewrite of the
    // current entry has nothing behind it to reveal.
    window.history.replaceState(null, '', '/a')
    render(
      <BrowserRouter>
        <Routes>
          <Route path="/a" element={<Shell label="first view" next="/b" />} />
          <Route path="/b" element={<Navigate to="/b/inner" replace />} />
          <Route path="/b/inner" element={<Shell label="second view" next="/a" />} />
        </Routes>
      </BrowserRouter>,
    )
    await goForward()

    edgeDrag(120)

    expect(screen.getByTestId('edge-back-layer').textContent).toContain('first view')
  })

  it('keeps the gutter and keeps navigating through eight backs off an over-budget trail', async () => {
    // Nine views the retention budget cannot hold together, so photographs are
    // being dropped while the trail is walked. That is the case a gesture built
    // on "there is a copy of where I am going" goes inert in, one step before
    // the finger reaches the far end.
    renderShell(true, OVER_BUDGET_PADDING)
    for (let step = 0; step < 8; step += 1) await goForward()
    expect(screen.getByText(labelAt(8))).toBeTruthy()

    const reveals: string[] = []
    for (let depth = 7; depth >= 0; depth -= 1) {
      expect(screen.getByTestId('edge-back-gutter')).toBeTruthy()
      reveals.push(screen.getByTestId('edge-back-layer').textContent ?? '')
      await edgeBack()
      expect(screen.getByText(labelAt(depth))).toBeTruthy()
      expect(stepsTaken).toHaveLength(8 - depth)
    }

    expect(stepsTaken).toEqual(Array.from({ length: 8 }, () => -1))
    // And at least one of those steps had nothing to reveal, which is the only
    // reason to walk a trail this long: the budget cannot hold nine of these, so
    // a step somewhere down here drags against an empty backdrop — and the
    // assertions above already counted its `navigate(-1)` with all the others.
    // How much of the trail goes dark is the retention unit test's business.
    expect(reveals).toContain('')
  }, 30_000)

  it('falls back to a plain backdrop, and still goes back at every step, with nothing photographed', async () => {
    // No photograph anywhere is strictly worse than anything retention can do to
    // the cache, and the gesture has to survive it: the reveal degrades, the
    // navigation does not.
    renderShell(false)
    for (let step = 0; step < 8; step += 1) await goForward()

    edgeDrag(120)
    const layer = screen.getByTestId('edge-back-layer')
    expect(layer.className).not.toContain('hidden')
    expect(layer.className).toContain('bg-background')
    expect(layer.textContent).toBe('')
    expect(screen.getByTestId('edge-back-gutter')).toBeTruthy()
    await act(async () => {
      window.dispatchEvent(new PointerEvent('pointerup'))
    })

    for (let depth = 7; depth >= 0; depth -= 1) {
      expect(screen.getByTestId('edge-back-gutter')).toBeTruthy()
      await edgeBack()
      expect(screen.getByText(labelAt(depth))).toBeTruthy()
    }
  })

  it('arms the drag on a missing photograph, and goes back exactly one entry', async () => {
    render(
      <MemoryRouter initialEntries={['/a']}>
        <Routes>
          <Route path="/a" element={<Shell label="first view" next="/b" />} />
          <Route path="/b" element={<Shell label="second view" next="/c" photographed={false} />} />
          <Route path="/c" element={<Shell label="third view" next="/a" />} />
        </Routes>
      </MemoryRouter>,
    )
    await goForward()
    await goForward()

    edgeDrag(past())

    // Nothing to reveal, but the live view still follows the finger.
    expect(screen.getByTestId('edge-back-layer').textContent).toBe('')
    expect((screen.getByText('third view').parentElement as HTMLElement).style.transform).toBe(
      `translate3d(${past()}px, 0, 0)`,
    )

    await act(async () => {
      window.dispatchEvent(new PointerEvent('pointerup'))
    })
    expect(screen.getByText('second view')).toBeTruthy()
  })

  it('does not move for a drag that starts mid-screen', async () => {
    renderShell()
    await goForward()

    act(() => {
      window.dispatchEvent(new PointerEvent('pointerdown', { clientX: 200, clientY: 400 }))
      window.dispatchEvent(new PointerEvent('pointermove', { clientX: 200 + past(), clientY: 400, cancelable: true }))
      window.dispatchEvent(new PointerEvent('pointerup'))
    })

    expect(liveView().style.transform).toBe('')
    expect(screen.getByText('second view')).toBeTruthy()
  })

  it('goes back once the drag is let go past the threshold', async () => {
    renderShell()
    await goForward()

    edgeDrag(past())
    await act(async () => {
      window.dispatchEvent(new PointerEvent('pointerup'))
    })

    expect(screen.getByText('first view')).toBeTruthy()
  })

  it('waits for the settle animation to finish before it swaps the view underneath', async () => {
    document.documentElement.style.setProperty('--duration-base', '180ms')
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      renderShell()
      await goForward()

      edgeDrag(past())
      act(() => {
        window.dispatchEvent(new PointerEvent('pointerup'))
      })
      expect(screen.getByText('second view')).toBeTruthy()

      await act(async () => {
        await vi.advanceTimersByTimeAsync(180)
      })
      expect(screen.getByText('first view')).toBeTruthy()
    } finally {
      vi.useRealTimers()
    }
  })

  it('settles back to rest with no navigation when the drag is let go short', async () => {
    renderShell()
    await goForward()

    edgeDrag(short())
    await act(async () => {
      window.dispatchEvent(new PointerEvent('pointerup'))
    })

    expect(screen.getByText('second view')).toBeTruthy()
    expect(liveView().style.transform).toBe('')
  })

  it('skips the drag animation under reduced motion, and still goes back', async () => {
    // jsdom ships no matchMedia at all, which is why the app calls it optionally.
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: (query: string) => ({
        matches: query.includes('prefers-reduced-motion'),
        addEventListener() {},
        removeEventListener() {},
      }),
    })

    renderShell()
    await goForward()

    edgeDrag(past())
    // Nothing translated and nothing was revealed: the gesture ran invisibly.
    expect(liveView().style.transform).toBe('')
    expect(screen.getByTestId('edge-back-layer').className).toContain('hidden')

    await act(async () => {
      window.dispatchEvent(new PointerEvent('pointerup'))
    })
    expect(screen.getByText('first view')).toBeTruthy()
  })
})
