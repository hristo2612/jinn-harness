import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, renderHook, waitFor } from "@testing-library/react"
import { createElement, type ReactNode } from "react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({ searchGlobal: vi.fn() }))

vi.mock("@/lib/search-api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/search-api")>()
  return { ...actual, searchGlobal: mocks.searchGlobal }
})

import { useGlobalSearch } from "../use-global-search"
import { searchResponse } from "./fixtures"

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return createElement(QueryClientProvider, { client }, children)
}

function renderSearch(query: string) {
  return renderHook(({ q }: { q: string }) => useGlobalSearch({ query: q, literal: false }), {
    initialProps: { q: query },
    wrapper,
  })
}

describe("useGlobalSearch", () => {
  beforeEach(() => {
    mocks.searchGlobal.mockReset()
    mocks.searchGlobal.mockResolvedValue(searchResponse())
    vi.useFakeTimers({ shouldAdvanceTime: true })
  })
  afterEach(() => { vi.useRealTimers() })

  it("coalesces a burst of keystrokes into one request for the settled query", async () => {
    const { rerender } = renderSearch("")
    for (const q of ["b", "bl", "blo", "blocked"]) rerender({ q })

    await act(async () => { await vi.advanceTimersByTimeAsync(300) })

    await waitFor(() => expect(mocks.searchGlobal).toHaveBeenCalledTimes(1))
    expect(mocks.searchGlobal.mock.calls[0][0]).toMatchObject({ q: "blocked", literal: false })
  })

  it("waits the full debounce before asking at all", async () => {
    const { rerender } = renderSearch("")
    rerender({ q: "blocked" })

    await act(async () => { await vi.advanceTimersByTimeAsync(200) })
    expect(mocks.searchGlobal).not.toHaveBeenCalled()

    await act(async () => { await vi.advanceTimersByTimeAsync(100) })
    await waitFor(() => expect(mocks.searchGlobal).toHaveBeenCalledTimes(1))
  })

  it("asks for nothing while the query is empty or only whitespace", async () => {
    const { rerender } = renderSearch("")
    rerender({ q: "   " })

    await act(async () => { await vi.advanceTimersByTimeAsync(500) })

    expect(mocks.searchGlobal).not.toHaveBeenCalled()
  })

  it("carries the scope and the literal override into the request", async () => {
    renderHook(() => useGlobalSearch({ query: "blocked", scope: "todo", literal: true }), { wrapper })

    await act(async () => { await vi.advanceTimersByTimeAsync(300) })

    await waitFor(() => expect(mocks.searchGlobal).toHaveBeenCalledTimes(1))
    expect(mocks.searchGlobal.mock.calls[0][0]).toMatchObject({ q: "blocked", scope: "todo", literal: true })
  })
})
