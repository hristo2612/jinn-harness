/**
 * UI-2 (docs/plans/ui-malleability-arc.md §9.5; §9.7 amendment 8(f); PLA-353
 * ruling wic_d919b13b976b item 5): what the extension tier cannot do yet, each
 * item rendered DISABLED on the extension's row with its finding number —
 * never silently absent. The list is closed: an item leaves it when the pin
 * that answers its finding lands.
 */
export const NOT_YET: ReadonlyArray<{ label: string; reason: string }> = [
  {
    label: "Install extension · #37",
    reason:
      "FINDINGS #37 / KG-1 (PLA-348): install needs adding an entry with grants. Waits on jinnd M2-K23.",
  },
  {
    label: "Remove extension · #37",
    reason:
      "FINDINGS #37 / KG-1 (PLA-348): remove needs deleting an entry. Waits on jinnd M2-K23.",
  },
  {
    label: "Disable extension · #37",
    reason:
      "FINDINGS #37 / KG-1 (PLA-348): disable needs changing the entry's disabled state. Waits on jinnd M2-K23.",
  },
  {
    label: "Widen topics · #37",
    reason:
      "FINDINGS #37 / KG-1 (PLA-348): widening topics also widens grants. Waits on jinnd M2-K23.",
  },
  {
    label: "Swap engine · #37",
    reason:
      "FINDINGS #37 / KG-1 (PLA-348): an engine swap changes the entry's package and hash. Waits on jinnd M2-K23.",
  },
  {
    label: "Refuse a moment mid-restart · #47",
    reason:
      "FINDINGS #47: at this pin a moment posted inside an extension's restart window is answered UNMODIFIED, never refused. Waits on jinnd M2-K26.",
  },
  {
    label: "A bad extension costs its own slot · #48",
    reason:
      "FINDINGS #48: at this pin a looping delivery spends the transport's deadline too. Waits on jinnd M2-K25 (per-delivery budget).",
  },
]

/** The tier's NOT-YET items for one extension row: disabled pills, the finding
 *  on each as its title, the number in the label so it is visible without one. */
export function NotYet({ id }: { id: string }) {
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
