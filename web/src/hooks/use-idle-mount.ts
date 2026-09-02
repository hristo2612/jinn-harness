import { useEffect, useState } from "react"

/**
 * Run a callback well AFTER the window `load` event, plus a fixed delay.
 *
 * Idle time alone is not late enough: this deliberately waits until the page has
 * fully loaded and then holds off a couple more seconds, so the work (and any
 * chunks it fetches) lands cleanly outside the first-paint / pre-interaction
 * network waterfall we measure against. Used to warm interaction-only chunks
 * (Cmd-K) and mount non-critical shell widgets without polluting the load numbers.
 */
export function runAfterLoad(callback: () => void, delay = 2500): () => void {
  if (typeof window === "undefined") {
    callback()
    return () => {}
  }

  let timer: number | undefined
  const schedule = () => {
    timer = window.setTimeout(callback, delay)
  }

  if (document.readyState === "complete") {
    schedule()
    return () => {
      if (timer !== undefined) window.clearTimeout(timer)
    }
  }

  window.addEventListener("load", schedule, { once: true })
  return () => {
    window.removeEventListener("load", schedule)
    if (timer !== undefined) window.clearTimeout(timer)
  }
}

export function useLoadDeferredMount(enabled = true, delay = 2500): boolean {
  const [mounted, setMounted] = useState(false)

  useEffect(() => {
    if (!enabled || mounted) return
    return runAfterLoad(() => setMounted(true), delay)
  }, [enabled, mounted, delay])

  return mounted
}
