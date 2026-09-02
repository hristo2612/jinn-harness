import type { ShortcutDef } from '@/hooks/use-keyboard-shortcuts'

/**
 * Stable identity for every keyboard shortcut the chat page offers. The chat
 * page binds an action to each id and the Settings reference lists them, so a
 * shortcut can never exist in one place and be missing from the other.
 */
export type ShortcutId =
  | 'new-chat'
  | 'next-session'
  | 'prev-session'
  | 'next-employee'
  | 'delete-session'
  | 'delete-session-forward'
  | 'copy-chat'
  | 'close-overlay'
  | 'focus-chat'
  | 'keyboard-shortcuts'
  | 'close-tab'
  | 'prev-tab'
  | 'next-tab'
  | 'toggle-chat-list'
  | 'toggle-chat-list-alias'
  | `tab-${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9}`

export interface ShortcutMeta {
  id: ShortcutId
  key: string
  modifiers?: ShortcutDef['modifiers']
  category: ShortcutDef['category']
  description: string
}

/** What the chat page supplies for a catalogued shortcut: the behaviour. */
export interface ShortcutBinding {
  action: () => void
  enabled?: boolean
}

/** The order categories are presented in, shared by the overlay and Settings. */
export const SHORTCUT_CATEGORY_ORDER: ShortcutDef['category'][] = ['Navigation', 'Actions', 'Help']

const MODIFIER_SYMBOLS: Record<string, string> = {
  meta: '⌘',
  shift: '⇧',
  alt: '⌥',
}

/** The badge label for a shortcut, e.g. `⌘⌥S`. */
export function formatShortcutKeys(shortcut: Pick<ShortcutMeta, 'key' | 'modifiers'>): string {
  const parts = (shortcut.modifiers ?? []).map(mod => MODIFIER_SYMBOLS[mod] ?? mod)
  parts.push(shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key)
  return parts.join('')
}

const TAB_NUMBERS = [1, 2, 3, 4, 5, 6, 7, 8, 9] as const

export const SHORTCUT_CATALOG: readonly ShortcutMeta[] = [
  { id: 'new-chat', key: 'n', category: 'Actions', description: 'New chat' },
  { id: 'next-session', key: 'j', category: 'Navigation', description: 'Next session' },
  { id: 'prev-session', key: 'k', category: 'Navigation', description: 'Previous session' },
  { id: 'next-employee', key: 'e', category: 'Navigation', description: 'Next employee' },
  { id: 'delete-session', key: 'Backspace', category: 'Actions', description: 'Delete session' },
  { id: 'delete-session-forward', key: 'Delete', category: 'Actions', description: 'Delete session' },
  { id: 'copy-chat', key: 'c', category: 'Actions', description: 'Copy chat' },
  { id: 'close-overlay', key: 'Escape', category: 'Navigation', description: 'Close overlay' },
  { id: 'focus-chat', key: '/', category: 'Actions', description: 'Focus chat' },
  { id: 'keyboard-shortcuts', key: '?', category: 'Help', description: 'Keyboard shortcuts' },
  { id: 'close-tab', key: 'w', modifiers: ['meta'], category: 'Actions', description: 'Close tab' },
  { id: 'prev-tab', key: '[', modifiers: ['meta', 'shift'], category: 'Navigation', description: 'Previous tab' },
  { id: 'next-tab', key: ']', modifiers: ['meta', 'shift'], category: 'Navigation', description: 'Next tab' },
  // ⌥⌘S is the macOS-native sidebar toggle; ⌘\ is the web-friendly alias
  // (Linear/VS Code class).
  { id: 'toggle-chat-list', key: 's', modifiers: ['meta', 'alt'], category: 'Navigation', description: 'Toggle chat list' },
  { id: 'toggle-chat-list-alias', key: '\\', modifiers: ['meta'], category: 'Navigation', description: 'Toggle chat list' },
  ...TAB_NUMBERS.map((n): ShortcutMeta => ({
    id: `tab-${n}`,
    key: String(n),
    modifiers: ['meta', 'alt'],
    category: 'Navigation',
    description: `Tab ${n}`,
  })),
]

/**
 * Pair every catalogued shortcut with its behaviour. The two sides must cover
 * exactly the same ids: a shortcut listed in Settings that does nothing, or a
 * bound action nobody can discover, is a bug worth failing loudly for.
 */
export function buildShortcuts(bindings: Record<ShortcutId, ShortcutBinding>): ShortcutDef[] {
  const catalogued = new Set<string>(SHORTCUT_CATALOG.map(meta => meta.id))
  const unknown = Object.keys(bindings).filter(id => !catalogued.has(id))
  if (unknown.length > 0) {
    throw new Error(`Shortcut bindings reference ids missing from SHORTCUT_CATALOG: ${unknown.join(', ')}`)
  }

  return SHORTCUT_CATALOG.map(meta => {
    const binding = bindings[meta.id]
    if (!binding) throw new Error(`Shortcut "${meta.id}" is catalogued but has no bound action`)
    return {
      key: meta.key,
      modifiers: meta.modifiers,
      category: meta.category,
      description: meta.description,
      action: binding.action,
      enabled: binding.enabled,
    }
  })
}
