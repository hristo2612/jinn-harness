import { useState } from "react"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { TodoDialog } from "../todo-dialog"

/** Both the dialog and Radix ask the node what animation it is running. jsdom
 *  computes none at all, so a test that wants an exit has to say so. */
function stubExitAnimation(animationName: string): void {
  const computed = window.getComputedStyle
  vi.spyOn(window, "getComputedStyle").mockImplementation((element, pseudo) => {
    const style = computed.call(window, element, pseudo)
    Object.defineProperty(style, "animationName", { value: animationName, configurable: true })
    return style
  })
}

function Harness({ onClosed }: { onClosed?: () => void }) {
  const [phase, setPhase] = useState<"gone" | "open" | "leaving">("gone")
  return (
    <>
      <button type="button" onClick={() => setPhase("open")}>Open details</button>
      {phase !== "gone" && (
        <TodoDialog
          label="Todo details"
          testId="todo-details"
          open={phase === "open"}
          onRequestClose={() => setPhase("leaving")}
          onClosed={() => { setPhase("gone"); onClosed?.() }}
        >
          <button type="button">First action</button>
          <button type="button">Last action</button>
        </TodoDialog>
      )}
    </>
  )
}

function open(): void {
  const opener = screen.getByRole("button", { name: "Open details" })
  opener.focus()
  fireEvent.click(opener)
}

afterEach(() => {
  vi.restoreAllMocks()
})

describe("TodoDialog focus scope", () => {
  it("traps focus, closes on Escape, and restores focus to the opener", async () => {
    render(<Harness />)
    const opener = screen.getByRole("button", { name: "Open details" })
    open()
    const dialog = await screen.findByRole("dialog", { name: "Todo details" })
    const first = screen.getByRole("button", { name: "First action" })
    await waitFor(() => expect(document.activeElement).toBe(dialog))

    const last = screen.getByRole("button", { name: "Last action" })
    last.focus()
    fireEvent.keyDown(last, { key: "Tab" })
    expect(document.activeElement).toBe(first)

    fireEvent.keyDown(dialog, { key: "Escape" })
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(document.activeElement).toBe(opener)
  })
})

describe("TodoDialog exit", () => {
  it("stays on screen under data-state=closed until its exit animation ends", async () => {
    stubExitAnimation("jinn-sheet-out")
    const onClosed = vi.fn()
    render(<Harness onClosed={onClosed} />)
    open()
    const dialog = await screen.findByTestId("todo-details")

    fireEvent.keyDown(dialog, { key: "Escape" })

    await waitFor(() => expect(dialog.dataset.state).toBe("closed"))
    expect(onClosed).not.toHaveBeenCalled()

    fireEvent.animationEnd(dialog, { animationName: "jinn-sheet-out" })

    await waitFor(() => expect(onClosed).toHaveBeenCalledTimes(1))
    expect(screen.queryByTestId("todo-details")).toBeNull()
  })

  it("leaves nothing behind when reduced motion removes the exit to wait for", async () => {
    // No animationend will ever fire, so holding the dialog for one would
    // strand it over the page with focus still trapped inside.
    const onClosed = vi.fn()
    render(<Harness onClosed={onClosed} />)
    open()
    const dialog = await screen.findByTestId("todo-details")

    fireEvent.keyDown(dialog, { key: "Escape" })

    await waitFor(() => expect(onClosed).toHaveBeenCalledTimes(1))
    expect(screen.queryByTestId("todo-details")).toBeNull()
  })
})
