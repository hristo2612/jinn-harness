import * as React from "react"

import { cn } from "@/lib/utils"

/**
 * A single-line text field.
 *
 * Separation is a fill rather than a hairline: at rest the field is a tinted
 * well on its surface, and focus deepens the fill and adds the accent ring. 34px
 * is the floor so the field is a comfortable target on a phone.
 */
function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "flex h-[34px] w-full rounded-[var(--radius-md)] bg-[var(--fill-quaternary)] px-[var(--space-3)] text-[length:var(--text-footnote)] text-[var(--text-primary)] outline-none transition-[background-color,box-shadow] duration-150 ease-[var(--ease-smooth)] placeholder:text-[var(--text-tertiary)] hover:bg-[var(--fill-tertiary)] focus-visible:bg-[var(--fill-tertiary)] focus-visible:ring-[3px] focus-visible:ring-[var(--accent-fill)] disabled:cursor-not-allowed disabled:opacity-50",
        className
      )}
      {...props}
    />
  )
}

export { Input }
