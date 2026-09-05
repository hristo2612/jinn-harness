import { api, type PluginCatalogEntryWire } from "@/lib/api"
import { authFetch } from "@/lib/auth"
import { readProfile, type ProfileEntryWire, type EntryRecordWire } from "@/lib/profile-admin"
import { NAVIGATION_ENTRY, NAVIGATION_TOPIC, TOOLS_FIRST_SOURCE } from "@/lib/navigation-extension"

export const NAVIGATION_STATE_KEY = ["navigation-extension-state"] as const
export interface NavigationSnapshot {
  entries: ProfileEntryWire[]
  catalog: PluginCatalogEntryWire[]
  witnessed: { ordinal: number; "committed-by": number; to: string; incarnation?: number }[]
}
export interface NavigationRequest {
  operation: "add" | "enable" | "disable" | "remove"
  seq: number
  ordinal: number
}

export async function readNavigationSnapshot(): Promise<NavigationSnapshot> {
  const [document, catalog, response] = await Promise.all([
    readProfile(), api.listPlugins("main"), authFetch(`/v1/plugins/main/${NAVIGATION_ENTRY}/transitions`),
  ])
  if (!response.ok) throw new Error(`Runtime evidence unavailable (${response.status})`)
  const transitions = await response.json()
  if (!Array.isArray(transitions.witnessed)) throw new Error("Runtime evidence has no witnessed transitions")
  return { entries: document.profile.entries, catalog: catalog.entries, witnessed: transitions.witnessed }
}

function boundedBudget(value: unknown): value is { fuel: number } {
  const fuel = (value as { fuel?: number } | undefined)?.fuel
  return Number.isSafeInteger(fuel) && Number(fuel) > 0 && Number(fuel) <= 4_000_000_000
}

export function navigationInstall(snapshot: NavigationSnapshot): EntryRecordWire {
  if (snapshot.entries.some(entry => entry.id === NAVIGATION_ENTRY)) throw new Error("ext-navigation is already occupied; inspect it before making changes")
  const boa = snapshot.entries.find(entry => entry.package === "ext/jinn-ext-js-boa" && snapshot.catalog.some(row => row.id === entry.id && row.lifecycle.state === "active"))
  const budget = boa?.config.data?.budget
  if (!boa || !boa.hash || !boundedBudget(budget)) {
    throw new Error("No active admitted Boa package with a bounded budget is available. Refresh after one is installed.")
  }
  return {
    id: NAVIGATION_ENTRY, package: boa.package, hash: boa.hash, parent: null, disabled: false,
    grants: [NAVIGATION_TOPIC, "jinn:clock"],
    config: { data: { topics: [NAVIGATION_TOPIC], source: TOOLS_FIRST_SOURCE, origin: "agent", budget } },
  }
}

/** Positive evidence after the request, never absence from /v1/status. */
export function navigationSettled(snapshot: NavigationSnapshot, request: NavigationRequest): boolean {
  const document = snapshot.entries.find(entry => entry.id === NAVIGATION_ENTRY)
  const runtime = snapshot.catalog.find(entry => entry.id === NAVIGATION_ENTRY)
  const after = snapshot.witnessed.filter(row => row.ordinal > request.ordinal && row["committed-by"] >= request.seq)
  if (request.operation === "remove") return !document && after.some(row => row.to === "disposed")
  if (request.operation === "disable") return document?.disabled === true && after.some(row => row.to === "disposed")
  if (!document || document.disabled || runtime?.lifecycle.state !== "active") return false
  return after.some(row => row.to === "active" && row.incarnation !== undefined && row.incarnation === runtime.incarnation)
}

export interface SourceObservation {
  source: unknown
  incarnation?: number
  awaitingAfter?: number
  message: string
}

/** A new document digest alone cannot associate the source with an Active seat. */
export function observeNavigationSource(previous: SourceObservation | undefined, snapshot: NavigationSnapshot): SourceObservation {
  const document = snapshot.entries.find(entry => entry.id === NAVIGATION_ENTRY)
  if (!document) return { source: undefined, message: "No stored source is installed." }
  const source = document.config.data?.source
  if (document.disabled) return { source, message: "Stored source is disabled; activation is not requested." }
  const runtime = snapshot.catalog.find(entry => entry.id === NAVIGATION_ENTRY)
  const incarnation = runtime?.incarnation
  const changed = previous !== undefined && source !== previous.source
  const awaitingAfter = changed ? previous.incarnation : previous?.awaitingAfter
  const message = sourceActivationMessage(runtime, awaitingAfter)
  return { source, incarnation: incarnation ?? previous?.incarnation, awaitingAfter, message }
}

function sourceActivationMessage(runtime: PluginCatalogEntryWire | undefined, awaitingAfter: number | undefined): string {
  if (awaitingAfter === undefined) return "Stored source and observed runtime are separate readings."
  return runtime?.lifecycle.state === "active" && runtime.incarnation !== undefined && runtime.incarnation > awaitingAfter
    ? "A fresh Active incarnation was observed after the source changed; delivery success is still unreported."
    : "Source changed; waiting to observe a fresh Active incarnation."
}
