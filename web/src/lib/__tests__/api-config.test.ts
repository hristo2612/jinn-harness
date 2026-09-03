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
