import type { GlobalSearchWire, QueryFacetWire } from "@/lib/search-api"

const LINE = "flex flex-wrap items-center gap-[7px] px-5 pb-[13px] pl-[46px] text-[12.5px] text-[var(--text-quaternary)] max-[480px]:px-4 max-[480px]:pl-[42px]"
const CHIP = "inline-flex items-center gap-[5px] rounded-[7px] px-2 py-[3px] text-[12px]"

export interface ReadBackLineProps {
  parsed: GlobalSearchWire["parsed"]
  onRemoveFacet: (facet: QueryFacetWire) => void
  onToggleLiteral: () => void
}

/** A guess is something to take back; a typed token is already the operator's
 *  word, so it is shown as settled rather than as a thing to undo. */
function FacetChip({ facet, onRemove }: { facet: QueryFacetWire; onRemove: (facet: QueryFacetWire) => void }) {
  const shared = `${CHIP} `
  if (facet.origin === "token") {
    return (
      <span
        data-testid={`search-facet-${facet.kind}`}
        data-origin="token"
        className={`${shared}bg-[var(--accent-fill)] text-[var(--accent)]`}
      >
        {facet.value}
      </span>
    )
  }
  return (
    <button
      type="button"
      data-testid={`search-facet-${facet.kind}`}
      data-origin="inferred"
      onClick={() => onRemove(facet)}
      className={`${shared}bg-[var(--fill-tertiary)] text-[var(--text-secondary)]`}
    >
      {facet.value}
      <span aria-hidden="true" className="opacity-50">×</span>
      <span className="sr-only">Remove this filter</span>
    </button>
  )
}

/**
 * How the gateway read the query, said back. Words it turned into a filter on
 * its own are chips the operator can drop; tokens they typed are shown as
 * committed. Either way the override out of the guessing is one keypress.
 */
export function ReadBackLine({ parsed, onRemoveFacet, onToggleLiteral }: ReadBackLineProps) {
  const literalLabel = parsed.literal ? "Searching the words literally" : "Search literally"
  return (
    <div className={LINE} data-testid="search-readback">
      {parsed.literal ? (
        <span data-testid="search-readback-literal">Read as literal text</span>
      ) : (
        <>
          {parsed.facets.length > 0 && <span>Understood as</span>}
          {parsed.facets.map(facet => (
            <FacetChip key={`${facet.kind}:${facet.span.start}`} facet={facet} onRemove={onRemoveFacet} />
          ))}
        </>
      )}
      {parsed.freeText && (
        <span className="text-[var(--text-tertiary)]">
          {(parsed.literal || parsed.facets.length > 0) && "· "}searching “{parsed.freeText}”
        </span>
      )}
      <button
        type="button"
        onClick={onToggleLiteral}
        data-testid="search-literal-toggle"
        aria-pressed={parsed.literal}
        className={`ml-auto ${CHIP} ${parsed.literal ? "bg-[var(--accent-fill)] text-[var(--accent)]" : "text-[var(--text-tertiary)]"}`}
      >
        {literalLabel}
        <kbd className="font-[family-name:var(--font-code)] text-[11px]">⌘⏎</kbd>
      </button>
    </div>
  )
}
