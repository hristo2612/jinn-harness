import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { FolderOpen, History } from "lucide-react"
import { api } from "@/lib/api"
import { ToggleSwitch } from "../shared"
import type { PluginInventoryRow, PluginStatus } from "./inventory"

/** UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 4): why every control on
 *  a row is disabled. The operator API writes config only; a plugin's shape is
 *  a profile edit. */
export const READ_ONLY_REASON =
  "FINDINGS #37 / KG-1 (PLA-348): a plugin's shape is a profile edit; the operator API writes config only"

/** An inventory row with the catalog's lifecycle reading beside it. */
export type CatalogRow = PluginInventoryRow & {
  state?: string
  incarnation?: number
  package?: string
  provides?: string[]
  /** UI-2 (§9.2 item 14): the entry's declared `origin`, when it has one. */
  attestation?: { origin: string }
}

/** The pill's colour per status. Errors read as a red wash rather than a solid
 *  alarm block, the same restraint the Reset section uses. */
const STATUS_TINT: Record<PluginStatus, { label: string; fg: string; bg: string }> = {
  loaded: {
    label: "Loaded",
    fg: "var(--system-green)",
    bg: "color-mix(in srgb, var(--system-green) 13%, transparent)",
  },
  disabled: { label: "Disabled", fg: "var(--text-tertiary)", bg: "var(--fill-tertiary)" },
  error: {
    label: "Error",
    fg: "var(--system-red)",
    bg: "color-mix(in srgb, var(--system-red) 13%, transparent)",
  },
}

const KIND_LABEL: Record<PluginInventoryRow["kind"], string> = {
  client: "Dashboard only",
  "client+server": "Dashboard + gateway",
}

function StatusPill({ status }: { status: PluginStatus }) {
  const { label, fg, bg } = STATUS_TINT[status]
  return (
    <span
      data-testid={`plugin-status-${status}`}
      className="inline-flex h-[22px] flex-none items-center rounded-full px-2.5 text-[length:var(--text-caption2)] font-semibold"
      style={{ background: bg, color: fg }}
    >
      {label}
    </span>
  )
}

/** UI-2 (§9.2 item 14): who wrote an extension's source, as the entry declares
 *  it (`human` / `agent`). Rendered only when the row carries one. */
function OriginBadge({ plugin }: { plugin: CatalogRow }) {
  if (!plugin.attestation) return null
  return (
    <span
      data-testid={`plugin-origin-${plugin.id}`}
      title="The entry's declared origin: who wrote this extension's source"
      className="inline-flex h-[18px] flex-none items-center rounded-full bg-[var(--fill-tertiary)] px-2 text-[length:var(--text-caption2)] font-semibold text-[var(--text-secondary)]"
    >
      {plugin.attestation.origin}
    </span>
  )
}

/** The catalog's reading: the lifecycle state, the incarnation it rests on, and
 *  the package it names — or the inventory's kind, for a row without one. */
function Reading({ plugin }: { plugin: CatalogRow }) {
  if (plugin.state === undefined) {
    return (
      <>
        <span className="tabular-nums">v{plugin.version}</span>
        <span className="text-[var(--text-quaternary)]">·</span>
        <span className="truncate">{KIND_LABEL[plugin.kind]}</span>
      </>
    )
  }
  return (
    <>
      <span data-testid={`plugin-state-${plugin.id}`}>{plugin.state}</span>
      <span className="text-[var(--text-quaternary)]">·</span>
      <span className="tabular-nums">incarnation {plugin.incarnation ?? "none"}</span>
      {plugin.package && (
        <>
          <span className="text-[var(--text-quaternary)]">·</span>
          <span className="truncate font-[family-name:var(--font-code)]">{plugin.package}</span>
        </>
      )}
    </>
  )
}

/** What the plugin is: its name and state, what it can reach, and why it is not
 *  running when it is not. */
function PluginIdentity({ plugin }: { plugin: CatalogRow }) {
  return (
    <span className="flex min-w-0 flex-1 basis-[200px] flex-col gap-[3px]">
      <span className="flex min-w-0 items-center gap-2">
        <span className="truncate text-[length:var(--text-subheadline)] font-medium leading-[1.3] text-[var(--text-primary)]">
          {plugin.name}
        </span>
        <StatusPill status={plugin.status} />
        <OriginBadge plugin={plugin} />
      </span>
      <span className="flex min-w-0 items-center gap-[7px] text-[length:var(--text-caption1)] text-[var(--text-tertiary)]">
        <Reading plugin={plugin} />
      </span>
      {plugin.error && (
        <span
          data-testid={`plugin-error-${plugin.id}`}
          className="text-[length:var(--text-caption1)] leading-[1.4] text-[var(--system-red)]"
        >
          {plugin.error}
        </span>
      )}
    </span>
  )
}

/** The entry's ledger lines, from `GET /v1/plugins/main/{id}/history`, read
 *  only once the operator opens them. */
