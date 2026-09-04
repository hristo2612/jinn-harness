import { createElement, type ReactNode } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const listPlugins = vi.fn()
vi.mock("@/lib/api", () => ({ api: { listPlugins: (...args: unknown[]) => listPlugins(...args) } }))
const authFetch = vi.fn()
vi.mock("@/lib/auth", () => ({ authFetch: (...args: unknown[]) => authFetch(...args) }))
const setDisabled = vi.fn()
vi.mock("@/lib/profile-admin", () => ({
  profileAdmin: { setDisabled: (...args: unknown[]) => setDisabled(...args) },
}))

const { usePluginInventory, useRescanPlugins, useRevealPlugin, useTogglePlugin } = await import("../inventory")

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)
}

beforeEach(() => {
  listPlugins.mockReset()
  authFetch.mockReset()
  setDisabled.mockReset()
})

// UI-1 §4.2 item 10 (§8 amendment 6): the inventory's read is the daemon's
// `main` catalog through item 1's adapter, one function, the inventory's own
// row shape. Pin-bump 10 (jinnd M2-K23, FINDINGS #37 closed): the toggle is
// ONE `jinn:profile-admin` write through `PATCH /v1/profile/entries/{id}
// { disabled }`; reveal and rescan still have no counterpart — a catalog
// entry is not a folder — and refuse client-side, sending nothing.
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

  it("toggles a plugin as one jinn:profile-admin write: PATCH { disabled } (pin f8b285b, #37 closed)", async () => {
    setDisabled.mockResolvedValue({ "api-version": "0.3.0", id: "jinn-cron", write: "set-disabled", "administered-seq": 7 })
    const { result } = renderHook(() => useTogglePlugin(), { wrapper: wrapper() })

    await expect(result.current.mutateAsync({ id: "jinn-cron", enabled: false })).resolves.toMatchObject({
      write: "set-disabled",
      "administered-seq": 7,
    })
    expect(setDisabled).toHaveBeenCalledWith("jinn-cron", true)

    await expect(result.current.mutateAsync({ id: "jinn-cron", enabled: true })).resolves.toMatchObject({ write: "set-disabled" })
    expect(setDisabled).toHaveBeenLastCalledWith("jinn-cron", false)
  })

  it("surfaces the kernel's typed refusal from the toggle, unchanged", async () => {
    setDisabled.mockRejectedValue(new Error("set-disabled refused (conflict): an operation is in flight on the entry"))
    const { result } = renderHook(() => useTogglePlugin(), { wrapper: wrapper() })

    await expect(result.current.mutateAsync({ id: "jinn-cron", enabled: false })).rejects.toThrow("conflict")
  })

  it("reveal and rescan still have no counterpart: a catalog entry is not a folder", async () => {
    const { result } = renderHook(() => ({ reveal: useRevealPlugin(), rescan: useRescanPlugins() }), { wrapper: wrapper() })

    await expect(result.current.reveal.mutateAsync("jinn-cron")).rejects.toThrow("not a folder")
    await expect(result.current.rescan.mutateAsync()).rejects.toThrow("not a folder")

    expect(authFetch).not.toHaveBeenCalled()
    expect(setDisabled).not.toHaveBeenCalled()
    expect(listPlugins).not.toHaveBeenCalled()
  })
})
