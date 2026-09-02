import { Fragment, type ReactNode } from "react"
import type { SearchMatchReasonWire, SearchPreviewWire } from "@/lib/search-api"
import { FIELD_ATTRIBUTION, metaFor, statusTint } from "./kind-meta"
import { MatchSnippet } from "./match-snippet"
import type { SearchRow } from "./rows"
import { Workbench } from "./workbench"
import type { TodoWorkbench } from "./use-todo-workbench"

const KICKER = "text-[10.5px] font-semibold uppercase tracking-[0.07em] text-[var(--text-quaternary)]"
const TITLE = "mt-[9px] text-[19px] font-medium leading-[1.28] tracking-[-0.014em] text-[var(--text-primary)]"
const CARD = "mt-4 rounded-[var(--radius-lg)] bg-[var(--fill-tertiary)] px-[14px] py-3"
const CARD_LABEL = "text-[11px] font-semibold uppercase tracking-[0.05em] text-[var(--text-quaternary)]"

/** Where the snippet came from. A comment says so even when the field does not,
 *  and the count of the other comments is the rest of the answer. */
function attribution(reasons: readonly SearchMatchReasonWire[]): string {
  const head = reasons[0].commentId ? FIELD_ATTRIBUTION.comment : FIELD_ATTRIBUTION[reasons[0].field]
  const others = reasons.slice(1).filter(reason => reason.field === "comment").length
  return others === 0 ? head : `${head} · also matched ${others} comment${others === 1 ? "" : "s"}`
}

function WhyCard({ reasons }: { reasons: readonly SearchMatchReasonWire[] }) {
  return (
    <div className={CARD} data-testid="search-why">
      <div className={CARD_LABEL}>Why this matched</div>
      <p className="mt-1.5 text-[13.5px] leading-[1.5] text-[var(--text-secondary)]">
        <MatchSnippet snippet={reasons[0].snippet} />
      </p>
      <p className="mt-2 text-[12px] text-[var(--text-quaternary)]" data-testid="search-why-attribution">
        {attribution(reasons)}
      </p>
    </div>
  )
}

function Notice({ children }: { children: React.ReactNode }) {
  return <div className="flex h-full flex-col justify-center px-[22px] py-5 text-[13.5px] text-[var(--text-tertiary)]">{children}</div>
}

export interface PreviewPaneProps {
  row: SearchRow | undefined
  /** The gateway's own words when it refused the query. */
  error: Error | null
  /** Shown when there is nothing selected — the state, said quietly. */
  hint: string
  literal: boolean
  onSearchLiterally: () => void
  /** Present only while a Todo row is selected; every other kind stays read-only. */
  workbench: TodoWorkbench | undefined
}

function Rejection({ message, literal, onSearchLiterally }: { message: string; literal: boolean; onSearchLiterally: () => void }) {
  return (
    <Notice>
      <p className="text-[var(--text-primary)]" data-testid="search-error">{message}</p>
      {!literal && (
        <button
          type="button"
          onClick={onSearchLiterally}
          data-testid="search-error-literal"
          className="mt-3 self-start rounded-[var(--radius-md)] bg-[var(--accent-fill)] px-3 py-1.5 text-[13px] text-[var(--accent)]"
        >
          Search literally <kbd className="ml-1 font-[family-name:var(--font-code)] text-[11px]">⌘⏎</kbd>
        </button>
      )}
    </Notice>
  )
}

/** Status, owner and whatever the kind calls its own subtitle, in that order.
 *  `status` overrides the search payload's copy once the workbench has loaded
 *  the Todo itself, so a move made here shows here. */
function metaOf(preview: SearchPreviewWire, status: string | undefined): ReactNode[] {
  const parts: ReactNode[] = []
  const shown = status ?? preview.status
  if (shown) {
    parts.push(
      <span key="status" className="flex items-center gap-1.5">
        <span className="size-[7px] flex-none rounded-full" style={{ background: statusTint(shown) }} />
        {shown}
      </span>,
    )
  }
  if (preview.owner) parts.push(<span key="owner">{preview.owner}</span>)
  if (preview.subtitle) parts.push(<span key="subtitle" className="truncate">{preview.subtitle}</span>)
  return parts
}

function RecentPreview({ label }: { label: string }) {
  return (
    <div className="px-[22px] py-5" data-testid="search-preview">
      <div className={KICKER}>Recent</div>
      <h2 className={TITLE}>{label}</h2>
      <div className={CARD}>
        <div className={CARD_LABEL}>Why this is here</div>
        <p className="mt-1.5 text-[13.5px] leading-[1.5] text-[var(--text-secondary)]">You opened this from search recently.</p>
      </div>
    </div>
  )
}

function ResultPreview({ row, workbench }: {
  row: Extract<SearchRow, { result: unknown }>
  workbench: TodoWorkbench | undefined
}) {
  const { preview, reason } = row.result
  const meta = metaFor(row.kind)
  return (
    <div className="px-[22px] py-5" data-testid="search-preview">
      <div className={KICKER}>{row.kind === "todo" ? `${meta.label} · ${row.result.id}` : meta.label}</div>
      <h2 className={TITLE}>{preview.title}</h2>
      <div className="mt-[9px] flex flex-wrap items-center gap-2 text-[12.5px] text-[var(--text-tertiary)]">
        {metaOf(preview, workbench?.status).map((part, index) => (
          <Fragment key={index}>
            {index > 0 && <span className="text-[var(--text-quaternary)]">·</span>}
            {part}
          </Fragment>
        ))}
      </div>
      <WhyCard reasons={reason} />
      {workbench && <Workbench workbench={workbench} />}
    </div>
  )
}

/** A selected row always says why it is here; the states around it say what the
 *  overlay is doing instead. Neither is ever blank. */
export function PreviewPane({ row, error, hint, literal, onSearchLiterally, workbench }: PreviewPaneProps) {
  if (error) return <Rejection message={error.message} literal={literal} onSearchLiterally={onSearchLiterally} />
  if (!row) return <Notice><p data-testid="search-preview-hint">{hint}</p></Notice>
  if (row.kind === "recent") return <RecentPreview label={row.recent.label} />
  return <ResultPreview row={row} workbench={workbench} />
}
