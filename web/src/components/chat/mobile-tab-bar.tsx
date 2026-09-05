import { Link, useLocation } from "react-router-dom"
import { activeHref, isNavItemActive } from "@/components/pill-nav"
import { MORE_NAV_ITEM } from "@/lib/nav"
import { NOT_IN_PROFILE } from "@/lib/nav-provided"
import { useProvidedNavigation } from "@/lib/use-provided-navigation"
import { cn } from "@/lib/utils"
import { useFeatures } from "@/hooks/use-features"

// ---------------------------------------------------------------------------
// MobileTabBar — GRS-022. The SOLE mobile nav: an icon-only iOS-style bottom tab
// bar. Its tabs are the route table's (adaptation 15, `lib/nav-provided`): the
// primary destinations Chat · Todos · Workflows in their slots, provided or
// not, then the surfaces this bundle renders where More would sit — at the
// shipped table Settings and Plugins, live. There is no More tab, and nothing
// here redirects. An absent destination keeps its slot disabled and says why:
// the visible caption "not in this profile" under its glyph (the one text the
// bar carries), which is also the control's accessible description; the
// control stays focusable, so a finger and a keyboard both reach the reason.
// A `title` is never the reason on a touch surface (Taste §2).
// Mobile only (lg:hidden); the parent decides when to mount it. --material-thick
// over content with the single 0.5px top hairline iOS tab bars are allowed (the
// one sanctioned exception to "no hairlines at rest"). Frosted on pointer:fine;
// coarse pointers get the same material composited to alpha 1
// (--material-thick-opaque) instead, so the backdrop filter stops re-rasterising
// over the scrolling thread.
//
// Live tabs are icons-only (HIG icons-over-labels): the glyphs are
// self-explanatory. The "you are here" cue is the active tab's --accent tint
// (the sanctioned exception to the desktop rail's "never --accent" rule, scoped
// to THIS component only — the desktop NavRibbon is untouched). Every tab keeps
// an aria-label (no visible text ≠ no accessible name) and a ≥49px tap target so
// a label-free bar stays fully accessible and thumb-friendly.
//
// When a profile's router does render the More screen, its More tab stays lit
// while the operator is on any of its overflow children, so the bar always
// shows where you are even after a one-tap dive out of the overflow list.
// ---------------------------------------------------------------------------

function isTabActive(href: string, pathname: string, overflowHrefs: string[]): boolean {
  if (href === MORE_NAV_ITEM.href) {
    return (
      pathname === MORE_NAV_ITEM.href ||
      overflowHrefs.some((h) => isNavItemActive(h, pathname))
    )
  }
  return isNavItemActive(href, pathname)
}

export function MobileTabBar() {
  const pathname = useLocation().pathname
  const { data: features } = useFeatures()
  const navigation = useProvidedNavigation(features?.notesEnabled === true)
  const active = activeHref(navigation.mobileItems, pathname)

  return (
    <nav
      aria-label="Primary"
      className={cn(
        "fixed inset-x-0 bottom-0 z-40 lg:hidden",
        "flex items-stretch",
        // Named so a route change snapshots the bar on its own rather than
        // cross-fading the one surface that is on both sides of every tap.
        "[view-transition-name:jinn-tab-bar]",
        "border-t-[0.5px] border-[var(--separator)] bg-[var(--material-thick-opaque)]",
        "[@media(pointer:fine)]:bg-[var(--material-thick)]",
        "[@media(pointer:fine)]:[backdrop-filter:blur(20px)_saturate(1.3)]",
        "[@media(pointer:fine)]:[-webkit-backdrop-filter:blur(20px)_saturate(1.3)]",
        "py-1.5 pb-[max(var(--safe-bottom),6px)]",
      )}
    >
      {navigation.mobileItems.map((item) => {
        const isActive =
          item.href === MORE_NAV_ITEM.href ? isTabActive(item.href, pathname, navigation.overflowHrefs) : item.href === active
        const Icon = item.icon
        const tabClass = cn(
          // min-w-0 so flex-1 can shrink past the tap pill's own 3.5rem: at a
          // large browser text size four rem-wide pills outrun a phone
          // viewport and the last tab lands off-screen.
          "min-h-[49px] min-w-0 flex-1 flex items-center justify-center",
          "transition-colors",
          isActive
            ? "text-[var(--accent)]"
            : "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]",
        )
        const glyph = (
          <span className="flex h-9 w-14 max-w-full items-center justify-center rounded-full">
            <Icon size={25} className="shrink-0" />
          </span>
        )
        // An absent destination (adaptation 15): its slot kept, marked, inert,
        // its reason where a finger and a keyboard reach it.
        if (!item.provided) {
          const reasonId = `mobile-tab-reason-${item.label.toLowerCase().replace(/\s+/g, "-")}`
          return (
            <span
              key={item.href}
              role="link"
              tabIndex={0}
              aria-disabled="true"
              aria-label={item.label}
              aria-describedby={reasonId}
              title={NOT_IN_PROFILE}
              className={cn(tabClass, "flex-col cursor-not-allowed opacity-40")}
            >
              {glyph}
              <span id={reasonId} className="px-1 text-center text-[10px] leading-3 font-[var(--weight-medium)]">
                {NOT_IN_PROFILE}
              </span>
            </span>
          )
        }
        return (
          <Link
            key={item.href}
            to={item.href}
            viewTransition
            aria-label={item.label}
            aria-current={isActive ? "page" : undefined}
            onClick={() => {
              // HIG tab re-tap: tapping the already-active Chat tab scrolls the
              // chat list back to the top (the route is unchanged, so the Link is
              // otherwise a no-op). GRS-023.
              if (isActive && item.href === "/") {
                document
                  .querySelector<HTMLElement>("[data-chat-list-scroll]")
                  ?.scrollTo({ top: 0, behavior: "smooth" })
              }
            }}
            className={tabClass}
          >
            {glyph}
          </Link>
        )
      })}
    </nav>
  )
}
