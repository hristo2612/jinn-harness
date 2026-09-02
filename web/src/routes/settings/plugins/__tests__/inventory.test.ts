import { createElement, type ReactNode } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const listPlugins = vi.fn()
vi.mock("@/lib/api", () => ({ api: { listPlugins: (...args: unknown[]) => listPlugins(...args) } }))
const authFetch = vi.fn()
vi.mock("@/lib/auth", () => ({ authFetch: (...args: unknown[]) => authFetch(...args) }))

const { usePluginInventory, useRescanPlugins, useRevealPlugin, useTogglePlugin } = await import("../inventory")

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)
}

beforeEach(() => {
  listPlugins.mockReset()
  authFetch.mockReset()
})

// UI-1 §4.2 item 10 (§8 amendment 6): the inventory's read is the daemon's
// `main` catalog through item 1's adapter, one function, the inventory's own
// row shape; the old gateway's writes have no `/v1` counterpart and refuse
// client-side, sending nothing.
describe("the plugins inventory on the daemon", () => {
  it("reads the main catalog through the /v1 adapter in the inventory's shape", async () => {
    listPlugins.mockResolvedValue({
      catalog: "main",
      "served-by": "jinn-plugins-live",
      entries: [
        { id: "jinn-cron", incarnation: 3, lifecycle: { state: "active" } },
        { id: "jinn-broken", lifecycle: { state: "failed", reason: "no such wasm" } },
        { id: "jinn-idle", incarnation: 1, lifecycle: { state: "inactive" } },
      ],
    })

    const { result } = renderHook(() => usePluginInventory(), { wrapper: wrapper() })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(listPlugins).toHaveBeenCalledWith("main")
    expect(result.current.data).toEqual([
      { id: "jinn-cron", name: "jinn-cron", version: "3", kind: "client+server", status: "loaded" },
      { id: "jinn-broken", name: "jinn-broken", version: "none", kind: "client+server", status: "error", error: "no such wasm" },
      { id: "jinn-idle", name: "jinn-idle", version: "1", kind: "client+server", status: "disabled" },
    ])
    expect(authFetch).not.toHaveBeenCalled()
  })

  it("refuses a write client-side and sends nothing (FINDINGS #37 / KG-1)", async () => {
    const { result } = renderHook(
      () => ({ toggle: useTogglePlugin(), reveal: useRevealPlugin(), rescan: useRescanPlugins() }),
      { wrapper: wrapper() },
    )

    await expect(result.current.toggle.mutateAsync({ id: "jinn-cron", enabled: false })).rejects.toThrow("config only")
    await expect(result.current.reveal.mutateAsync("jinn-cron")).rejects.toThrow("config only")
    await expect(result.current.rescan.mutateAsync()).rejects.toThrow("config only")

    expect(authFetch).not.toHaveBeenCalled()
    expect(listPlugins).not.toHaveBeenCalled()
  })
})
