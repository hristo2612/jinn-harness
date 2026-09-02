import { beforeEach, describe, expect, it, vi } from "vitest"

const authFetch = vi.fn()
vi.mock("@/lib/auth", () => ({ authFetch: (...args: unknown[]) => authFetch(...args) }))

const { fetchTalkCapability } = await import("../talk-capability")

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } })
}

beforeEach(() => {
  authFetch.mockReset()
})

describe("asking whether voice is set up", () => {
  it("reads the capability without opening anything", async () => {
    authFetch.mockResolvedValue(
      json({ configured: true, provider: "openai", providers: ["openai"], voices: ["marin", "cedar"] }),
    )

    const capability = await fetchTalkCapability()

    expect(capability).toEqual({
      configured: true,
      provider: "openai",
      providers: ["openai"],
      voices: ["marin", "cedar"],
    })
    const [url, init] = authFetch.mock.calls[0] as [string, RequestInit]
    expect(url).toBe("/api/talk/config")
    expect(init.method).toBe("GET")
  })

  it("reads a gateway that answers nothing as not configured, rather than as ready", async () => {
    authFetch.mockResolvedValue(json({}))

    expect(await fetchTalkCapability()).toEqual({
      configured: false,
      provider: null,
      providers: [],
      voices: [],
    })
  })

  it("refuses to call a refused probe an answer", async () => {
    authFetch.mockResolvedValue(json({ error: "nope" }, 500))

    await expect(fetchTalkCapability()).rejects.toThrow("500")
  })
})
