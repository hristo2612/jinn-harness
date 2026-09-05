import { afterEach, expect, it, vi } from "vitest"
import { cleanup, renderHook, waitFor } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import type { ReactNode } from "react"
import { useProvidedNavigation } from "../use-provided-navigation"

vi.mock("@/contrib/use-contributions", () => ({ useContributions: () => [] }))
const { moment } = vi.hoisted(() => ({ moment: vi.fn() }))
vi.mock("@/lib/api", () => ({ api: { moment } }))
afterEach(() => { cleanup(); vi.resetAllMocks() })

it("uses the daemon's arrangement for both navigation surfaces", async () => {
  moment.mockImplementation(async (_domain, _topic, p) => {
    const fold = (xs: {id:string; label:string; provided:boolean}[]) => xs.filter(x => x.provided).reverse().map(x => ({...x, label: x.id === "/settings/plugins" ? "My tools" : x.label}))
    return {...p, items: fold(p.items), mobileItems: fold(p.mobileItems)}
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = ({children}: {children: ReactNode}) => <QueryClientProvider client={client}>{children}</QueryClientProvider>
  const {result} = renderHook(() => useProvidedNavigation(false), {wrapper})
  await waitFor(() => expect(result.current.items.map(x => x.label)).toEqual(["My tools", "Settings"]))
  expect(result.current.mobileItems.map(x => x.label)).toEqual(["My tools", "Settings"])
})

it("rejects a late response that started before an administration", async () => {
  let finish!: (value: unknown) => void
  moment.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = ({children}: {children: ReactNode}) => <QueryClientProvider client={client}>{children}</QueryClientProvider>
  const {result} = renderHook(() => useProvidedNavigation(false), {wrapper})
  await waitFor(() => expect(moment).toHaveBeenCalledOnce())
  await client.cancelQueries({queryKey: ["navigation-moment"]})
  moment.mockImplementation(async (_domain, _topic, p) => p)
  await client.invalidateQueries({queryKey: ["navigation-moment"]})
  const base = navigationPayload(providedNavigationFor(false))
  finish({...base, items: base.items.map(x => ({...x, label: "Stale"}))})
  await waitFor(() => expect(result.current.difference).toBe("No navigation changes returned."))
  expect(result.current.items.some(x => x.label === "Stale")).toBe(false)
})

import { foldNavigation, navigationPayload } from "../navigation-extension"
import { providedNavigationFor } from "../nav-provided"

it("reconstructs authority, hrefs and icons from the offered items only", () => {
  const base = providedNavigationFor(false)
  const p = navigationPayload(base)
  const result = foldNavigation(base, {...p, future: 1, items: p.items.map(x => ({...x, provided: true, href: "https://evil.example", icon: "fake"}))})
  expect(result).toEqual(base)
})

it.each([
  null, {}, {items: [], mobileItems: []},
  {items: [{id: "/escape", label: "Escape"}], mobileItems: []},
])("falls back for malformed output %j", value => {
  expect(() => foldNavigation(providedNavigationFor(false), value)).toThrow()
})

it.each(["", " ", "x".repeat(41), "<b>Tools</b>", "hidden\u202ename"])("refuses invalid label %j", label => {
  const base = providedNavigationFor(false)
  const p = navigationPayload(base)
  expect(() => foldNavigation(base, {...p, items: p.items.map(x => ({...x, label}))})).toThrow()
})

it("refuses duplicate IDs and removing either recovery destination", () => {
  const base = providedNavigationFor(false)
  const p = navigationPayload(base)
  expect(() => foldNavigation(base, {...p, items: [...p.items, p.items[0]]})).toThrow()
  for (const id of ["/settings", "/settings/plugins"]) {
    expect(() => foldNavigation(base, {...p, mobileItems: p.mobileItems.filter(x => x.id !== id)})).toThrow()
  }
})

it("uses standard navigation with the actual refusal reason", async () => {
  moment.mockRejectedValue(new Error("restarting: the walk was refused whole"))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = ({children}: {children: ReactNode}) => <QueryClientProvider client={client}>{children}</QueryClientProvider>
  const {result} = renderHook(() => useProvidedNavigation(false), {wrapper})
  await waitFor(() => expect(result.current.notice).toContain("restarting"))
  expect(result.current.items).toEqual(providedNavigationFor(false).items)
})
