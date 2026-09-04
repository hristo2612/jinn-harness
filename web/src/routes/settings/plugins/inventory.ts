import { useEffect } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { GATEWAY_EVENTS } from "@jinn/gateway-events"
import { useGateway } from "@/hooks/use-gateway"
import { api, type PluginCatalogEntryWire } from "@/lib/api"
import { profileAdmin } from "@/lib/profile-admin"

/**
 * The `/settings/plugins` data lane.
 *
 * UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 10, §8 amendment 6): the
 * read is the daemon's `main` plugins catalog through item 1's adapter
 * (`GET /v1/plugins/main`), folded into the inventory's own row shape by one
 * function. Pin-bump 10 (jinnd M2-K23, FINDINGS #37 closed at `f8b285b`): the
 * toggle is ONE `jinn:profile-admin` write — `PATCH /v1/profile/entries/{id}
 * { disabled }` — a disposal or a fresh incarnation, on the record. Reveal and
 * rescan still have no counterpart: a catalog entry is not a folder; they
 * refuse client-side and send nothing.
 */

export type PluginKind = "client" | "client+server"
export type PluginStatus = "loaded" | "disabled" | "error"

/** One inventory row, as the catalog's entry folds into it. Disabled and broken
 *  plugins are in it too: both are states, not absences. */
export interface PluginInventoryRow {
  id: string
  name: string
  version: string
  kind: PluginKind
  status: PluginStatus
  /** Why it failed, when `status` is `"error"`. */
  error?: string
  /** Present only for a plugin the gateway has ever run a watcher for. */
  watcher?: { status: "running" | "stopped" | "error"; detail?: string; restarts: number }
}

export const PLUGIN_INVENTORY_KEY = ["plugin-inventory"] as const

const CATALOG = "main"

const ERROR_STATES = new Set(["failed", "interrupted", "disposed", "unrecognised"])

const NOT_A_FOLDER = "a catalog entry is not a folder: nothing to reveal or rescan (the composition is the document of record, pin f8b285b)"

function statusOf(state: string): PluginStatus {
  if (state === "active") return "loaded"
  return ERROR_STATES.has(state) ? "error" : "disabled"
}

/** The reason a reading carries, spelled for the row; the kernel's own words. */
function reasonOf(lifecycle: PluginCatalogEntryWire["lifecycle"]): string | undefined {
  if (lifecycle["kernel-state"]) return `the kernel reported "${lifecycle["kernel-state"]}"`
  if (lifecycle.reason === undefined) return undefined
  return typeof lifecycle.reason === "string" ? lifecycle.reason : JSON.stringify(lifecycle.reason)
}

/** The one function: a catalog entry in the inventory's shape. */
function inventoryRowOf(entry: PluginCatalogEntryWire): PluginInventoryRow {
  const status = statusOf(entry.lifecycle.state)
  return {
    id: entry.id,
    name: entry.id,
    version: entry.incarnation === undefined ? "none" : String(entry.incarnation),
    kind: "client+server",
    status,
    ...(status === "error" ? { error: reasonOf(entry.lifecycle) } : {}),
  }
}

/** A write the daemon has no route for: refused here, nothing sent. */
function refused(): Promise<void> {
  return Promise.reject(new Error(NOT_A_FOLDER))
}

export function usePluginInventory() {
  return useQuery({
    queryKey: PLUGIN_INVENTORY_KEY,
    queryFn: async (): Promise<PluginInventoryRow[]> => (await api.listPlugins(CATALOG)).entries.map(inventoryRowOf),
  })
}

/** Re-read the inventory when the plugins directory changes under the page: a
 *  folder edited on disk changes this list without anyone touching the UI, and
 *  the gateway says so. */
export function useInventoryFollowsDisk(): void {
  const qc = useQueryClient()
  const { subscribe } = useGateway()
  useEffect(
    () =>
      subscribe((frame) => {
        if (frame.event === GATEWAY_EVENTS.pluginsChanged) {
          void qc.invalidateQueries({ queryKey: PLUGIN_INVENTORY_KEY })
        }
      }),
    [subscribe, qc],
  )
}

/** Toggle one plugin — `set-disabled`, one ledgered write — showing the new
 *  state immediately and rolling it back if the kernel refuses (typed). */
export function useTogglePlugin() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) => profileAdmin.setDisabled(id, !enabled),
    onMutate: async ({ id, enabled }) => {
      await qc.cancelQueries({ queryKey: PLUGIN_INVENTORY_KEY })
      const previous = qc.getQueryData<PluginInventoryRow[]>(PLUGIN_INVENTORY_KEY)
      qc.setQueryData<PluginInventoryRow[]>(PLUGIN_INVENTORY_KEY, (rows) =>
        rows?.map((row) =>
          // An errored plugin keeps its status: enabling it does not fix it, and
          // showing it as loaded for a moment would say it did.
          row.id === id && row.status !== "error"
            ? { ...row, status: enabled ? "loaded" : "disabled" }
            : row,
        ),
      )
      return { previous }
    },
    onError: (_error, _variables, context) => {
      if (context?.previous) qc.setQueryData(PLUGIN_INVENTORY_KEY, context.previous)
    },
    onSettled: () => void qc.invalidateQueries({ queryKey: PLUGIN_INVENTORY_KEY }),
  })
}

export function useRevealPlugin() {
  return useMutation({
    mutationFn: (_id: string) => refused(),
  })
}

export function useRescanPlugins() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => refused(),
    onSettled: () => void qc.invalidateQueries({ queryKey: PLUGIN_INVENTORY_KEY }),
  })
}
