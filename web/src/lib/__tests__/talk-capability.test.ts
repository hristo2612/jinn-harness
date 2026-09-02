import { beforeEach, describe, expect, it, vi } from "vitest"

const authFetch = vi.fn()
vi.mock("@/lib/auth", () => ({ authFetch: (...args: unknown[]) => authFetch(...args) }))

const { fetchTalkCapability } = await import("../talk-capability")

beforeEach(() => {
  authFetch.mockReset()
})

// UI-1 §4.2 item 11 (§8 amendment 6): Talk is out of scope, so the capability
// resolves ABSENT client-side and issues no request. The three tests of the
// old gateway's talk probe went with the probe.
describe("asking whether voice is set up", () => {
  it("resolves absent without asking the gateway", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch")

    expect(await fetchTalkCapability()).toEqual({
      configured: false,
      provider: null,
      providers: [],
      voices: [],
    })

    expect(authFetch).not.toHaveBeenCalled()
    expect(fetchSpy).not.toHaveBeenCalled()
  })
})
