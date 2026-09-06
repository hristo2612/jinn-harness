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

it("releases controls after a stalled runtime read and excludes its late result", async () => {
  read.mockResolvedValue(empty)
  remove.mockResolvedValue({"administered-seq": 10})
  const {result} = renderHook(() => useNavigationAdmin(), {wrapper})
  await waitFor(() => expect(result.current.snapshot.isSuccess).toBe(true))
  vi.useFakeTimers()
  const late: ((value: NavigationSnapshot) => void)[] = []
  let pending!: Promise<void>
  await act(async () => { pending = result.current.act("remove") })
  read.mockImplementation(() => new Promise(resolve => { late.push(resolve) }))
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000) })
  expect(result.current.busy).toBe(false)
  expect(result.current.message).toContain("runtime unconfirmed")
  await act(async () => { late[0]({...empty, witnessed:[{ordinal:1,"committed-by":11,to:"disposed"}]}); await pending })
  expect(result.current.message).not.toContain("witnessed")
  expect(result.current.snapshot.data?.witnessed).toEqual([])
})

it("does not wait for a stalled refresh after a confirmed removal", async () => {
  read.mockResolvedValue(empty)
  remove.mockResolvedValue({"administered-seq": 10})
  const client = new QueryClient({defaultOptions:{queries:{retry:false}}})
  const {result} = renderHook(() => useNavigationAdmin(), {wrapper: ({children}) => <QueryClientProvider client={client}>{children}</QueryClientProvider>})
  await waitFor(() => expect(result.current.snapshot.isSuccess).toBe(true))
  vi.spyOn(client, "invalidateQueries").mockImplementation(() => new Promise(() => {}))
  vi.useFakeTimers()
  await act(async () => { void result.current.act("remove") })
  read.mockResolvedValue({...empty, witnessed:[{ordinal:1,"committed-by":11,to:"disposed"}]})
  await act(async () => { await vi.advanceTimersByTimeAsync(500) })
  expect(result.current.busy).toBe(false)
  expect(result.current.message).toContain("Removal witnessed")
})

it("reports a stalled write as uncertain instead of rejected", async () => {
  read.mockResolvedValue(empty)
  remove.mockImplementation(() => new Promise(() => {}))
  const {result} = renderHook(() => useNavigationAdmin(), {wrapper})
  await waitFor(() => expect(result.current.snapshot.isSuccess).toBe(true))
  vi.useFakeTimers()
  await act(async () => { void result.current.act("remove") })
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000) })
  expect(result.current.busy).toBe(false)
  expect(result.current.message).toContain("acceptance and runtime unconfirmed")
})
