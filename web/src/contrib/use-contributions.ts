import { useCallback, useSyncExternalStore } from "react"
import { NO_CONTRIBUTIONS, contributions } from "./registry"
import type { ResolvedContribution } from "./types"

// A server render has no registrations to read, and the frozen empty array
// keeps that answer identical across calls.
function noContributions(): readonly ResolvedContribution[] {
  return NO_CONTRIBUTIONS
}

/**
 * The visible contributions for one area. The subscription is area-scoped, so a
 * slot re-renders only when its own area mutates.
 */
export function useContributions(area: string): readonly ResolvedContribution[] {
  const subscribe = useCallback(
    (onChange: () => void) => contributions.subscribeArea(area, onChange),
    [area],
  )
  const getSnapshot = useCallback(() => contributions.getArea(area), [area])

  return useSyncExternalStore(subscribe, getSnapshot, noContributions)
}
