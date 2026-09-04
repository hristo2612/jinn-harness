import { render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { PluginRow, type CatalogRow } from '../plugin-row'

/* UI-2 (docs/plans/ui-malleability-arc.md §9.7 amendment 8(d), 8(f)): an
 * extension's row renders its source breadcrumb from the catalog's
 * attestation — a stable reading, never a sliding history window — and every
 * NOT-YET item of the tier is DISABLED on the row with its finding number,
 * never silently absent; an answered finding's pill is gone (#48 at pin
 * b1dbe8f, pin-bump 8). */

vi.mock('@/lib/api', () => ({ api: { pluginHistory: vi.fn() } }))

function row(overrides: Partial<CatalogRow>): CatalogRow {
  return {
    id: 'ext-green',
    name: 'ext-green',
    version: '3',
    kind: 'client+server',
    status: 'loaded',
    state: 'active',
    incarnation: 3,
    package: 'ext/jinn-ext-js-boa',
    provides: [],
    ...overrides,
  }
}

function renderRow(plugin: CatalogRow) {
  render(<PluginRow plugin={plugin} onToggle={() => {}} onReveal={() => {}} />)
}

describe('an extension row', () => {
  it('renders the source breadcrumb from the catalog attestation', () => {
    renderRow(row({ attestation: { origin: 'human', source: 'sha256:5faf0000' } }))

    expect(screen.getByTestId('plugin-origin-ext-green').textContent).toBe('human')
    expect(screen.getByTestId('plugin-source-ext-green').textContent).toBe('source sha256:5faf0000')
  })

  it('renders every NOT-YET item disabled, each with its finding number', () => {
    renderRow(row({ attestation: { origin: 'human', source: 'sha256:5faf0000' } }))

    const notYet = within(screen.getByTestId('plugin-not-yet-ext-green')).getAllByRole('button')
    expect(notYet).toHaveLength(6)
    for (const item of notYet) {
      expect(item.getAttribute('aria-disabled')).toBe('true')
      expect((item as HTMLButtonElement).disabled).toBe(true)
    }
    const text = notYet.map((item) => `${item.textContent} ${item.getAttribute('title')}`).join('\n')
    for (const item of ['Install extension · #37', 'Remove extension · #37', 'Disable extension · #37', 'Widen topics · #37', 'Swap engine · #37', 'Refuse a moment mid-restart · #47']) {
      expect(text).toContain(item)
    }
    // Pin-bump 8 (jinnd M2-K25): #48 is answered, so its pill is gone — never a
    // disabled control for a limit that no longer exists.
    expect(text).not.toContain('#48')
  })
})

describe('a row that declares no attestation', () => {
  it('carries neither a breadcrumb nor the tier\'s NOT-YET items', () => {
    renderRow(row({ id: 'jinn-api-http', name: 'jinn-api-http', package: 'api/jinn-api-http' }))

    expect(screen.queryByTestId('plugin-source-jinn-api-http')).toBeNull()
    expect(screen.queryByTestId('plugin-not-yet-jinn-api-http')).toBeNull()
  })
})
