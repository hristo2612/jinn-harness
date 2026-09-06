import { beforeEach, describe, expect, it, vi } from "vitest"

const authFetch = vi.fn()
vi.mock("@/lib/auth", () => ({ authFetch: (...args: unknown[]) => authFetch(...args) }))

const { createConfigApi, PATCH_SETTINGS_MOMENT } = await import("../api-config")

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } })
}

/** A daemon with one namespace, `cron`, declaring `tick-ms`. */
function daemon() {
  authFetch.mockImplementation(async (path: string, init?: RequestInit) => {
    if (path === "/v1/settings") return json(200, { namespaces: { cron: {} } })
    if (path === "/v1/settings/cron" && (!init || !init.method)) {
      return json(200, {
        namespace: "cron",
        settings: { "tick-ms": 500 },
        schema: { properties: { "tick-ms": { kind: "integer", required: false } }, additional: false },
      })
    }
    if (path === "/v1/settings/cron" && init?.method === "PATCH") {
      const { patch } = JSON.parse(String(init.body)) as { patch: Record<string, unknown> }
      return json(200, { namespace: "cron", settings: { "tick-ms": 500, ...patch } })
    }
    throw new Error(`unexpected request ${init?.method ?? "GET"} ${path}`)
  })
}

const conflict = vi.fn((status: number, message: string, remedy?: string) =>
  Object.assign(new Error(message), { status, remedy, code: "CONFIG_CONFLICT" }),
)
const responseError = vi.fn(async (res: Response) => new Error(`http ${res.status}`))

beforeEach(() => {
  authFetch.mockReset()
  conflict.mockClear()
  responseError.mockClear()
})

// UI-2 §9.2 item 13: a save is a moment FIRST, and the patch that leaves the
// page is the one the daemon's extensions folded.
describe("saving settings through the before-patch-settings moment", () => {
  it("dispatches the moment before the patch and sends the folded patch", async () => {
    daemon()
    const calls: string[] = []
    const moment = vi.fn(async (domain: string, topic: string, payload: unknown) => {
      calls.push(`moment ${domain}/${topic}`)
      const { namespace, patch } = payload as { namespace: string; patch: Record<string, unknown> }
      return json(200, { namespace, patch: { ...patch, "tick-ms": 900 } })
    })
    authFetch.mockImplementation(((fetch) => async (path: string, init?: RequestInit) => {
      if (init?.method === "PATCH") calls.push(`patch ${path}`)
      return fetch(path, init)
    })(authFetch.getMockImplementation()!))

    const api = createConfigApi({ responseError, conflict, moment })
    const document = await api.getConfig()
    await api.updateConfig({ cron: { "tick-ms": 700 } }, document.revision)

    expect(moment).toHaveBeenCalledWith(PATCH_SETTINGS_MOMENT.domain, PATCH_SETTINGS_MOMENT.topic, {
      namespace: "cron",
      patch: { "tick-ms": 700 },
    })
    expect(calls).toEqual(["moment ui/before-patch-settings", "patch /v1/settings/cron"])
    const patchCall = authFetch.mock.calls.find(([, init]) => (init as RequestInit | undefined)?.method === "PATCH")!
    expect(JSON.parse(String((patchCall[1] as RequestInit).body))).toEqual({ patch: { "tick-ms": 900 } })
  })

  it("surfaces a refused walk as the conflict notice and sends no patch", async () => {
    daemon()
    const moment = vi.fn(async () =>
      json(503, { error: { code: "unavailable", detail: "restarting: the walk on jinn:ui/before-patch-settings was refused whole", refusal: "restarting" } }),
    )
    const api = createConfigApi({ responseError, conflict, moment })
    const document = await api.getConfig()

    await expect(api.updateConfig({ cron: { "tick-ms": 700 } }, document.revision)).rejects.toMatchObject({
      code: "CONFIG_CONFLICT",
      status: 503,
      message: expect.stringMatching(/^restarting: /),
    })
    expect(conflict).toHaveBeenCalledTimes(1)
    expect(authFetch.mock.calls.some(([, init]) => (init as RequestInit | undefined)?.method === "PATCH")).toBe(false)
  })

  // §9.7 amendment 8(d): the page shows the FOLDED value after a moment, so the
  // save answers the document the daemon holds, not the one the page sent.
  it("answers the folded document, which is what the page shows after the save", async () => {
    daemon()
    const moment = vi.fn(async (_domain: string, _topic: string, payload: unknown) => {
      const { namespace, patch } = payload as { namespace: string; patch: Record<string, unknown> }
      return json(200, { namespace, patch: { ...patch, "tick-ms": 900 } })
    })
    const api = createConfigApi({ responseError, conflict, moment })
    const document = await api.getConfig()
    const saved = await api.updateConfig({ cron: { "tick-ms": 700 } }, document.revision)

    expect(saved.config).toEqual({ cron: { "tick-ms": 900 } })
    expect(saved.revision).toBe(JSON.stringify({ cron: { "tick-ms": 900 } }))
  })

  it("issues no moment when nothing declared changed", async () => {
    daemon()
    const moment = vi.fn()
    const api = createConfigApi({ responseError, conflict, moment })
    const document = await api.getConfig()
    await api.updateConfig({ cron: { "tick-ms": 500, undeclared: true } }, document.revision)
    expect(moment).not.toHaveBeenCalled()
  })
})

