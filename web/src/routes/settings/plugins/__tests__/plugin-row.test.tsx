import { createElement, type ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { PluginRow, type CatalogRow } from '../plugin-row'

/* UI-2 (docs/plans/ui-malleability-arc.md §9.7 amendment 8(d), 8(f)): an
 * extension's row renders its source breadcrumb from the catalog's
 * attestation — a stable reading, never a sliding history window. Pin-bump
 * 10 (jinnd M2-K23, FINDINGS #37 closed at `f8b285b`): the five #37 pills —
 * install, remove, disable, widen topics, swap engine — are LIVE actions on
 * the row, each one `jinn:profile-admin` write through the transport, and a
 * typed refusal is rendered inline, in the kernel's words. The NOT-YET list
 * mechanism stays for future items and is empty at this pin. */

vi.mock('@/lib/api', () => ({ api: { pluginHistory: vi.fn() } }))
const admin = vi.hoisted(() => ({
  addEntry: vi.fn(),
  removeEntry: vi.fn(),
  setDisabled: vi.fn(),
  setGrants: vi.fn(),
  swapPlugin: vi.fn(),
}))
vi.mock('@/lib/profile-admin', () => ({ profileAdmin: admin }))

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
    grants: ['jinn:ui/before-send'],
    ...overrides,
  }
}

function renderRow(plugin: CatalogRow) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)
  render(<PluginRow plugin={plugin} onToggle={() => {}} onReveal={() => {}} />, { wrapper })
}

beforeEach(() => {
  for (const write of Object.values(admin)) write.mockReset()
})

