import { createContext, useContext, useMemo, type ReactNode } from "react"
import { cn } from "@/lib/utils"

export const PRIMARY_ACTION_SLOT = "primary-action"

/** FAB clearance above the tab bar. Both the offset and the scaffold pad consume this. */
export const FAB_BOTTOM_WITH_TAB =
  "calc(var(--tab-bar-height)+max(var(--safe-bottom),6px)+var(--space-4))"
export const FAB_BOTTOM_WITHOUT_TAB = "calc(max(var(--safe-bottom),6px)+var(--space-4))"

type Placement = "fab" | "trailing"

const PlacementContext = createContext<{ placement: Placement; hideMobileTabBar: boolean }>({
  placement: "fab",
  hideMobileTabBar: false,
})

export function PrimaryActionPlacementProvider({
  placement,
  hideMobileTabBar = false,
  children,
}: {
  placement: Placement
  hideMobileTabBar?: boolean
  children: ReactNode
}) {
  const value = useMemo(
    () => ({ placement, hideMobileTabBar }),
    [placement, hideMobileTabBar],
  )
  return <PlacementContext.Provider value={value}>{children}</PlacementContext.Provider>
}

type ActionProps = {
  "aria-label": string
  label: string
  icon?: ReactNode
  onClick: () => void
  disabled?: boolean
  testId?: string
}

function TrailingAction({ "aria-label": ariaLabel, label, icon, onClick, disabled, testId }: ActionProps) {
  return (
    <button
      type="button"
      data-slot={PRIMARY_ACTION_SLOT}
      data-primary-action="trailing"
      data-testid={testId}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "hidden h-9 shrink-0 items-center gap-1.5 rounded-full bg-[var(--fill-secondary)] px-3.5",
        "text-[length:var(--text-subheadline)] font-[var(--weight-medium)] text-[var(--text-primary)]",
        "lg:inline-flex",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]",
        "disabled:text-[var(--text-quaternary)]",
      )}
    >
      {icon}
      {label}
    </button>
  )
}

function FabAction({ "aria-label": ariaLabel, icon, onClick, disabled, testId, hideMobileTabBar }: ActionProps & { hideMobileTabBar: boolean }) {
  return (
    <button
      type="button"
      data-slot={PRIMARY_ACTION_SLOT}
      data-primary-action="fab"
      data-testid={testId ? `${testId}-fab` : undefined}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "absolute right-[var(--space-4)] z-30 size-14 rounded-full lg:hidden",
        hideMobileTabBar
          ? "bottom-[calc(max(var(--safe-bottom),6px)+var(--space-4))]"
          : "bottom-[calc(var(--tab-bar-height)+max(var(--safe-bottom),6px)+var(--space-4))]",
        "shadow-[var(--shadow-key)]",
        "transition-transform duration-[var(--duration-fast)] ease-[var(--ease-snappy)]",
        "active:scale-[0.94]",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]",
        "[&_svg]:size-6",
        disabled
          ? "bg-[var(--fill-tertiary)] text-[var(--text-quaternary)] shadow-none"
          : "bg-[var(--accent)] text-[var(--accent-contrast)]",
      )}
    >
      {icon}
    </button>
  )
}

export function PrimaryAction(props: ActionProps) {
  const { placement, hideMobileTabBar } = useContext(PlacementContext)
  if (placement === "trailing") return <TrailingAction {...props} />
  return <FabAction {...props} hideMobileTabBar={hideMobileTabBar} />
}