function PluginHistory({ id }: { id: string }) {
  const history = useQuery({
    queryKey: ["plugin-history", "main", id],
    queryFn: () => api.pluginHistory("main", id),
  })
  if (history.isPending) return <span className="text-[var(--text-tertiary)]">Reading the ledger…</span>
  if (history.isError) return <span className="text-[var(--system-red)]">{history.error.message}</span>
  const { lines, window } = history.data
  return (
    <ol data-testid={`plugin-history-${id}`} className="flex flex-col gap-0.5 font-[family-name:var(--font-code)]">
      {lines.length === 0 && <li className="text-[var(--text-tertiary)]">No lines in the window read.</li>}
      {lines.map((line) => (
        <li key={line.seq} className="flex gap-2">
          <span className="tabular-nums text-[var(--text-quaternary)]">{line.seq}</span>
          <span className="truncate">{line.kind}</span>
        </li>
      ))}
      {window?.truncated && <li className="text-[var(--text-tertiary)]">Older lines exist unread.</li>}
    </ol>
  )
}

/** Reveal, rendered disabled: there is no folder behind a catalog entry. */
function RevealButton({ name, onReveal }: { name: string; onReveal: () => void }) {
  return (
    <button
      type="button"
      aria-label={`Open the ${name} folder`}
      aria-disabled="true"
      disabled
      title={READ_ONLY_REASON}
      onClick={onReveal}
      className="grid size-[34px] place-items-center rounded-full text-[var(--text-tertiary)] opacity-50"
    >
      <FolderOpen size={15} strokeWidth={2.1} aria-hidden />
    </button>
  )
}

/** The row's controls: history opens; reveal and the switch are disabled with
 *  the finding (`READ_ONLY_REASON`) on each.
 *
 *  A broken plugin carries no switch. Its inventory row says "error" and not
 *  which of the operator's two lists it is in, so a switch here would have to
 *  pick a position it cannot know — and a control that shows a state nobody
 *  asked for is worse than no control. The row still shows, with its reason:
 *  one that vanished when it broke would be one nobody could fix. */
function RowControls({
  plugin,
  historyOpen,
  onHistory,
  onToggle,
  onReveal,
}: {
  plugin: CatalogRow
  historyOpen: boolean
  onHistory: () => void
  onToggle: (enabled: boolean) => void
  onReveal: () => void
}) {
  const decidable = plugin.status !== "error"
  return (
    <span className="flex flex-none items-center gap-1">
      <button
        type="button"
        aria-label={`${historyOpen ? "Hide" : "Show"} the ${plugin.name} history`}
        aria-expanded={historyOpen}
        onClick={onHistory}
        className="grid size-[34px] place-items-center rounded-full text-[var(--text-tertiary)] transition-colors hover:bg-[var(--fill-secondary)] hover:text-[var(--text-primary)]"
      >
        <History size={15} strokeWidth={2.1} aria-hidden />
      </button>
      <RevealButton name={plugin.name} onReveal={onReveal} />
      {/* The switch's width is held whether or not there is a switch, so the
          reveal buttons stay in one column down the list. */}
      <span
        className="flex w-[44px] flex-none justify-end opacity-50 pointer-events-none"
        aria-disabled="true"
        title={READ_ONLY_REASON}
      >
        {decidable && (
          <ToggleSwitch
            checked={plugin.status === "loaded"}
            onChange={onToggle}
            ariaLabel={plugin.status === "loaded" ? `Disable ${plugin.name}` : `Enable ${plugin.name}`}
          />
        )}
      </span>
    </span>
  )
}

/**
 * One plugin. Everything an operator needs to decide about it is on the row:
 * what it is, what it can reach, whether it is running, and why not when it is
 * not. The controls are disabled (see `READ_ONLY_REASON`); the history opens.
 */
export function PluginRow({
  plugin,
  onToggle,
  onReveal,
}: {
  plugin: CatalogRow
  onToggle: (enabled: boolean) => void
  onReveal: () => void
}) {
  const [historyOpen, setHistoryOpen] = useState(false)

  return (
    <div
      data-testid={`plugin-row-${plugin.id}`}
      className="flex min-h-[56px] flex-wrap items-center gap-x-3 gap-y-2 rounded-[13px] px-3 py-2.5 transition-colors duration-150 ease-[var(--ease-smooth)] hover:bg-[var(--fill-quaternary)]"
    >
      <PluginIdentity plugin={plugin} />
      <RowControls
        plugin={plugin}
        historyOpen={historyOpen}
        onHistory={() => setHistoryOpen((open) => !open)}
        onToggle={onToggle}
        onReveal={onReveal}
      />
      {historyOpen && (
        <div className="basis-full pl-1 text-[length:var(--text-caption1)] leading-relaxed text-[var(--text-secondary)]">
          <PluginHistory id={plugin.id} />
        </div>
      )}
    </div>
  )
}