describe('an extension row', () => {
  it('renders the source breadcrumb from the catalog attestation', () => {
    renderRow(row({ attestation: { origin: 'human', source: 'sha256:5faf0000' } }))

    expect(screen.getByTestId('plugin-origin-ext-green').textContent).toBe('human')
    expect(screen.getByTestId('plugin-source-ext-green').textContent).toBe('source sha256:5faf0000')
  })

  it('carries the four #37 actions live and the disable switch live, and no NOT-YET pill', () => {
    renderRow(row({ attestation: { origin: 'human', source: 'sha256:5faf0000' } }))

    const actions = within(screen.getByTestId('plugin-actions-ext-green')).getAllByRole('button')
    expect(actions.map((action) => action.textContent)).toEqual(['Install', 'Remove', 'Widen topics', 'Swap engine'])
    for (const action of actions) {
      expect(action.getAttribute('aria-disabled')).not.toBe('true')
      expect((action as HTMLButtonElement).disabled).toBe(false)
    }
    const toggle = screen.getByRole('switch', { name: 'Disable ext-green' })
    expect(toggle.getAttribute('aria-disabled')).not.toBe('true')
    expect(screen.queryByTestId('plugin-not-yet-ext-green')).toBeNull()
    expect(document.body.textContent).not.toContain('#37')
  })

  it("stands every action and form control at the settings page's 34 px control height, no hairline at rest", () => {
    renderRow(row({}))

    const actions = within(screen.getByTestId('plugin-actions-ext-green')).getAllByRole('button')
    fireEvent.click(actions[0])
    const form = screen.getByTestId('plugin-action-form-ext-green')
    const controls = [...actions, ...within(form).getAllByRole('button'), ...within(form).getAllByRole('textbox')]
    expect(controls).toHaveLength(4 + 2 + 5)
    for (const control of controls) {
      expect(control.className).toContain('h-[34px]')
      expect(control.className).not.toMatch(/h-\[(22|30)px\]|shadow-\[inset/)
    }
  })

  it('swaps the engine as one write: package and hash through PATCH', async () => {
    admin.swapPlugin.mockResolvedValue({ 'api-version': '0.3.0', id: 'ext-green', write: 'swap-plugin', 'administered-seq': 9 })
    renderRow(row({}))

    fireEvent.click(screen.getByRole('button', { name: 'Swap engine' }))
    const form = screen.getByTestId('plugin-action-form-ext-green')
    fireEvent.change(within(form).getByLabelText('Package'), { target: { value: 'ext/jinn-ext-js-other' } })
    fireEvent.change(within(form).getByLabelText('Hash'), { target: { value: 'abc123' } })
    fireEvent.click(within(form).getByRole('button', { name: 'Apply' }))

    await waitFor(() => expect(admin.swapPlugin).toHaveBeenCalledWith('ext-green', 'ext/jinn-ext-js-other', 'abc123'))
    await waitFor(() => expect(screen.queryByTestId('plugin-action-form-ext-green')).toBeNull())
  })

  it('widens topics as a grants change, the existing grants kept', async () => {
    admin.setGrants.mockResolvedValue({ 'api-version': '0.3.0', id: 'ext-green', write: 'set-grants', 'administered-seq': 10 })
    renderRow(row({}))

    fireEvent.click(screen.getByRole('button', { name: 'Widen topics' }))
    const form = screen.getByTestId('plugin-action-form-ext-green')
    fireEvent.change(within(form).getByLabelText('Topics'), { target: { value: 'jinn:ui/after-answer, jinn:ui/before-patch-settings' } })
    fireEvent.click(within(form).getByRole('button', { name: 'Apply' }))

    await waitFor(() =>
      expect(admin.setGrants).toHaveBeenCalledWith('ext-green', [
        'jinn:ui/before-send',
        'jinn:ui/after-answer',
        'jinn:ui/before-patch-settings',
      ]),
    )
  })

  it('removes after a confirmation, and installs a sibling entry with its grants', async () => {
    admin.removeEntry.mockResolvedValue({ 'api-version': '0.3.0', id: 'ext-green', write: 'remove-entry', 'administered-seq': 11 })
    admin.addEntry.mockResolvedValue({ 'api-version': '0.3.0', id: 'ext-blue', write: 'add-entry', 'administered-seq': 12 })
    renderRow(row({}))

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))
    fireEvent.click(within(screen.getByTestId('plugin-action-form-ext-green')).getByRole('button', { name: 'Apply' }))
    await waitFor(() => expect(admin.removeEntry).toHaveBeenCalledWith('ext-green'))

    fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    const form = screen.getByTestId('plugin-action-form-ext-green')
    fireEvent.change(within(form).getByLabelText('Id'), { target: { value: 'ext-blue' } })
    fireEvent.change(within(form).getByLabelText('Hash'), { target: { value: 'def456' } })
    fireEvent.click(within(form).getByRole('button', { name: 'Apply' }))
    await waitFor(() =>
      expect(admin.addEntry).toHaveBeenCalledWith(
        expect.objectContaining({ id: 'ext-blue', package: 'ext/jinn-ext-js-boa', hash: 'def456', grants: ['jinn:ui/before-send'] }),
      ),
    )
  })

  it('renders the kernel\'s typed refusal inline, in its words', async () => {
    admin.swapPlugin.mockRejectedValue(new Error('swap-plugin refused (malformed): package "ext/nowhere" was never admitted under this pin'))
    renderRow(row({}))

    fireEvent.click(screen.getByRole('button', { name: 'Swap engine' }))
    const form = screen.getByTestId('plugin-action-form-ext-green')
    fireEvent.change(within(form).getByLabelText('Hash'), { target: { value: 'zzz' } })
    fireEvent.click(within(form).getByRole('button', { name: 'Apply' }))

    const refusal = await screen.findByTestId('plugin-refusal-ext-green')
    expect(refusal.textContent).toContain('malformed')
    expect(refusal.textContent).toContain('never admitted')
    expect(refusal.getAttribute('role')).toBe('alert')
  })
})

describe('a row that declares no attestation', () => {
  it('carries no breadcrumb, no NOT-YET item, and the same live actions', () => {
    renderRow(row({ id: 'jinn-api-http', name: 'jinn-api-http', package: 'api/jinn-api-http' }))

    expect(screen.queryByTestId('plugin-source-jinn-api-http')).toBeNull()
    expect(screen.queryByTestId('plugin-not-yet-jinn-api-http')).toBeNull()
    expect(within(screen.getByTestId('plugin-actions-jinn-api-http')).getAllByRole('button')).toHaveLength(4)
  })
})
