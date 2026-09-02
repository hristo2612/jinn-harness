import type { EngineRegistryEntry, EnginesResponse } from "@/lib/engine-registry"

/**
 * The wire shapes of UI-1's re-seat (docs/plans/ui-malleability-arc.md §4.2
 * items 1 and 6), kept beside `api.ts` as a leaf so the adapters there stay
 * one line each: the `/v1` answers they read (kebab-case keys, as every seam's
 * payloads are; unknown siblings are kept), and the opaque stand-ins for the
 * `@jinn/workflow-wire` names — that alias reached into the old daemon's source
 * and is not carried, and its consumers are out of scope, so the names survive
 * only to keep `api.ts`'s signatures and re-exports their shape.
 */

/** One engine of `GET /v1/engines`: its `describe` answer, or the error in its place. */
export interface EngineListingEntryWire {
  engine: string
  contract: string
  describe?: { provider?: string; models?: string[]; "default-model"?: string | null } | null
  error?: { code?: string; detail?: string } | null
}
export interface EngineListingWire {
  engines: EngineListingEntryWire[]
}

function engineRegistryEntryOf(entry: EngineListingEntryWire): EngineRegistryEntry {
  const models = entry.describe?.models ?? []
  return {
    name: entry.engine,
    available: !entry.error,
    defaultModel: entry.describe?.["default-model"] ?? models[0] ?? "",
    effortMechanism: "none",
    models: models.map((id) => ({ id, label: id, supportsEffort: false, effortLevels: [] })),
  }
}

/** The listing folded into the registry shape the settings editor reads. */
export function engineRegistryOf(listing: EngineListingWire): EnginesResponse {
  const engines = Object.fromEntries(listing.engines.map((entry) => [entry.engine, engineRegistryEntryOf(entry)]))
  return { default: listing.engines[0]?.engine ?? "", engines }
}

/** One entry of a plugins catalog, as `GET /v1/plugins/{catalog}` lists it. */
export interface PluginCatalogEntryWire {
  id: string
  package?: string
  incarnation?: number
  provides?: string[]
  grants?: { source: string; values: unknown[]; qualifier: string }
  lifecycle: { state: string; reason?: unknown; "kernel-state"?: string }
}
export interface PluginCatalogListingWire {
  catalog: string
  "served-by": string
  entries: PluginCatalogEntryWire[]
  read?: { qualifier?: string }
}
export interface PluginHistoryLineWire {
  seq: number
  "wall-ms": number
  entry: string
  kind: string
  payload: unknown
  sensitivity: string
}
export interface PluginHistoryWire {
  plugin: string
  lines: PluginHistoryLineWire[]
  window?: { from: number; to: number; scanned: number; truncated: boolean }
  qualifier?: string
}

/* The `@jinn/workflow-wire` stand-ins. */
type WorkflowWireObject = Record<string, unknown>
export type WorkflowBindingWire = WorkflowWireObject
export type WorkflowPredicateWire = WorkflowWireObject
export type JsonValueWire = WorkflowWireObject
export type WorkflowApprovalWire = WorkflowWireObject
export type WorkflowAttemptStatusWire = WorkflowWireObject
export type WorkflowAttemptWire = WorkflowWireObject
export type WorkflowChildRunWire = WorkflowWireObject
export type WorkflowDefinitionWire = WorkflowWireObject
export type WorkflowDefinitionSummaryWire = WorkflowWireObject
export type WorkflowRunErrorWire = WorkflowWireObject
export type WorkflowNodeWire = WorkflowWireObject
export type WorkflowNodeOutputWire = WorkflowWireObject
export type WorkflowNodeRunWire = WorkflowWireObject
export type WorkflowNodeRunStatusWire = WorkflowWireObject
export type WorkflowOutputSchemaWire = WorkflowWireObject
export type WorkflowRunDetailUnprojectedWire = WorkflowWireObject
export type WorkflowRunDetailWire = WorkflowWireObject
export type WorkflowRunLeanWire = WorkflowWireObject
export type WorkflowRunStatusWire = WorkflowWireObject
export type WorkflowRunSummaryWire = WorkflowWireObject
export type WorkflowTriggerKindWire = WorkflowWireObject
export type WorkflowIssueWire = WorkflowWireObject
