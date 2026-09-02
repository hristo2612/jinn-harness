import type { Platform } from "./contracts"
import { getPlatform } from "./platform"

const VARIABLE = "--keyboard-inset"

export function startKeyboardInset(platform: Platform = getPlatform()): () => void {
  if (typeof document === "undefined") return () => {}
  const root = document.documentElement
  root.style.setProperty(VARIABLE, "0px")

  const unsubscribe = platform.observe("viewport.keyboard-inset", (event) => {
    if (event.kind === "viewport.keyboard-inset") {
      root.style.setProperty(VARIABLE, `${event.inset}px`)
    }
  })

  return () => {
    unsubscribe()
    root.style.setProperty(VARIABLE, "0px")
  }
}
