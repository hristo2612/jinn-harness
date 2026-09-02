import { fireEvent } from '@testing-library/react'
import { vi } from 'vitest'

/**
 * A layout engine for any virtualised list, since jsdom has none.
 *
 * The virtualizer sizes the scrollport and every row from `offsetHeight`, which
 * jsdom reports as 0 — and a zero-height scrollport renders NO rows at all, so
 * without this a windowed list comes out empty rather than windowed. The
 * scrollport is VIEWPORT_H and a row is as tall as `rowHeight` says — one
 * number for every row, or a function when a test grows one of them — and each
 * row's rect is derived from the `translateY` the virtualizer itself wrote, so
 * what a test reads back is the list's own arithmetic and not a second model of it.
 *
 * The virtual block does not have to start at the scrollport's top: a header
 * padding or a row rendered above it push everything down by `blockOffset`,
 * which is the coordinate gap a list has to declare as `scrollMargin`. It reads
 * live, so a test can mount something above the block and watch what moves.
 *
 * Install it BEFORE rendering: the first measurement happens on mount.
 */

/** How tall a row is. A function when a test needs ONE row to change height
 *  after mount — media decoding into a row is the case that matters. */
export type RowHeight = number | ((row: HTMLElement) => number)

/** Which DOM the harness should drive — one virtualised surface's selectors. */
export interface VirtualLayoutTargets {
  /** Selector for the scrollport the virtualizer measures. */
  scroller: string
  /** Selector matching one rendered row. */
  row: string
  /** The attribute on that row carrying its identity. */
  rowId: string
}

export interface VirtualLayout {
  /** Distance from the scrollport's top edge to this row's top edge. */
  offsetOf: (rowId: string) => number
  /** Mounted rows, in DOM order, by their identity attribute. */
  mountedRowIds: () => string[]
  /** Those of them the reader can actually see. */
  visibleRowIds: () => string[]
  scrollTop: () => number
  scrollTo: (top: number) => void
  scrollToBottom: () => void
  release: () => void
}

/** How far below the scrollport's top the virtual block starts. */
export type BlockOffset = () => number

/** The one scroll position the spies and the scroller both read and write. */
interface ScrollState {
  top: number
  contentHeight: () => number
}

const domRect = (top: number, height: number): DOMRect => ({
  top, bottom: top + height, height, left: 0, right: 500, width: 500, x: 0, y: top,
  toJSON: () => ({}),
}) as DOMRect

function rowStart(host: HTMLElement): number {
  return Number(/translateY\((-?[\d.]+)px\)/.exec(host.style.transform)?.[1] ?? 0)
}

/** The spacer the rows are absolutely positioned inside — the one element whose
 *  children are the rows themselves. */
function isBlock(el: HTMLElement): boolean {
  return el.firstElementChild?.hasAttribute('data-index') === true
}

/** Teach jsdom the one geometry this harness models. Returns the undo. */
function installMeasurementSpies(
  rowHeight: RowHeight,
  viewportHeight: number,
  targets: VirtualLayoutTargets,
  state: ScrollState,
  blockOffset: BlockOffset,
): () => void {
  const isScroller = (el: HTMLElement) => el.matches(targets.scroller)
  const heightOf = typeof rowHeight === 'function' ? rowHeight : () => rowHeight
  const heightSpy = vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get')
    .mockImplementation(function (this: HTMLElement) {
      if (isScroller(this)) return viewportHeight
      return this.hasAttribute('data-index') ? heightOf(this) : 0
    })
  const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect')
    .mockImplementation(function (this: HTMLElement) {
      if (isScroller(this)) return domRect(0, viewportHeight)
      const blockTop = blockOffset() - state.top
      if (isBlock(this)) return domRect(blockTop, parseFloat(this.style.height) || 0)
      const host = this.closest<HTMLElement>('[data-index]')
      return host ? domRect(blockTop + rowStart(host), heightOf(host)) : domRect(0, 0)
    })
  return () => { heightSpy.mockRestore(); rectSpy.mockRestore() }
}

/** Give one scroller the scroll position jsdom keeps at a constant zero. */
function bindScroller(
  node: HTMLDivElement,
  viewportHeight: number,
  state: ScrollState,
  blockOffset: BlockOffset,
): void {
  Object.defineProperty(node, 'clientHeight', { configurable: true, get: () => viewportHeight })
  // The spacer the virtualizer sizes IS the scrollable content, so reading its
  // height back — plus whatever sits above it — is what a browser would report
  // for the scroller.
  state.contentHeight = () => {
    const spacer = node.querySelector<HTMLElement>('[data-index]')?.parentElement
    return spacer ? blockOffset() + (parseFloat(spacer.style.height) || 0) : 0
  }
  Object.defineProperty(node, 'scrollHeight', { configurable: true, get: () => state.contentHeight() })
  // Browsers clamp; without it a scroll past the end would stay past the end.
  Object.defineProperty(node, 'scrollTop', {
    configurable: true,
    get: () => state.top,
    set: (next: number) => {
      state.top = Math.max(0, Math.min(next, Math.max(0, state.contentHeight() - viewportHeight)))
    },
  })
  node.scrollTo = ((options: ScrollToOptions) => {
    node.scrollTop = options.top ?? 0
    fireEvent.scroll(node)
  }) as HTMLElement['scrollTo']
}

/** The three read-only questions a test asks about the mounted rows. */
function rowQueries(
  scroller: () => HTMLDivElement,
  targets: VirtualLayoutTargets,
  viewportHeight: number,
): Pick<VirtualLayout, 'offsetOf' | 'mountedRowIds' | 'visibleRowIds'> {
  const mountedRows = () => Array.from(scroller().querySelectorAll<HTMLElement>(targets.row))
  const idOf = (row: HTMLElement) => row.getAttribute(targets.rowId) ?? ''
  return {
    offsetOf: (id) => {
      const row = scroller().querySelector<HTMLElement>(`[${targets.rowId}="${id}"]`)
      if (!row) throw new Error(`row ${id} is not mounted`)
      return row.getBoundingClientRect().top
    },
    mountedRowIds: () => mountedRows().map(idOf),
    visibleRowIds: () => mountedRows()
      .filter((row) => {
        const rect = row.getBoundingClientRect()
        return rect.bottom > 0 && rect.top < viewportHeight
      })
      .map(idOf),
  }
}

export function installVirtualLayout(
  rowHeight: RowHeight,
  viewportHeight: number,
  targets: VirtualLayoutTargets,
  blockOffset: BlockOffset = () => 0,
): VirtualLayout {
  const state: ScrollState = { top: 0, contentHeight: () => 0 }
  const release = installMeasurementSpies(rowHeight, viewportHeight, targets, state, blockOffset)

  // The scroller only exists once the list has rendered, so it is bound lazily.
  let bound: HTMLDivElement | null = null
  const scroller = (): HTMLDivElement => {
    const node = document.querySelector(targets.scroller) as HTMLDivElement
    if (node !== bound) {
      bound = node
      bindScroller(node, viewportHeight, state, blockOffset)
    }
    return node
  }

  const scrollTo = (top: number) => {
    scroller().scrollTop = top
    fireEvent.scroll(scroller())
  }
  // Binding the scroller is what teaches `state` the content height, so asking
  // for it before the first `scroller()` call would answer zero.
  const contentHeight = () => {
    scroller()
    return state.contentHeight()
  }

  return {
    ...rowQueries(scroller, targets, viewportHeight),
    scrollTop: () => state.top,
    scrollTo,
    scrollToBottom: () => scrollTo(contentHeight()),
    release,
  }
}
