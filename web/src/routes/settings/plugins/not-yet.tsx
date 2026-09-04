/**
 * UI-2 (docs/plans/ui-malleability-arc.md §9.5; §9.7 amendment 8(f); PLA-353
 * ruling wic_d919b13b976b item 5): what the extension tier cannot do yet, each
 * item rendered DISABLED on the extension's row with its finding number —
 * never silently absent. The list is closed: an item leaves it when the pin
 * that answers its finding lands — "A bad extension costs its own slot · #48"
 * left at pin b1dbe8f (jinnd M2-K25, harness pin-bump 8): a looping delivery
 * now ends the extension's own instance, and the entry may declare a budget.
 * "Refuse a moment mid-restart · #47" left at pin 138fdce (jinnd M2-K26,
 * harness pin-bump 9): a moment inside an extension's restart window is
 * refused typed `restarting`, never answered unmodified. The five #37 items
 * (install, remove, disable, widen topics, swap engine) left at pin f8b285b
 * (jinnd M2-K23, harness pin-bump 10): each is one `jinn:profile-admin` write
 * from the transport, live on the row (`actions.tsx`, the switch). The list
 * is EMPTY at this pin; the mechanism stays for the next finding.
 */
export const NOT_YET: ReadonlyArray<{ label: string; reason: string }> = []

/** The tier's NOT-YET items for one extension row: disabled pills, the finding
 *  on each as its title, the number in the label so it is visible without one. */
export function NotYet({ id }: { id: string }) {
  if (NOT_YET.length === 0) return null
  return (
    <span data-testid={`plugin-not-yet-${id}`} className="flex flex-wrap items-center gap-1.5 pt-0.5">
      {NOT_YET.map((item) => (
        <button
          key={item.label}
          type="button"
          disabled
          aria-disabled="true"
          title={item.reason}
          className="inline-flex h-[20px] items-center rounded-full bg-[var(--fill-tertiary)] px-2 text-[length:var(--text-caption2)] font-medium text-[var(--text-tertiary)] opacity-60"
        >
          {item.label}
        </button>
      ))}
    </span>
  )
}
