import { useCallback, useEffect, useRef, useState } from "react"
import { api } from "@/lib/api"
import type { Config } from "./config-shape"

/**
 * How long an edit waits before it is written. One value for the whole page: a
 * field with a delay of its own would race the field beside it to the same
 * document, and there is no Save button left to reconcile the two.
 */
export const CONFIG_COMMIT_DEBOUNCE_MS = 600

export type ConfigSaveState =
  | { phase: "idle" }
  | { phase: "saving" }
  | { phase: "saved" }
  | { phase: "failed"; message: string }

export interface ConfigCommitOptions {
  /** Why this document cannot be saved yet, or null — asked before the wire. */
  blocker: (config: Config) => string | null
  onSaved: () => void
  /**
   * The document as the daemon answered the write. UI-2 (§9.7 amendment 8(d)):
   * a save is a moment first and an extension may FOLD the patch, so the page
   * replaces its draft with this rather than saying "Saved" over what it sent.
   * Not called when a newer edit is already queued: that write goes out next
   * and brings its own answer, and replacing the draft under it would clobber
   * the very edit about to be written.
   */
  onFolded: (config: Config) => void
  onConflict: (conflict: { message: string; remedy?: string }) => void
}

interface CommitQueue {
  pending: Config | null
  inFlight: boolean
  revision: string
  timer: ReturnType<typeof setTimeout> | null
}

/** What a refused write means for the status, and whether it also needs a notice. */
function failureFor(err: { code?: string; message: string; remedy?: string }) {
  // A conflict is not a failed save, it is a save that has not happened yet: it
  // gets its own notice with the way out, and deliberately no retry.
  if (err?.code === "CONFIG_CONFLICT") {
    return {
      // The notice already carries the gateway's sentence and the Reload that
      // resolves it, so repeating it here would only say the same thing twice.
      state: { phase: "failed", message: "Not saved — reload to continue" } as const,
      conflict: { message: err.message, remedy: err.remedy },
    }
  }
  return { state: { phase: "failed", message: `Failed to save: ${err.message}` } as const }
}

/**
 * Write the queued document, if there is one and the wire is free. Runs again
 * when it settles: an edit made while that PUT was in flight holds the revision
 * the gateway has just superseded, so it goes out after it rather than beside it.
 */
function drain(
  queue: CommitQueue,
  options: ConfigCommitOptions,
  setSaveState: (state: ConfigSaveState) => void,
): void {
  const next = queue.pending
  if (next === null || queue.inFlight) return
  queue.pending = null

  const blocker = options.blocker(next)
  if (blocker) {
    setSaveState({ phase: "failed", message: blocker })
    return
  }

  queue.inFlight = true
  setSaveState({ phase: "saving" })
  api
    .updateConfig(next, queue.revision || undefined)
    .then((result) => {
      queue.revision = result?.revision ?? ""
      if (result?.config && queue.pending === null) options.onFolded(result.config as Config)
      setSaveState({ phase: "saved" })
      options.onSaved()
    })
    .catch((err) => {
      const failure = failureFor(err)
      if (failure.conflict) {
        // Sending the queued edit anyway is exactly the clobber being refused.
        queue.pending = null
        options.onConflict(failure.conflict)
      }
      setSaveState(failure.state)
    })
    .finally(() => {
      queue.inFlight = false
      drain(queue, options, setSaveState)
    })
}

/**
 * The Settings page's write path. Every edit schedules one debounced PUT of the
 * whole document carrying the revision the last read or write left behind, so a
 * hand edit made under an open page is still refused rather than overwritten.
 */
export function useConfigCommit(options: ConfigCommitOptions) {
  const latest = useRef(options)
  useEffect(() => {
    latest.current = options
  })

  const [saveState, setSaveState] = useState<ConfigSaveState>({ phase: "idle" })
  const queue = useRef<CommitQueue>({ pending: null, inFlight: false, revision: "", timer: null })

  const flush = useCallback(() => {
    queue.current.timer = null
    drain(queue.current, latest.current, setSaveState)
  }, [])

  const commit = useCallback(
    (next: Config) => {
      queue.current.pending = next
      if (queue.current.timer) clearTimeout(queue.current.timer)
      queue.current.timer = setTimeout(flush, CONFIG_COMMIT_DEBOUNCE_MS)
    },
    [flush],
  )

  /**
   * The revision the page has just read: what the next write is based on. Anything
   * still queued was built on the document that read replaced, so it is dropped with
   * it — sending it now would carry the fresh revision straight past the staleness
   * check and overwrite the very edit the reload went to fetch.
   */
  const adoptRevision = useCallback((next: string) => {
    if (queue.current.timer) clearTimeout(queue.current.timer)
    queue.current.timer = null
    queue.current.pending = null
    queue.current.revision = next
  }, [])

  // Leaving the page inside the debounce window would drop the edit silently, and
  // there is no longer a Save button whose idleness would have given it away.
  useEffect(
    () => () => {
      if (!queue.current.timer) return
      clearTimeout(queue.current.timer)
      flush()
    },
    [flush],
  )

  return { saveState, commit, adoptRevision }
}
