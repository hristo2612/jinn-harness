import { act, cleanup, renderHook, waitFor } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import type { ReactNode } from "react"
import { afterEach, expect, it, vi } from "vitest"
import { useNavigationAdmin } from "../use-navigation-admin"
import type { NavigationSnapshot } from "../navigation-state"

const { read, remove } = vi.hoisted(() => ({read: vi.fn(), remove: vi.fn()}))
vi.mock("../navigation-state", async importOriginal => ({...await importOriginal<typeof import("../navigation-state")>(), readNavigationSnapshot: read}))
vi.mock("@/lib/profile-admin", () => ({profileAdmin: {removeEntry: remove}}))
afterEach(() => { cleanup(); vi.useRealTimers(); vi.resetAllMocks() })
const empty: NavigationSnapshot = {entries: [], catalog: [], witnessed: []}
const wrapper = ({children}: {children: ReactNode}) => <QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}>{children}</QueryClientProvider>

it("accepted removal with no positive runtime witness times out as unconfirmed", async () => {
  read.mockResolvedValue(empty)
  remove.mockResolvedValue({"administered-seq": 10})
  const {result} = renderHook(() => useNavigationAdmin(), {wrapper})
  await waitFor(() => expect(result.current.snapshot.isSuccess).toBe(true))
  vi.useFakeTimers()
  let pending!: Promise<void>
  await act(async () => { pending = result.current.act("remove") })
  expect(result.current.message).toContain("Accepted (record 10); waiting")
  expect(result.current.busy).toBe(true)
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000); await pending })
  expect(result.current.message).toContain("runtime unconfirmed")
  expect(result.current.message).not.toContain("witnessed")
  expect(result.current.busy).toBe(false)
})

it("confirms removal only after a disposal after the accepted record", async () => {
  read.mockResolvedValue(empty)
  remove.mockResolvedValue({"administered-seq": 10})
  const {result} = renderHook(() => useNavigationAdmin(), {wrapper})
  await waitFor(() => expect(result.current.snapshot.isSuccess).toBe(true))
  vi.useFakeTimers()
  let pending!: Promise<void>
  await act(async () => { pending = result.current.act("remove") })
  read.mockResolvedValue({...empty, witnessed:[{ordinal: 1, "committed-by": 11, to:"disposed"}]})
  await act(async () => { await vi.advanceTimersByTimeAsync(500); await pending })
  expect(result.current.message).toBe("Removal witnessed after record 10.")
})
