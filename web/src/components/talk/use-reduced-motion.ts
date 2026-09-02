import { useEffect, useState } from "react"

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)"

/**
 * Live `prefers-reduced-motion` flag. Read on the first render, not in an effect,
 * so nothing animates for a frame before being cancelled.
 */
export function usePrefersReducedMotion(): boolean {
  const [reduce, setReduce] = useState(() => window.matchMedia?.(REDUCED_MOTION).matches === true)
  useEffect(() => {
    const mq = window.matchMedia?.(REDUCED_MOTION)
    if (!mq) return
    const on = () => setReduce(mq.matches)
    on()
    mq.addEventListener("change", on)
    return () => mq.removeEventListener("change", on)
  }, [])
  return reduce
}
