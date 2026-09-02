import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { NativeGatewayProfilesSnapshot } from "@/lib/native-gateway-profiles"
import { NativePairingScreen } from "./native-pairing-screen"

const pairAndInstall = vi.fn()
const select = vi.fn()
const remove = vi.fn()
const retry = vi.fn()
let snapshot: NativeGatewayProfilesSnapshot | undefined

vi.mock("@/lib/native-gateway-bootstrap", () => ({
  pairAndInstallNativeGateway: (...args: unknown[]) => pairAndInstall(...args),
  nativeGatewayProfiles: () => (snapshot
    ? { subscribe: () => () => {}, snapshot: () => snapshot, select, remove, retry }
    : undefined),
}))

const alpha = { id: "alpha", origin: "http://127.0.0.1:7799", name: "Alpha", deviceId: "device:alpha" }
const beta = { id: "beta", origin: "http://127.0.0.1:7800", name: "Beta", deviceId: "device:beta" }

describe("NativePairingScreen", () => {
  beforeEach(() => {
    snapshot = undefined
    for (const spy of [pairAndInstall, select, remove, retry]) spy.mockReset()
  })

  it("pairs an explicit gateway origin and code", async () => {
    pairAndInstall.mockResolvedValue("http://127.0.0.1:7779")
    const onPaired = vi.fn()
    render(<NativePairingScreen onPaired={onPaired} />)
    fireEvent.change(screen.getByLabelText("Pair code"), { target: { value: "ABCD-EFGH" } })
    fireEvent.click(screen.getByRole("button", { name: "Pair gateway" }))
    await waitFor(() => expect(onPaired).toHaveBeenCalledWith("http://127.0.0.1:7779"))
    expect(pairAndInstall).toHaveBeenCalledWith("http://127.0.0.1:7779", "ABCD-EFGH")
  })

  it("offers the other paired gateways when the remembered one stopped answering", async () => {
    snapshot = {
      profiles: [alpha, beta],
      activeId: alpha.id,
      generation: 1,
      status: "unreachable",
      failedProfileId: alpha.id,
      error: "connection refused",
      activeReachable: false,
    }
    select.mockResolvedValue(undefined)
    render(<NativePairingScreen />)

    expect(screen.getByRole("heading", { name: "Cannot reach Alpha" })).toBeTruthy()
    expect(screen.getByRole("alert").textContent).toContain("connection refused")
    expect(screen.getByLabelText("Gateway origin")).toBeTruthy()

    fireEvent.click(screen.getByRole("button", { name: "Use" }))
    await waitFor(() => expect(select).toHaveBeenCalledWith(beta.id))

    fireEvent.click(screen.getByRole("button", { name: "Remove Beta" }))
    await waitFor(() => expect(remove).toHaveBeenCalledWith(beta.id))
  })

  it("retries the active gateway rather than switching away from it", async () => {
    snapshot = {
      profiles: [alpha],
      activeId: alpha.id,
      generation: 1,
      status: "unreachable",
      failedProfileId: alpha.id,
      activeReachable: false,
    }
    retry.mockResolvedValue(undefined)
    render(<NativePairingScreen />)

    fireEvent.click(screen.getByRole("button", { name: "Retry" }))
    await waitFor(() => expect(retry).toHaveBeenCalled())
    expect(select).not.toHaveBeenCalled()
  })

  it("keeps naming the unreachable active gateway after a switch to another one fails", () => {
    snapshot = {
      profiles: [alpha, beta],
      activeId: alpha.id,
      generation: 1,
      status: "unreachable",
      // The later failed switch overwrote the record of alpha's own failure.
      failedProfileId: beta.id,
      error: "connection refused",
      activeReachable: false,
    }
    render(<NativePairingScreen />)

    expect(screen.getByRole("heading", { name: "Cannot reach Alpha" })).toBeTruthy()
  })

  it("waits on the connecting state while the remembered gateway is being checked", () => {
    snapshot = { profiles: [alpha], activeId: alpha.id, generation: 0, status: "checking", activeReachable: false }
    render(<NativePairingScreen />)
    expect(screen.getByText("Connecting to Alpha...")).toBeTruthy()
    expect(screen.queryByLabelText("Pair code")).toBeNull()
  })
})
