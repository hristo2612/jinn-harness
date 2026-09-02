import { cn } from "@/lib/utils"
import { ORB_VARIANTS, SILENT_ENERGY, type OrbState, type OrbVariant } from "./orb-motion"
import { OrbCanvas } from "./orb-canvas"

const LABELS: Record<OrbVariant, string> = {
  mist: "Mist",
  coin: "Coin",
  ring: "Ring",
  pulse: "Pulse",
}

const QUIET_ENERGY = { current: SILENT_ENERGY }

interface OrbVariantPickerProps {
  value: OrbVariant
  onChange: (variant: OrbVariant) => void
  state?: OrbState
  className?: string
}

/** A compact radio gallery shared by Voice settings and the public orb bench. */
export function OrbVariantPicker({
  value,
  onChange,
  state = "idle",
  className,
}: OrbVariantPickerProps) {
  return (
    <div
      role="radiogroup"
      aria-label="Talk orb style"
      className={cn("grid grid-cols-2 gap-[var(--space-2)] sm:grid-cols-4", className)}
    >
      {ORB_VARIANTS.map((variant) => {
        const selected = variant === value
        return (
          <button
            key={variant}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={`${LABELS[variant]} orb`}
            data-orb-variant-option={variant}
            onClick={() => onChange(variant)}
            className={cn(
              "min-h-[104px] cursor-pointer rounded-[var(--radius-lg)] border-none px-[var(--space-2)] py-[var(--space-3)]",
              "flex flex-col items-center justify-center gap-[var(--space-2)]",
              "text-[length:var(--text-caption1)] text-[var(--text-secondary)]",
              "outline-none transition-[scale,background-color,box-shadow] duration-150 ease-out active:scale-[0.96]",
              "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]",
              selected
                ? "bg-[var(--accent-fill)] shadow-[0_0_0_1px_var(--talk-focus-ring)]"
                : "bg-[var(--fill-quaternary)] shadow-[var(--shadow-subtle)] hover:bg-[var(--fill-tertiary)]",
            )}
          >
            <OrbCanvas variant={variant} state={state} energyRef={QUIET_ENERGY} size={64} motion="still" />
            <span>{LABELS[variant]}</span>
          </button>
        )
      })}
    </div>
  )
}
