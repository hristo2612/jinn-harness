import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

/**
 * A small status label.
 *
 * The tinted variants are the app's existing chip recipe — the colour at 12%
 * for the fill and at full strength for the text — so a plugin's status chip
 * and the app's own read as one language. No border at rest: the fill is the
 * separation.
 */
const badgeVariants = cva(
  "inline-flex w-fit shrink-0 items-center justify-center gap-[var(--space-1)] whitespace-nowrap rounded-[var(--radius-sm)] px-[var(--space-2)] py-[2px] text-[length:var(--text-caption1)] font-[var(--weight-medium)] [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3",
  {
    variants: {
      variant: {
        default: "bg-[var(--fill-primary)] text-[var(--text-primary)]",
        secondary: "bg-[var(--fill-tertiary)] text-[var(--text-secondary)]",
        success:
          "bg-[color-mix(in_srgb,var(--system-green)_12%,transparent)] text-[var(--system-green)]",
        warning:
          "bg-[color-mix(in_srgb,var(--system-orange)_12%,transparent)] text-[var(--system-orange)]",
        destructive:
          "bg-[color-mix(in_srgb,var(--system-red)_12%,transparent)] text-[var(--system-red)]",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

function Badge({
  className,
  variant = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"span"> &
  VariantProps<typeof badgeVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "span"

  return (
    <Comp
      data-slot="badge"
      data-variant={variant}
      className={cn(badgeVariants({ variant, className }))}
      {...props}
    />
  )
}

export { Badge, badgeVariants }
