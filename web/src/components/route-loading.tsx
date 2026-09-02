import { useRouteLoadingPresence } from '@/components/chat/chat-hydration'
import { MobileTabBar } from '@/components/chat/mobile-tab-bar'

/**
 * The route-level Suspense fallback. It MUST carry the mobile tab bar: routes
 * are code-split and each page mounts its own MobileTabBar, so on a cold tab
 * switch this fallback is the committed frame between two pages. Without the
 * bar here, that frame has no `jinn-tab-bar` view-transition element — the
 * bar's old snapshot plays an exit fade and the chat list shows through where
 * the bar was (the mobile "nav briefly disappears" bug). With it, the
 * view-transition pairs old and new bars and the nav holds still through the
 * chunk load. Desktop is unaffected (the bar is lg:hidden).
 */
export function RouteLoading({ label = 'Loading page' }: { label?: string }) {
  useRouteLoadingPresence()
  return (
    <div className="flex h-dvh items-center justify-center bg-background" role="status" aria-label={label}>
      <div className="size-5 animate-spin rounded-full border-2 border-[var(--fill-tertiary)] border-t-[var(--accent)]" />
      <MobileTabBar />
    </div>
  )
}
