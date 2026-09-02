import { useCallback } from "react"
import type { KeyboardEvent } from "react"

export interface SearchKeyboardOptions {
  rowCount: number
  selectedIndex: number
  onMove: (index: number) => void
  onActivate: () => void
  onToggleLiteral: () => void
}

/**
 * ↑↓ move the selection, ⏎ opens it, ⌘⏎ re-runs the query literally. Movement
 * wraps, so both ends of a list are one keypress from the other. Esc is not
 * here: the dialog owns dismissal, and clearing the query is a step before it.
 *
 * Keys struck inside the workbench or the command form are theirs: the
 * composer takes ⏎, the picker rows take ↑↓, and the object picker runs a list
 * of its own — none of which is this list's to answer.
 */
function insideOwnKeys(target: EventTarget | null): boolean {
  return target instanceof Element
    && target.closest("[data-search-workbench],[data-command-form]") !== null
}

export function useSearchKeyboard({
  rowCount, selectedIndex, onMove, onActivate, onToggleLiteral,
}: SearchKeyboardOptions) {
  return useCallback((event: KeyboardEvent) => {
    if (insideOwnKeys(event.target)) return
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      onToggleLiteral()
      return
    }
    if (rowCount === 0) return
    if (event.key === "Enter") {
      event.preventDefault()
      onActivate()
      return
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return
    event.preventDefault()
    onMove((selectedIndex + (event.key === "ArrowDown" ? 1 : -1) + rowCount) % rowCount)
  }, [rowCount, selectedIndex, onMove, onActivate, onToggleLiteral])
}
