import { SHORTCUT_CATALOG, SHORTCUT_CATEGORY_ORDER, formatShortcutKeys } from "@/lib/shortcut-catalog"
import { Section } from "./shared"

/**
 * The reference for every keyboard shortcut the chat surface offers. Reads from
 * the same catalog the chat page binds its actions to, so this list cannot fall
 * behind what the keys actually do.
 */
export function ShortcutsSection() {
  const groups = SHORTCUT_CATEGORY_ORDER
    .map(category => ({ category, items: SHORTCUT_CATALOG.filter(s => s.category === category) }))
    .filter(group => group.items.length > 0)

  return (
    <Section title="Keyboard Shortcuts">
      <div>
        {groups.map((group, i) => (
          <div key={group.category} className={i > 0 ? "mt-[var(--space-5)]" : undefined}>
            <div
              id={`shortcuts-${group.category}`}
              className="text-[length:var(--text-caption2)] font-[var(--weight-medium)] uppercase tracking-[var(--tracking-wide)] text-[var(--text-tertiary)] pb-[var(--space-2)]"
            >
              {group.category}
            </div>
            <ul aria-labelledby={`shortcuts-${group.category}`} className="list-none m-0 p-0">
              {group.items.map(shortcut => (
                <li
                  key={shortcut.id}
                  className="flex items-center justify-between gap-[var(--space-4)] py-[var(--space-2)]"
                >
                  <span className="min-w-0 text-[length:var(--text-subheadline)] text-[var(--text-secondary)]">
                    {shortcut.description}
                  </span>
                  <kbd className="shrink-0 inline-flex min-w-[28px] items-center justify-center rounded-[var(--radius-sm)] bg-[var(--fill-tertiary)] px-[var(--space-2)] py-[2px] font-[family-name:var(--font-mono)] text-[length:var(--text-caption1)] font-[var(--weight-medium)] text-[var(--text-primary)]">
                    {formatShortcutKeys(shortcut)}
                  </kbd>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </Section>
  )
}
