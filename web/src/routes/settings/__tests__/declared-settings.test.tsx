import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { DeclaredNamespace } from "@/lib/api-config"
import { DeclaredSettings } from "../declared-settings"

/* The settings page renders only what the seam's namespace schema declares
 * (UI-1 §4.2 item 1, §8 amendment 4). The `cron` namespace below is the one the
 * ui profile declares; `ui` adds the one kind it lacks, a bool. */

const declared: Record<string, DeclaredNamespace> = {
  cron: {
    additional: false,
    properties: {
      jobs: { kind: "array", required: true },
      "tick-ms": { kind: "integer", required: false },
      "notify-token": { kind: "secret-ref", required: false },
      "entry-id": { kind: "string", required: false },
    },
  },
  ui: { additional: false, properties: { "dark-mode": { kind: "bool", required: false } } },
}

const config = {
  cron: { jobs: [], "tick-ms": 100, "notify-token": { $secret: "cron-notify" }, "entry-id": "cron" },
  ui: { "dark-mode": true },
}

function renderDeclared() {
  const onChange = vi.fn()
  render(<DeclaredSettings config={config} declared={declared} onChange={onChange} />)
  return onChange
}

describe("DeclaredSettings", () => {
  it("renders one section per namespace and marks required fields", () => {
    renderDeclared()
    expect(screen.getByText("cron")).toBeTruthy()
    expect(screen.getByText("ui")).toBeTruthy()
    expect(screen.getByText("jobs (required)")).toBeTruthy()
    expect(screen.getByText("tick-ms")).toBeTruthy()
  })

  it("commits an integer edit as a number on the namespace path", () => {
    const onChange = renderDeclared()
    fireEvent.change(screen.getByLabelText("tick-ms"), { target: { value: "250" } })
    expect(onChange).toHaveBeenCalledWith(["cron", "tick-ms"], 250)
  })

  it("does not commit an integer that is negative or fractional", () => {
    const onChange = renderDeclared()
    fireEvent.change(screen.getByLabelText("tick-ms"), { target: { value: "-1" } })
    fireEvent.change(screen.getByLabelText("tick-ms"), { target: { value: "2.5" } })
    expect(onChange).not.toHaveBeenCalled()
  })

  it("shows a secret reference by name and renders no input for it", () => {
    renderDeclared()
    expect(screen.getByText("secret reference: cron-notify")).toBeTruthy()
    expect(screen.queryByLabelText("notify-token")).toBeNull()
    for (const control of document.querySelectorAll("input, textarea")) {
      expect((control as HTMLInputElement).value).not.toContain("cron-notify")
    }
  })

  it("renders a bool as a toggle that commits the flipped value", () => {
    const onChange = renderDeclared()
    const toggle = screen.getByRole("switch", { name: "dark-mode" })
    expect(toggle.getAttribute("aria-checked")).toBe("true")
    fireEvent.click(toggle)
    expect(onChange).toHaveBeenCalledWith(["ui", "dark-mode"], false)
  })

  it("commits JSON on blur only when it parses to the declared shape", () => {
    const onChange = renderDeclared()
    const jobs = screen.getByLabelText("jobs")
    fireEvent.change(jobs, { target: { value: "[" } })
    fireEvent.blur(jobs)
    expect(screen.getByText(/Not valid JSON/)).toBeTruthy()
    fireEvent.change(jobs, { target: { value: "{}" } })
    fireEvent.blur(jobs)
    expect(screen.getByText(/Not an array/)).toBeTruthy()
    expect(onChange).not.toHaveBeenCalled()
    fireEvent.change(jobs, { target: { value: '[{"id":"nightly"}]' } })
    fireEvent.blur(jobs)
    expect(onChange).toHaveBeenCalledWith(["cron", "jobs"], [{ id: "nightly" }])
  })
})
