import { Dialog as DialogPrimitive } from "radix-ui"
import { useCallback, useEffect, useRef } from "react"
import { cn } from "@/lib/utils"

/** Radix holds its content mounted until the exit animation ends, so the caller
 *  may stop rendering the dialog only once nothing is left on screen. Reduced
 *  motion leaves no animation to end, and waiting for an event that will never
 *  fire would strand the dialog there — so ask the node what it is running
 *  rather than assuming it runs anything. */
function useExitReport(
  open: boolean,
  content: React.RefObject<HTMLDivElement | null>,
  onClosed: () => void,
): void {
  const latest = useRef(onClosed)
  useEffect(() => {
    latest.current = onClosed
  })

  useEffect(() => {
    if (open) return
    const node = content.current
    if (!node) {
      latest.current()
      return
    }
    const { animationName } = window.getComputedStyle(node)
    if (!animationName || animationName === "none") {
      latest.current()
      return
    }
    const report = (event: AnimationEvent) => {
      if (event.target === node) latest.current()
    }
    node.addEventListener("animationend", report)
    return () => node.removeEventListener("animationend", report)
  }, [open, content])
}

/** Shared focus scope for Todo sheets and creation dialogs. Radix owns focus
 * trapping, scroll lock, Escape dispatch, outside interaction, and focus
 * restoration; callers own the dirty-close policy. */
export function TodoDialog({
  open,
  label,
  onRequestClose,
  onClosed,
  children,
  className,
  testId,
  overlayTestId,
}: {
  /** Flip to false to play the exit. Unmounting instead skips it: Radix reads
   *  the close off this prop, and a node that is already gone has no state to
   *  transition. */
  open: boolean
  label: string
  /** Escape or an outside interaction asked to leave. The caller decides
   *  whether that is granted — a dirty draft answers with a confirmation. */
  onRequestClose: () => void
  /** The exit is over and nothing of the dialog is left in the DOM. */
  onClosed: () => void
  children: React.ReactNode
  className?: string
  testId?: string
  overlayTestId?: string
}) {
  const contentRef = useRef<HTMLDivElement>(null)
  const openerRef = useRef<HTMLElement | null>(
    typeof document === "undefined" ? null : document.activeElement instanceof HTMLElement ? document.activeElement : null,
  )
  useEffect(() => () => {
    const opener = openerRef.current
    queueMicrotask(() => {
      if (opener?.isConnected) opener.focus()
    })
  }, [])
  useExitReport(open, contentRef, onClosed)
  const requestClose = useCallback(() => {
    onRequestClose()
  }, [onRequestClose])

  return (
    <DialogPrimitive.Root open={open} onOpenChange={(next) => { if (!next) requestClose() }}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-testid={overlayTestId}
          className="fixed inset-0 z-50 bg-[var(--scrim)] motion-safe:data-[state=closed]:animate-overlay-out motion-safe:data-[state=open]:animate-overlay-in md:bg-transparent"
        />
        <DialogPrimitive.Content
          ref={contentRef}
          tabIndex={-1}
          data-testid={testId}
          aria-describedby={undefined}
          onOpenAutoFocus={(event) => {
            event.preventDefault()
            contentRef.current?.focus()
          }}
          onEscapeKeyDown={(event) => {
            event.preventDefault()
            if (document.activeElement instanceof HTMLElement && document.activeElement.closest("[data-todo-field-edit]")) return
            requestClose()
          }}
          className={cn("fixed z-50 outline-none", className)}
        >
          <DialogPrimitive.Title className="sr-only">{label}</DialogPrimitive.Title>
          {children}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
