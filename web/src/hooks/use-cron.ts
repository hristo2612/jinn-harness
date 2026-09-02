import { useCallback, useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'
import { api } from '@/lib/api'

export function useCronJobs() {
  return useQuery({
    queryKey: queryKeys.cron.all,
    queryFn: () => api.getCronJobs(),
  })
}

/** How long the button holds its acknowledgement, and how long the log is given
 *  to catch up before it is asked for again. */
const SETTLE_MS = 2000

/**
 * Run-now, for the cron document and the command overlay both. The
 * acknowledgement travels with the mutation because both surfaces show one, and
 * the timers are cleared on unmount — the overlay comes and goes far more often
 * than the page this was lifted out of.
 *
 * The keys invalidated are the cron document's own literals, which are not the
 * `queryKeys.cron` entries above: those name a different cache the detail page
 * does not read.
 */
export function useTriggerCronJob(id: string) {
  const qc = useQueryClient()
  const [triggered, setTriggered] = useState(false)
  const timers = useRef<number[]>([])
  const clearTimers = useCallback(() => {
    for (const timer of timers.current) window.clearTimeout(timer)
    timers.current = []
  }, [])
  useEffect(() => clearTimers, [clearTimers])

  const trigger = useMutation({
    mutationFn: () => api.triggerCronJob(id),
    onSuccess: () => {
      clearTimers()
      setTriggered(true)
      timers.current.push(
        window.setTimeout(() => setTriggered(false), SETTLE_MS),
        // The run lands in the log a beat after the trigger returns.
        window.setTimeout(() => {
          void qc.invalidateQueries({ queryKey: ['cron-runs', id] })
          void qc.invalidateQueries({ queryKey: ['cron-jobs'] })
        }, SETTLE_MS),
      )
    },
  })

  return { trigger, triggered }
}
