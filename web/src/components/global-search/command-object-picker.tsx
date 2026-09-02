import { useEffect, useState, type KeyboardEvent } from "react"
import { KIND_META } from "./kind-meta"
import { ResultList } from "./result-list"
import { resultRows, type SearchRow } from "./rows"
import { useGlobalSearch } from "./use-global-search"

/* The verb asking what it should act on. It prompts rather than guessing, and
 * it searches the one kind the verb can take — so whatever comes back is
 * already an answer. Its keys are its own: the list behind it is a mode away. */

const PROMPT: Record<"todo" | "cron", string> = {
  todo: "Which Todo?",
  cron: "Which cron job?",
}

const LABEL = "text-[10.5px] font-semibold uppercase tracking-[0.06em] text-[var(--text-quaternary)]"
const INPUT = "mt-1.5 w-full rounded-[var(--radius-md)] bg-[var(--fill-tertiary)] px-3 py-2 text-[13.5px] text-[var(--text-primary)] outline-none focus-visible:bg-[var(--fill-secondary)] placeholder:text-[var(--text-quaternary)]"

export function CommandObjectPicker({ kind, onPick }: {
  kind: "todo" | "cron"
  onPick: (row: SearchRow) => void
}) {
  const [query, setQuery] = useState("")
  const [selected, setSelected] = useState(0)
  const search = useGlobalSearch({ query, scope: kind, literal: false })
  const rows = resultRows(search.data?.results)
  const rowKey = rows.map(row => row.key).join(" ")
  useEffect(() => { setSelected(0) }, [rowKey])

  function handleKeyDown(event: KeyboardEvent) {
    if (rows.length === 0) return
    if (event.key === "Enter") {
      event.preventDefault()
      onPick(rows[selected])
      return
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return
    event.preventDefault()
    setSelected((selected + (event.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length)
  }

  const typing = query.trim().length > 0
  return (
    <div className="mt-4" data-testid="command-object-picker">
      <label className={LABEL} htmlFor="command-object-query">{PROMPT[kind]}</label>
      <input
        id="command-object-query"
        data-command-primary=""
        data-testid="command-object-query"
        value={query}
        onChange={event => setQuery(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={`Search ${KIND_META[kind].plural.toLowerCase()}`}
        className={INPUT}
      />
      <div className="mt-1.5 max-h-[228px] overflow-y-auto">
        <ResultList
          rows={rows}
          selectedIndex={selected}
          onSelect={setSelected}
          onActivate={onPick}
          emptyLabel={typing ? "No matches" : `Type to find a ${KIND_META[kind].label.toLowerCase()}`}
          loading={typing && search.isFetching && !search.data}
        />
      </div>
    </div>
  )
}
