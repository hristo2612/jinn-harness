import { describe, expect, it } from "vitest"
import { VERBS, commandFor, parseCommand } from "../verbs"

/* The layering rule, as a grammar. Command mode is entered by a leading ">" and
 * by nothing else, so a plain query is never parsed for verbs — which is what
 * makes "find wins" structural rather than a tiebreak. */

describe("parseCommand", () => {
  it("leaves a plain query alone, even when it opens with a verb word", () => {
    expect(parseCommand("run the migration")).toBeNull()
    expect(parseCommand("assign")).toBeNull()
    expect(parseCommand("new")).toBeNull()
    expect(parseCommand("")).toBeNull()
    // A ">" that is not the first character is a character, not a mode.
    expect(parseCommand("a > b")).toBeNull()
  })

  it("offers every verb for a bare '>'", () => {
    const command = parseCommand(">")
    expect(command?.matches.map(verb => verb.name)).toEqual(["assign", "move", "run", "new"])
    expect(command?.verb).toBeUndefined()
  })

  it("narrows to the verbs a partial word could still become", () => {
    expect(parseCommand(">m")?.matches.map(verb => verb.name)).toEqual(["move"])
    expect(parseCommand(">n")?.verb).toBeUndefined()
    expect(parseCommand(">zzz")?.matches).toEqual([])
  })

  it("commits the verb once the word names one outright", () => {
    expect(parseCommand(">assign")?.verb?.name).toBe("assign")
    expect(parseCommand(">MOVE")?.verb?.name).toBe("move")
  })

  it("hands `new` the rest of the line as its title", () => {
    expect(parseCommand(">new some words")?.argument).toBe("some words")
    expect(parseCommand(">new   padded  title  ")?.argument).toBe("padded  title")
    expect(parseCommand(">new")?.argument).toBe("")
  })

  it("names the object kind each verb acts on, and only `run` confirms", () => {
    expect(VERBS.map(verb => verb.object)).toEqual(["todo", "todo", "cron", undefined])
    expect(VERBS.filter(verb => verb.confirms).map(verb => verb.name)).toEqual(["run"])
  })

  it("commits a verb into the field with the space its argument follows", () => {
    expect(commandFor(VERBS[3])).toBe(">new ")
  })
})
