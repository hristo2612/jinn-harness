import { useEffect, useState } from "react"
import type { SearchRow } from "./rows"
import type { Command, Verb } from "./verbs"

/* What a command acts on. The object is the last result the operator picked out
 * in find mode — the thing they were just looking at — and it is held across the
 * clearing of the field that typing ">" needs. A row of the wrong kind is no
 * object at all, which is what puts the picker up instead of a guess. */

/** A row a verb can act on. Recents are places the operator went, not records. */
export type CommandObject = Extract<SearchRow, { result: unknown }>

export interface CommandMode {
  command: Command
  verb: Verb | undefined
  /** The verb's object, once one of the right kind is in hand. */
  object: CommandObject | undefined
  /** The verb wants an object and has none, so the pane asks for one. */
  needsObject: boolean
  /** Answers that question. */
  chooseObject: (row: SearchRow) => void
}

/** The candidate, if it is the kind the verb takes. Anything else is no object
 *  at all, which is what the pane prompts about. */
function objectOf(row: SearchRow | undefined, kind: "todo" | "cron"): CommandObject | undefined {
  return row && row.kind !== "recent" && row.kind === kind ? row : undefined
}

export function useCommandMode(
  command: Command | null,
  /** The list's selection, which only find mode has. */
  selectedRow: SearchRow | undefined,
  /** Closing the overlay ends the session the pin belongs to. */
  open: boolean,
): CommandMode | undefined {
  const active = command !== null
  const [pinned, setPinned] = useState<SearchRow | undefined>(undefined)
  const [chosen, setChosen] = useState<SearchRow | undefined>(undefined)

  // Find mode keeps the pin current; command mode holds what it inherited,
  // because the search behind it has stood down and has no selection to offer.
  // A recent is a place the operator went, not a row a verb can write to.
  useEffect(() => {
    if (!active && selectedRow && selectedRow.kind !== "recent") setPinned(selectedRow)
  }, [active, selectedRow])

  useEffect(() => { if (!active) setChosen(undefined) }, [active])
  useEffect(() => {
    if (open) return
    setPinned(undefined)
    setChosen(undefined)
  }, [open])

  if (!command) return undefined
  const wanted = command.verb?.object
  const object = wanted ? objectOf(chosen ?? pinned, wanted) : undefined
  return {
    command,
    verb: command.verb,
    object,
    needsObject: Boolean(wanted) && !object,
    chooseObject: setChosen,
  }
}
