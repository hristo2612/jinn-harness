import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { emojiForName } from "@/lib/emoji-pool"
import type { JinnSettings } from "@/lib/settings"
import { DEFAULTS } from "@/lib/settings"

const settings = { ...DEFAULTS } as JinnSettings

vi.mock("@/routes/settings-provider", () => ({
  useSettings: () => ({ settings }),
}))

import { EmployeeAvatar } from "../employee-avatar"

function renderWith(overrides: Partial<JinnSettings>, name: string) {
  Object.assign(settings, DEFAULTS, overrides)
  render(<EmployeeAvatar name={name} />)
}

describe("EmployeeAvatar", () => {
  it("renders the operator's chosen emoji", () => {
    renderWith({ operatorEmoji: "🦊" }, "operator")

    expect(screen.getByText("🦊")).toBeTruthy()
  })

  it("falls back to the hashed emoji when the operator has chosen none", () => {
    renderWith({ operatorEmoji: null }, "operator")

    expect(screen.getByText(emojiForName("operator"))).toBeTruthy()
  })

  it("leaves an employee avatar alone when the operator has chosen an emoji", () => {
    renderWith({ operatorEmoji: "🦊" }, "jinn-dev")

    expect(screen.getByText(emojiForName("jinn-dev"))).toBeTruthy()
  })
})
