import { describe, it, expect, vi } from 'vitest'
import {
  SHORTCUT_CATALOG,
  buildShortcuts,
  formatShortcutKeys,
  type ShortcutBinding,
  type ShortcutId,
} from '../shortcut-catalog'

/** A binding for every catalogued shortcut, so a test can drift one side only. */
function allBindings(): Record<ShortcutId, ShortcutBinding> {
  const bindings = {} as Record<ShortcutId, ShortcutBinding>
  for (const meta of SHORTCUT_CATALOG) bindings[meta.id] = { action: vi.fn() }
  return bindings
}

describe('formatShortcutKeys', () => {
  it('uppercases a single-letter key', () => {
    expect(formatShortcutKeys({ key: 'j' })).toBe('J')
  })

  it('leaves a named key as written', () => {
    expect(formatShortcutKeys({ key: 'Backspace' })).toBe('Backspace')
  })

  it('prefixes modifier symbols in the order given', () => {
    expect(formatShortcutKeys({ key: 's', modifiers: ['meta', 'alt'] })).toBe('⌘⌥S')
    expect(formatShortcutKeys({ key: '[', modifiers: ['meta', 'shift'] })).toBe('⌘⇧[')
  })
})

describe('SHORTCUT_CATALOG', () => {
  it('has a unique id per entry', () => {
    const ids = SHORTCUT_CATALOG.map(s => s.id)
    expect(new Set(ids).size).toBe(ids.length)
  })
})

describe('buildShortcuts', () => {
  it('returns one shortcut per catalog entry, carrying its binding', () => {
    const bindings = allBindings()
    const built = buildShortcuts(bindings)

    expect(built).toHaveLength(SHORTCUT_CATALOG.length)
    built.forEach((shortcut, i) => {
      const meta = SHORTCUT_CATALOG[i]
      expect(shortcut.key).toBe(meta.key)
      expect(shortcut.modifiers).toEqual(meta.modifiers)
      expect(shortcut.category).toBe(meta.category)
      expect(shortcut.description).toBe(meta.description)
      expect(shortcut.action).toBe(bindings[meta.id].action)
    })
  })

  it('carries the binding’s enabled flag through', () => {
    const bindings = allBindings()
    bindings['copy-chat'] = { action: vi.fn(), enabled: false }
    const built = buildShortcuts(bindings)
    const copyChat = built[SHORTCUT_CATALOG.findIndex(s => s.id === 'copy-chat')]
    expect(copyChat.enabled).toBe(false)
  })

  it('fails when the catalog gains an id the bindings do not cover', () => {
    const bindings = allBindings()
    delete (bindings as Record<string, ShortcutBinding>)['next-session']
    expect(() => buildShortcuts(bindings)).toThrow(/next-session/)
  })

  it('fails when the bindings carry an id the catalog does not have', () => {
    const bindings = allBindings() as Record<string, ShortcutBinding>
    bindings['summon-a-pony'] = { action: vi.fn() }
    expect(() => buildShortcuts(bindings as Record<ShortcutId, ShortcutBinding>)).toThrow(/summon-a-pony/)
  })
})
