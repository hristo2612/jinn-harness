import { expect, it } from "vitest"
import { navigationInstall, navigationSettled, observeNavigationSource, type NavigationSnapshot } from "../navigation-state"
import { NAVIGATION_ENTRY, NAVIGATION_TOPIC } from "@/lib/navigation-extension"
const snapshot = (): NavigationSnapshot => ({
  entries: [{id: "ext-green", package: "ext/jinn-ext-js-boa", hash: "sha256:admitted", config: {data: {budget: {fuel: 1000}}}}],
  catalog: [{id: "ext-green", lifecycle: {state: "active"}}], witnessed: [],
})
it("uses the admitted artifact and bounded budget, requesting only topic and clock", () => {
  const record = navigationInstall(snapshot())
  expect(record.hash).toBe("sha256:admitted")
  expect(record.grants).toEqual([NAVIGATION_TOPIC, "jinn:clock"])
  expect(record.config).toMatchObject({data: {budget: {fuel: 1000}, topics: [NAVIGATION_TOPIC], origin: "agent"}})
})
it("refuses an occupied ID, missing admission or unbounded fuel", () => {
  const occupied = snapshot()
  occupied.entries.push({...occupied.entries[0], id: NAVIGATION_ENTRY})
  expect(() => navigationInstall(occupied)).toThrow("occupied")
  expect(() => navigationInstall({...snapshot(), catalog: []})).toThrow("admitted")
  const unbounded = snapshot()
  unbounded.entries[0].config.data = {budget: {fuel: 1e20}}
  expect(() => navigationInstall(unbounded)).toThrow("bounded")
})
it("never treats document absence, an old disposal or an accepted write as removal", () => {
  const s = snapshot()
  const request = {operation: "remove" as const, seq: 50, ordinal: 5}
  expect(navigationSettled(s, request)).toBe(false)
  s.witnessed = [{ordinal: 5, "committed-by": 40, to: "disposed"}]
  expect(navigationSettled(s, request)).toBe(false)
  s.witnessed.push({ordinal: 6, "committed-by": 60, to: "disposed"})
  expect(navigationSettled(s, request)).toBe(true)
  s.entries.push({...s.entries[0], id: NAVIGATION_ENTRY})
  expect(navigationSettled(s, request)).toBe(false)
})
it("requires a newly witnessed active incarnation for activation", () => {
  const s = snapshot()
  s.entries.push({...s.entries[0], id: NAVIGATION_ENTRY})
  s.catalog.push({id: NAVIGATION_ENTRY, incarnation: 8, lifecycle: {state: "active"}})
  const request = {operation: "enable" as const, seq: 50, ordinal: 5}
  expect(navigationSettled(s, request)).toBe(false)
  s.witnessed = [{ordinal: 6, "committed-by": 60, to: "active", incarnation: 7}]
  expect(navigationSettled(s, request)).toBe(false)
  s.witnessed[0].incarnation = 8
  expect(navigationSettled(s, request)).toBe(true)
})

it("a changed source needs a new Active incarnation, not just its new document", () => {
  const current = snapshot()
  current.entries.push({...current.entries[0], id: NAVIGATION_ENTRY, config:{data:{source:"first"}}})
  current.catalog.push({id:NAVIGATION_ENTRY, incarnation:8, lifecycle:{state:"active"}})
  const first = observeNavigationSource(undefined, current)
  current.entries[1].config.data = {source:"second"}
  const pending = observeNavigationSource(first, current)
  expect(pending.message).toContain("waiting")
  expect(observeNavigationSource(pending, current).message).toContain("waiting")
  current.catalog[1].incarnation = 9
  expect(observeNavigationSource(pending, current).message).toContain("fresh Active incarnation was observed")
})


it("stops awaiting source activation when the entry is disabled or removed", () => {
  const current = snapshot()
  current.entries.push({...current.entries[0], id: NAVIGATION_ENTRY, disabled: true, config: {data: {source: "second"}}})
  const pending = {source: "first", incarnation: 8, awaitingAfter: 8, message: "waiting"}
  expect(observeNavigationSource(pending, current).message).toContain("disabled")
  expect(observeNavigationSource(pending, current).awaitingAfter).toBeUndefined()
  current.entries.pop()
  expect(observeNavigationSource(pending, current).message).toContain("No stored source")
  expect(observeNavigationSource(pending, current).awaitingAfter).toBeUndefined()
})
