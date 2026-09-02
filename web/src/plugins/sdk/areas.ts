/**
 * The v1 contribution areas — the places a plugin may add something.
 *
 * The *values* are the contract: they appear in a plugin's manifest and in the
 * registry's keys, so renaming one breaks every plugin targeting it. The
 * property names are only an ergonomic alias for in-repo callers. This is the
 * single declaration of them; core surfaces and plugins address areas through
 * the same vocabulary or they are not addressing the same registry.
 */
export const AREAS = Object.freeze({
  routes: 'routes',
  sidebarNav: 'sidebar.nav',
  statusBarRight: 'statusbar.right',
  todoDetailActions: 'todo.detail.actions',
  todoDetailSections: 'todo.detail.sections',
  chatComposer: 'chat.composer',
  homeWidgets: 'home.widgets',
} as const)

/** Every area id a contribution may target. */
export type AreaId = (typeof AREAS)[keyof typeof AREAS]
