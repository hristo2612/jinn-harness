import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { SHORTCUT_CATALOG, formatShortcutKeys } from '@/lib/shortcut-catalog'
import { ShortcutsSection } from '../shortcuts-section'

describe('Settings shortcuts section', () => {
  it('lists every catalogued shortcut with its key label', () => {
    render(<ShortcutsSection />)

    const rows = screen.getAllByRole('listitem')
    expect(rows).toHaveLength(SHORTCUT_CATALOG.length)

    for (const meta of SHORTCUT_CATALOG) {
      const label = formatShortcutKeys(meta)
      const row = rows.find(r => r.textContent?.includes(meta.description) && r.textContent.includes(label))
      expect(row, `no row for ${meta.id}`).toBeTruthy()
    }
  })

  it('groups the shortcuts under their category headings', () => {
    render(<ShortcutsSection />)

    for (const category of new Set(SHORTCUT_CATALOG.map(s => s.category))) {
      expect(screen.getByRole('list', { name: category })).toBeTruthy()
    }
  })
})