it("keeps an applied patch with a pending notification distinct from completed delivery", async () => {
  daemon()
  const fetch = authFetch.getMockImplementation()!
  authFetch.mockImplementation(async (path: string, init?: RequestInit) => {
    const response = await fetch(path, init)
    if (init?.method !== "PATCH") return response
    return json(200, { ...await response.json(), notification: { state: "pending", revision: 1, detail: "waiting for declaration" } })
  })
  const api = createConfigApi({ responseError, conflict, moment: async (_d, _t, payload) => json(200, payload) })
  const document = await api.getConfig()
  const saved = await api.updateConfig({ cron: { "tick-ms": 700 } }, document.revision)
  expect(saved.config).toEqual({ cron: { "tick-ms": 700 } })
  expect(saved.notificationNotice).toMatch(/Saved.*cron.*notification pending/)
  expect(authFetch.mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "PATCH")).toHaveLength(1)
  // The provider restarted: absence of its in-memory record is not confirmation.
  expect((await api.getConfig()).notificationNotice).toMatch(/notification unconfirmed/)
})

it("clears a pending notice only from a settled observation, without writing again", async () => {
  daemon()
  const fetch = authFetch.getMockImplementation()!
  let state = "pending"
  authFetch.mockImplementation(async (path: string, init?: RequestInit) => {
    const response = await fetch(path, init)
    if (path !== "/v1/settings/cron") return response
    return json(200, { ...await response.json(), notification: { state, revision: 1 } })
  })
  const api = createConfigApi({ responseError, conflict, moment: async (_d, _t, payload) => json(200, payload) })
  expect((await api.getConfig()).notificationNotice).toMatch(/pending/)
  state = "settled"
  expect((await api.getConfig()).notificationNotice).toBeUndefined()
  expect(authFetch.mock.calls.some(([, init]) => (init as RequestInit | undefined)?.method === "PATCH")).toBe(false)
})

it("does not send remaining namespace edits after an applied patch goes pending", async () => {
  authFetch.mockImplementation(async (path: string, init?: RequestInit) => {
    if (path === "/v1/settings") return json(200, { namespaces: { first: {}, second: {} } })
    const namespace = path.split("/").at(-1)
    if (init?.method === "PATCH") return json(200, { namespace, settings: { value: 2 }, notification: { state: "pending", revision: 1 } })
    return json(200, { namespace, settings: { value: 1 }, schema: { properties: { value: { kind: "integer" } } } })
  })
  const api = createConfigApi({ responseError, conflict, moment: async (_d, _t, payload) => json(200, payload) })
  const document = await api.getConfig()
  const saved = await api.updateConfig({ first: { value: 2 }, second: { value: 2 } }, document.revision)
  expect(saved.config).toEqual({ first: { value: 2 }, second: { value: 1 } })
  expect(saved.notificationNotice).toMatch(/Remaining namespace edits were not sent/)
  expect(authFetch.mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "PATCH")).toHaveLength(1)
})

