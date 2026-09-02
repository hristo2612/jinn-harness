/* The overlay's command grammar. Command mode is entered by a leading ">" and by
 * nothing else: a plain query is never parsed for verbs, so a word that happens
 * to name one — "run the migration" — cannot stop being a search. Find winning
 * is structural here, not a tiebreak applied after the fact. */

export type VerbName = "assign" | "move" | "run" | "new"

export interface Verb {
  name: VerbName
  /** The kind of row the verb acts on. Absent where the verb needs no object. */
  object?: "todo" | "cron"
  /** The line under the verb's name in the list. */
  description: string
  /** The verb starts real work, so its form confirms: ⏎ hands the confirm the
   *  focus and pressing it is a second, deliberate keystroke. */
  confirms?: boolean
}

export const VERBS: readonly Verb[] = [
  { name: "assign", object: "todo", description: "Hand the Todo to someone" },
  { name: "move", object: "todo", description: "Change the Todo's status" },
  { name: "run", object: "cron", description: "Trigger the cron job now", confirms: true },
  { name: "new", description: "Create a Todo, titled from the rest of the line" },
]

export const COMMAND_PREFIX = ">"

export interface Command {
  /** The word after ">", lowercased. Empty while only ">" is on screen. */
  word: string
  /** The verb that word names outright, or undefined while it is still a prefix. */
  verb: Verb | undefined
  /** The rest of the line — `new`'s title, and no other verb's business. */
  argument: string
  /** The verbs the word could still become; all four for a bare ">". */
  matches: readonly Verb[]
}

/** A command, or null when the query is a search — which is every query that
 *  does not open with ">". */
export function parseCommand(query: string): Command | null {
  if (!query.startsWith(COMMAND_PREFIX)) return null
  const rest = query.slice(COMMAND_PREFIX.length)
  const space = rest.indexOf(" ")
  const word = (space === -1 ? rest : rest.slice(0, space)).toLowerCase()
  return {
    word,
    verb: VERBS.find(verb => verb.name === word),
    argument: space === -1 ? "" : rest.slice(space + 1).trim(),
    matches: VERBS.filter(verb => verb.name.startsWith(word)),
  }
}

/** What picking a verb row puts in the field: the verb, committed, with the
 *  space its argument would follow. */
export function commandFor(verb: Verb): string {
  return `${COMMAND_PREFIX}${verb.name} `
}

/** ⏎ hands the field over to the form. The confirming verb only takes the
 *  focus; the rest open their control outright. */
export function activatePrimary(verb: Verb): void {
  const control = document.querySelector<HTMLElement>("[data-command-primary]")
  if (!control) return
  control.focus()
  if (!verb.confirms) control.click()
}
