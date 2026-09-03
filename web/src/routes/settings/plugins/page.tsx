import { RefreshCw } from "lucide-react"
import { useQuery } from "@tanstack/react-query"
import { PageLayout } from "@/components/page-layout"
import { LargeTitleHeader } from "@/components/shell/large-title-header"
import { PageScaffold } from "@/components/shell/page-scaffold"
import { api, type PluginCatalogEntryWire } from "@/lib/api"
import { PluginList } from "./plugin-list"
import { READ_ONLY_REASON, type CatalogRow } from "./plugin-row"
import { PLUGIN_INVENTORY_KEY, useInventoryFollowsDisk, type PluginStatus } from "./inventory"

/* UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 4): the operator's view of
 * the daemon's `main` plugins catalog. The list is `GET /v1/plugins/main`; a
 * row's lifecycle reading (`state`, `incarnation`) and its `history` come from
 * the catalog. Enable, disable, rescan and reveal are rendered disabled with the
 * finding: the operator API writes config only. Header, row, refresh control
 * and skeleton ladder are the Cron page's, because this is the same kind of
 * list. */

const CATALOG = "main"

const ERROR_STATES = new Set(["failed", "interrupted", "disposed", "unrecognised"])

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

function rowOf(entry: PluginCatalogEntryWire): CatalogRow {
  const status = statusOf(entry.lifecycle.state)
  return {
    id: entry.id,
    name: entry.id,
    version: entry.incarnation === undefined ? "none" : String(entry.incarnation),
    kind: "client+server",
    status,
    ...(status === "error" ? { error: reasonOf(entry.lifecycle) } : {}),
    state: entry.lifecycle.state,
    incarnation: entry.incarnation,
    package: entry.package,
    provides: entry.provides ?? [],
    ...(entry.attestation ? { attestation: entry.attestation } : {}),
  }
}

function useCatalog() {
  return useQuery({
    queryKey: [...PLUGIN_INVENTORY_KEY, CATALOG],
    queryFn: async (): Promise<CatalogRow[]> => (await api.listPlugins(CATALOG)).entries.map(rowOf),
  })
}

function Header({ installed, enabled, busy }: {
  installed: number | null
  enabled: number
  busy: boolean
}) {
  return (
    <LargeTitleHeader
      title="Plugins"
      subtitle={
        installed === null
          ? "Everything the daemon's main catalog lists, and what it reads as active"
          : `${installed} listed · ${enabled} active`
      }
      trailing={
        <button
          type="button"
          aria-label="Rescan the plugins folder"
          aria-disabled="true"
          disabled
          title={READ_ONLY_REASON}
          className="grid size-[34px] place-items-center rounded-full text-[var(--text-tertiary)] opacity-50"
        >
          <RefreshCw size={14} strokeWidth={2.2} className={busy ? "animate-spin" : undefined} aria-hidden />
        </button>
      }
    />
  )
}

export default function PluginsSettingsPage() {
  const inventory = useCatalog()
  useInventoryFollowsDisk()

  const plugins = inventory.data ?? []

  return (
    <PageLayout>
      <PageScaffold
        contentWidth="840px"
        header={
          <Header
            installed={inventory.isSuccess ? plugins.length : null}
            enabled={plugins.filter((plugin) => plugin.status === "loaded").length}
            busy={inventory.isFetching}
          />
        }
      >
        <div>

          <div className="mt-[22px]">
            <PluginList
              inventory={inventory}
              onToggle={() => {}}
              onReveal={() => {}}
            />
            <p className="mt-3.5 px-1 text-[length:var(--text-caption1)] leading-relaxed text-[var(--text-tertiary)]">
              An enabled plugin runs with the same authority the dashboard and the gateway have. Enable only the ones
              you trust, the way you would a shell script.
            </p>
            <p
              data-testid="plugins-read-only-reason"
              className="mt-2 px-1 text-[length:var(--text-caption1)] leading-relaxed text-[var(--text-tertiary)]"
            >
              Enable, disable, rescan and reveal are disabled here: {READ_ONLY_REASON}
            </p>
          </div>
        </div>
      </PageScaffold>
    </PageLayout>
  )
}