function deferredResponse() {
  let resolve!: (response: Response) => void
  const promise = new Promise<Response>((done) => { resolve = done })
  return { promise, resolve }
}
const orderingWire = (period: number, state?: string, revision = 1) => ({
  namespace: "cron", revision, settings: { "tick-ms": period },
  schema: { properties: { "tick-ms": { kind: "integer" } } },
  ...(state ? { notification: { state, revision } } : {}),
})
function orderingApi(reads: Array<Response | Promise<Response>>, patched: Response | Promise<Response>) {
  authFetch.mockImplementation((path: string, init?: RequestInit) => {
    if (path === "/v1/settings") return Promise.resolve(json(200, { namespaces: { cron: {} } }))
    return init?.method === "PATCH" ? patched : reads.shift()
  })
  return createConfigApi({ responseError, conflict, moment: async (_d, _t, payload) => json(200, payload) })
}

it("excludes a read begun before a save from both notification and shared baseline publication", async () => {
  const old = deferredResponse()
  const api = orderingApi([json(200, orderingWire(500, "settled")), old.promise], json(200, orderingWire(700, "pending", 2)))
  const initial = await api.getConfig()
  const read = api.getConfig()
  const rejected = expect(read).rejects.toMatchObject({ name: "SupersededConfigRead" })
  const saved = await api.updateConfig({ cron: { "tick-ms": 700 } }, initial.revision)
  old.resolve(json(200, orderingWire(500, "settled")))
  await rejected
  const unchanged = await api.updateConfig(saved.config, saved.revision)
  expect(unchanged.config).toEqual(saved.config)
  expect(unchanged.notificationNotice).toMatch(/pending/)
  expect(authFetch.mock.calls.filter(([, init]) => init?.method === "PATCH")).toHaveLength(1)
})

it.each([true, false])("only the latest read publishes, including a provider revision reset (old first: %s)", async (oldFirst) => {
  const old = deferredResponse(), current = deferredResponse()
  const api = orderingApi([json(200, orderingWire(500, "pending", 9)), old.promise, current.promise], json(200, {}))
  await api.getConfig()
  const first = api.getConfig()
  await vi.waitFor(() => expect(authFetch).toHaveBeenCalledTimes(4))
  const rejected = expect(first).rejects.toMatchObject({ name: "SupersededConfigRead" })
  const second = api.getConfig()
  if (oldFirst) { old.resolve(json(200, orderingWire(500, "settled", 9))); await rejected }
  current.resolve(json(200, orderingWire(700, undefined, 0)))
  const latest = await second
  if (!oldFirst) { old.resolve(json(200, orderingWire(500, "settled", 9))); await rejected }
  expect(latest.notificationNotice).toMatch(/unconfirmed/)
  const baseline = await api.updateConfig(latest.config, latest.revision)
  expect(baseline.config).toEqual({ cron: { "tick-ms": 700 } })
  expect(baseline.notificationNotice).toMatch(/unconfirmed/)
  expect(authFetch.mock.calls.filter(([, init]) => init?.method === "PATCH")).toHaveLength(0)
})

it.each([true, false])("excludes reads begun during a save (delivered before the save: %s)", async (beforeSave) => {
  const patch = deferredResponse(), read = deferredResponse()
  const api = orderingApi([json(200, orderingWire(500)), read.promise], patch.promise)
  const initial = await api.getConfig()
  const save = api.updateConfig({ cron: { "tick-ms": 700 } }, initial.revision)
  const reload = api.getConfig()
  const rejected = expect(reload).rejects.toMatchObject({ name: "SupersededConfigRead" })
  if (beforeSave) { read.resolve(json(200, orderingWire(500, "settled"))); await rejected }
  patch.resolve(json(200, orderingWire(700, "pending", 2)))
  const saved = await save
  if (!beforeSave) { read.resolve(json(200, orderingWire(500, "settled"))); await rejected }
  expect((await api.updateConfig(saved.config, saved.revision)).notificationNotice).toMatch(/pending/)
  expect(authFetch.mock.calls.filter(([, init]) => init?.method === "PATCH")).toHaveLength(1)
})

it("does not publish a superseded read error over a newer observation", async () => {
  const old = deferredResponse()
  const api = orderingApi([old.promise, json(200, orderingWire(700, "pending"))], json(200, {}))
  const first = api.getConfig()
  await vi.waitFor(() => expect(authFetch).toHaveBeenCalledTimes(2))
  const rejected = expect(first).rejects.toMatchObject({ name: "SupersededConfigRead" })
  const latest = await api.getConfig()
  old.resolve(json(500, {}))
  await rejected
  expect((await api.updateConfig(latest.config, latest.revision)).notificationNotice).toMatch(/pending/)
})
