import { Fragment, type ReactNode } from "react"
import { FIELD_LABEL, metaFor, statusTint } from "./kind-meta"
import { MatchSnippet } from "./match-snippet"
import type { SearchRow } from "./rows"

const GROUP_HEAD = "px-[10px] pt-3 pb-[5px] text-[10.5px] font-semibold uppercase tracking-[0.07em] text-[var(--text-quaternary)]"

/** The wire title is plain text; the marked-up copy of it, when the words landed
 *  there, already comes back as a reason — so the same parser highlights both. */
function titleOf(row: SearchRow): ReactNode {
  if (row.kind === "recent") return row.recent.label
  const marked = row.result.reason.find(
    reason => (reason.field === "title" || reason.field === "name") && reason.snippet.includes("<mark>"),
  )
  return marked ? <MatchSnippet snippet={marked.snippet} /> : row.result.title
}

function StatusDot({ status }: { status: string }) {
  return <span className="size-[7px] flex-none rounded-full" style={{ background: statusTint(status) }} />
}

function sublineOf(row: SearchRow, status: string | undefined): ReactNode[] {
  if (row.kind === "recent") return [<span key="recent">Opened recently</span>]
  const { preview, reason } = row.result
  const shown = status ?? preview.status
  const parts: ReactNode[] = []
  if (row.kind === "todo") {
    parts.push(<span key="id" className="font-[family-name:var(--font-code)] text-[11px] tracking-normal">{row.result.id}</span>)
  } else if (preview.subtitle) {
    parts.push(<span key="subtitle">{preview.subtitle}</span>)
  }
  if (shown) {
    parts.push(<span key="status" className="flex items-center gap-1.5"><StatusDot status={shown} />{shown}</span>)
  }
  parts.push(<span key="field">{FIELD_LABEL[reason[0].field]}</span>)
  return parts
}

export interface ResultListProps {
  rows: SearchRow[]
  selectedIndex: number
  onSelect: (index: number) => void
  onActivate: (row: SearchRow) => void
  /** Shown in place of the rows when there are none. */
  emptyLabel: string
  loading: boolean
  /** The selected Todo's live status, once the workbench has loaded it — the
   *  same value its preview shows, so both move and revert on the one write. */
  selectedStatus?: string
}

function Row({ row, selected, status, onSelect, onActivate }: {
  row: SearchRow
  selected: boolean
  status: string | undefined
  onSelect: () => void
  onActivate: () => void
}) {
  const { Icon } = metaFor(row.kind)
  return (
    <div
      role="option"
      aria-selected={selected}
      data-testid={`search-row-${row.key}`}
      onPointerMove={onSelect}
      onClick={onActivate}
      className={`flex min-h-[42px] cursor-default items-center gap-[11px] rounded-[var(--radius-md)] px-[10px] py-2 ${selected ? "bg-[var(--accent-fill)]" : ""}`}
    >
      <span className={`grid size-6 flex-none place-items-center rounded-[7px] ${selected ? "bg-[var(--accent)] text-[var(--accent-contrast)]" : "bg-[var(--fill-tertiary)] text-[var(--text-secondary)]"}`}>
        <Icon size={13} aria-hidden="true" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14.5px] tracking-[-0.005em] text-[var(--text-primary)]">{titleOf(row)}</span>
        <span className="mt-0.5 flex items-center gap-1.5 truncate text-[12px] text-[var(--text-tertiary)]">
          {sublineOf(row, status).map((part, index) => (
            <Fragment key={index}>
              {index > 0 && <span className="text-[var(--text-quaternary)]">·</span>}
              {part}
            </Fragment>
          ))}
        </span>
      </span>
    </div>
  )
}

export function ResultList({ rows, selectedIndex, onSelect, onActivate, emptyLabel, loading, selectedStatus }: ResultListProps) {
  if (loading) {
    return (
      <div className="flex flex-col gap-1 px-[10px] pt-4" data-testid="search-list-loading">
        {[0, 1, 2, 3].map(index => (
          <div key={index} className="h-[46px] animate-pulse rounded-[var(--radius-md)] bg-[var(--fill-quaternary)]" />
        ))}
      </div>
    )
  }
  if (rows.length === 0) {
    return <p className="px-[10px] pt-4 text-[13.5px] text-[var(--text-tertiary)]" data-testid="search-list-empty">{emptyLabel}</p>
  }
  return (
    <div role="listbox" aria-label="Search results" className="flex flex-col">
      {rows.map((row, index) => (
        <Fragment key={row.key}>
          {(index === 0 || rows[index - 1].group !== row.group) && (
            <div className={GROUP_HEAD} role="presentation">{row.group}</div>
          )}
          <Row
            row={row}
            selected={index === selectedIndex}
            status={index === selectedIndex ? selectedStatus : undefined}
            onSelect={() => onSelect(index)}
            onActivate={() => onActivate(row)}
          />
        </Fragment>
      ))}
    </div>
  )
}
