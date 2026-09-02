import { act, fireEvent, render } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ChainEditor } from '../chain-editor'

/* The pointer half of reordering (the keyboard half is covered on the page, in
 * engines-section.test.tsx). jsdom has no layout, so the row slots the pointer
 * is tested against are stubbed the way the board's drag suite stubs columns. */

const ROW_HEIGHT = 40
const ROW_STEP = 44

function pointer(type: string, x: number, y: number) {
  return new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y })
}

/** Three equal rows stacked from the top of the viewport. */
function stubRowGeometry(): HTMLElement[] {
  const rows = Array.from(document.querySelectorAll<HTMLElement>('[data-chain-row]'))
  rows.forEach((row, index) => {
    const top = index * ROW_STEP
    vi.spyOn(row, 'getBoundingClientRect').mockReturnValue({
      x: 0, y: top, left: 0, top, right: 300, bottom: top + ROW_HEIGHT, width: 300, height: ROW_HEIGHT,
      toJSON: () => ({}),
    } as DOMRect)
  })
  return rows
}

let onChange: ReturnType<typeof vi.fn<(chain: string[]) => void>>

function renderEditor() {
  onChange = vi.fn<(chain: string[]) => void>()
  render(<ChainEditor engine="claude" chain={['codex', 'grok', 'pi']} options={[]} onChange={onChange} />)
  return stubRowGeometry()
}

beforeEach(() => {
  vi.restoreAllMocks()
})

describe('chain drag reorder', () => {
  it('drops a row into the slot the pointer is over', async () => {
    const rows = renderEditor()

    fireEvent.pointerDown(rows[0], { button: 0, clientX: 10, clientY: 10, pointerType: 'mouse' })
    await act(async () => { window.dispatchEvent(pointer('pointermove', 10, 20)) }) // past the lift threshold
    await act(async () => { window.dispatchEvent(pointer('pointermove', 10, 70)) }) // below row 2's midpoint
    await act(async () => { window.dispatchEvent(pointer('pointerup', 10, 70)) })

    expect(onChange).toHaveBeenCalledWith(['grok', 'codex', 'pi'])
  })

  it('commits nothing when the row is dropped back in its own slot', async () => {
    const rows = renderEditor()

    fireEvent.pointerDown(rows[0], { button: 0, clientX: 10, clientY: 10, pointerType: 'mouse' })
    await act(async () => { window.dispatchEvent(pointer('pointermove', 10, 20)) })
    await act(async () => { window.dispatchEvent(pointer('pointerup', 10, 20)) })

    expect(onChange).not.toHaveBeenCalled()
  })

  it('abandons the reorder on Escape, and the release after it commits nothing', async () => {
    const rows = renderEditor()

    fireEvent.pointerDown(rows[0], { button: 0, clientX: 10, clientY: 10, pointerType: 'mouse' })
    await act(async () => { window.dispatchEvent(pointer('pointermove', 10, 20)) })
    await act(async () => { window.dispatchEvent(pointer('pointermove', 10, 70)) })
    await act(async () => { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })) })
    await act(async () => { window.dispatchEvent(pointer('pointerup', 10, 70)) })

    expect(onChange).not.toHaveBeenCalled()
  })

  it('a press on a row control is that control, never a lift', async () => {
    const rows = renderEditor()
    const remove = rows[0].querySelector('button[aria-label="Remove Codex from the Claude chain"]')!

    fireEvent.pointerDown(remove, { button: 0, clientX: 10, clientY: 10, pointerType: 'mouse' })
    await act(async () => { window.dispatchEvent(pointer('pointermove', 10, 70)) })
    await act(async () => { window.dispatchEvent(pointer('pointerup', 10, 70)) })

    expect(onChange).not.toHaveBeenCalled()
  })
})
