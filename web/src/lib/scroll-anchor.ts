/**
 * Read-position anchoring for a scroll container.
 *
 * When content the reader is not looking at changes height — an older page
 * prepended above the transcript, a Todo re-sorting into another group — every
 * row below it moves, and holding `scrollTop` constant no longer holds the
 * reader's place. The fix is to note where a visible row sits before the change
 * and put it back afterwards.
 *
 * Safari implements no `overflow-anchor`, so on a phone this cannot be left to
 * the browser. Callers that correct continuously must check first whether the
 * browser already does it — see `useScrollAnchor`.
 */

/** How close to the top the reader must get before the next older page is requested. */
export const OLDER_LOAD_THRESHOLD_PX = 900

export interface ScrollAnchor {
  /** Attribute the rows were identified by; the restore must read the same one. */
  attribute: string
  /** The row the read position is measured against. */
  id: string | null
  /** Its distance from the scrollport's top edge when the anchor was taken. */
  offset: number
  scrollHeight: number
  scrollTop: number
  /**
   * Identity of the first rendered row at capture time. A caller that corrects
   * only for insertions above the reader compares it to tell the commit the
   * anchor was taken for from every other commit.
   */
  firstId: string | null
}

/** Anchor on the topmost row intersecting the scrollport. */
export function captureVisibleAnchor(
  node: HTMLElement,
  attribute: string,
  firstId: string | null = null,
): ScrollAnchor {
  const containerRect = node.getBoundingClientRect()
  const base = { attribute, scrollHeight: node.scrollHeight, scrollTop: node.scrollTop, firstId }
  const rows = Array.from(node.querySelectorAll<HTMLElement>(`[${attribute}]`))
  for (const row of rows) {
    const rect = row.getBoundingClientRect()
    if (rect.bottom >= containerRect.top && rect.top <= containerRect.bottom) {
      return { id: row.getAttribute(attribute), offset: rect.top - containerRect.top, ...base }
    }
  }
  return { id: null, offset: 0, ...base }
}

/**
 * Put the anchored row back where it was. Measuring the row itself absorbs
 * whatever height the change turned out to have; the scrollHeight delta is only
 * the fallback for when that row is no longer rendered — it moved into another
 * group, or out of the filter — and keeps the viewport on the content it was on
 * instead of snapping to an edge.
 */
export function restoreVisibleAnchor(node: HTMLElement, anchor: ScrollAnchor) {
  if (anchor.id) {
    const target = Array.from(node.querySelectorAll<HTMLElement>(`[${anchor.attribute}]`))
      .find((row) => row.getAttribute(anchor.attribute) === anchor.id)
    if (target) {
      const containerRect = node.getBoundingClientRect()
      node.scrollTop += target.getBoundingClientRect().top - containerRect.top - anchor.offset
      return
    }
  }
  node.scrollTop = anchor.scrollTop + (node.scrollHeight - anchor.scrollHeight)
}
