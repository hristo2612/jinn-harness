import { beforeEach, describe, expect, it, vi } from "vitest"
import type { NativeResponsePayload } from "@/platform/native-bridge"
import {
  StaleGatewayGenerationError,
  createNativeGatewayProfiles,
} from "../native-gateway-profiles"
import { MemoryStorage, bridgeFixture, deferred, response } from "./native-gateway-fixtures"

describe("native gateway profiles", () => {
  beforeEach(() => vi.restoreAllMocks())

  it("pairs a second exact-port profile without activating it", async () => {
    const { bridge } = bridgeFixture()
    const profiles = createNativeGatewayProfiles({ bridge, storage: new MemoryStorage() })

    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha-code", { activate: true })
    const generation = profiles.snapshot().generation
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta-code")

    expect(profiles.snapshot()).toMatchObject({ activeId: alpha.id, generation })
    expect(profiles.snapshot().profiles).toEqual([alpha, beta])
    expect(bridge.pair).toHaveBeenNthCalledWith(2, {
      target: { origin: "http://127.0.0.1:7780" },
      code: "beta-code",
    })
  })

  it("switches A to B to A while preserving the exact active identity", async () => {
    const { bridge } = bridgeFixture()
    const storage = new MemoryStorage()
    const beforeCommit = vi.fn(async () => {})
    const profiles = createNativeGatewayProfiles({ bridge, storage, beforeCommit })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")

    await profiles.select(beta.id)
    expect(profiles.snapshot()).toMatchObject({ activeId: beta.id, generation: 2 })
    expect(profiles.transport.profile.origin).toBe("http://127.0.0.1:7780")
    await profiles.select(alpha.id)
    expect(profiles.snapshot()).toMatchObject({ activeId: alpha.id, generation: 3 })
    expect(profiles.transport.profile.origin).toBe("http://127.0.0.1:7779")
    expect(beforeCommit).toHaveBeenCalledTimes(3)

    const restored = createNativeGatewayProfiles({ bridge, storage })
    expect(restored.snapshot().activeId).toBe(alpha.id)
  })

  it("quarantines a REST response and WebSocket frame delivered after a switch", async () => {
    const { bridge, requests, streams } = bridgeFixture()
    const profiles = createNativeGatewayProfiles({ bridge, storage: new MemoryStorage() })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")
    const late = deferred<NativeResponsePayload>()
    requests.mockImplementationOnce(() => late.promise)
    const pending = profiles.transport.request("/api/sessions")
    const frames = vi.fn()
    const socket = profiles.transport.openSocket("/ws")
    socket.onmessage = frames
    await vi.waitFor(() => expect(streams.size).toBe(1))
    const alphaStream = [...streams.values()][0]!

    await profiles.select(beta.id)
    late.resolve(response({ sessions: [{ id: "alpha-only" }] }))
    alphaStream({ event: "message", streamId: "stream-1", text: JSON.stringify({ event: "sessions:changed" }) })

    await expect(pending).rejects.toBeInstanceOf(StaleGatewayGenerationError)
    expect(frames).not.toHaveBeenCalled()
    expect(profiles.snapshot().activeId).toBe(beta.id)
    expect(alpha.id).not.toBe(beta.id)
  })

  it("quarantines a REST response and WebSocket frame delivered after switching back", async () => {
    const { bridge, requests, streams } = bridgeFixture()
    const profiles = createNativeGatewayProfiles({ bridge, storage: new MemoryStorage() })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")
    await profiles.select(beta.id)
    const late = deferred<NativeResponsePayload>()
    requests.mockImplementationOnce(() => late.promise)
    const pending = profiles.transport.request("/api/sessions")
    const frames = vi.fn()
    const socket = profiles.transport.openSocket("/ws")
    socket.onmessage = frames
    await vi.waitFor(() => expect(streams.size).toBe(1))
    const betaStream = [...streams.values()][0]!

    await profiles.select(alpha.id)
    late.resolve(response({ sessions: [{ id: "beta-only" }] }))
    betaStream({ event: "message", streamId: "stream-1", text: JSON.stringify({ event: "sessions:changed" }) })

    await expect(pending).rejects.toBeInstanceOf(StaleGatewayGenerationError)
    expect(frames).not.toHaveBeenCalled()
    expect(profiles.snapshot().activeId).toBe(alpha.id)
  })

  it("removes only the requested inactive profile and keeps the active profile intact", async () => {
    const { bridge } = bridgeFixture()
    const storage = new MemoryStorage()
    const profiles = createNativeGatewayProfiles({ bridge, storage })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")

    await profiles.remove(beta.id)

    expect(profiles.snapshot()).toMatchObject({ activeId: alpha.id, profiles: [alpha] })
    expect(bridge.forget).toHaveBeenCalledWith({ target: { origin: beta.origin } })
    expect(profiles.transport.profile.origin).toBe(alpha.origin)
  })

  it("keeps the active profile authenticated, connected, and stored when the other is removed", async () => {
    const { bridge, requests, streams } = bridgeFixture()
    const storage = new MemoryStorage()
    const profiles = createNativeGatewayProfiles({ bridge, storage })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")
    const frames = vi.fn()
    const socket = profiles.transport.openSocket("/ws")
    socket.onmessage = frames
    await vi.waitFor(() => expect(streams.size).toBe(1))
    const alphaStream = [...streams.values()][0]!

    await profiles.remove(beta.id)

    // Authentication: only beta's credential is revoked, and alpha still answers.
    expect(bridge.forget).toHaveBeenCalledTimes(1)
    expect(bridge.forget).toHaveBeenCalledWith({ target: { origin: beta.origin } })
    const state = await profiles.transport.request("/api/auth/state")
    expect(state.ok).toBe(true)
    expect(requests).toHaveBeenLastCalledWith(expect.objectContaining({ target: { origin: alpha.origin } }))

    // Connection state: removing an inactive profile never quarantines alpha's live socket.
    alphaStream({ event: "message", streamId: "stream-1", text: JSON.stringify({ event: "sessions:changed" }) })
    expect(frames).toHaveBeenCalledTimes(1)

    // Cached data: the persisted store still restores alpha, and only alpha.
    expect(createNativeGatewayProfiles({ bridge, storage }).snapshot()).toMatchObject({
      activeId: alpha.id,
      profiles: [alpha],
    })
  })

  it("commits an unreachable selection and retries that selected profile", async () => {
    const { bridge, requests } = bridgeFixture()
    const storage = new MemoryStorage()
    const profiles = createNativeGatewayProfiles({ bridge, storage })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")
    requests.mockRejectedValueOnce(new TypeError("connection refused"))

    await expect(profiles.select(beta.id)).rejects.toThrow("connection refused")

    expect(profiles.snapshot()).toMatchObject({
      activeId: beta.id,
      status: "unreachable",
      failedProfileId: beta.id,
      activeReachable: false,
    })
    expect(profiles.transport.profile.origin).toBe(beta.origin)
    expect(createNativeGatewayProfiles({ bridge, storage }).snapshot().activeId).toBe(beta.id)

    await profiles.retry()

    expect(requests).toHaveBeenLastCalledWith(expect.objectContaining({ target: { origin: beta.origin } }))
    expect(profiles.snapshot()).toMatchObject({ activeId: beta.id, status: "ready", activeReachable: true })
    expect(alpha.id).not.toBe(beta.id)
  })

  it("names the profile a switch is reaching for while the switch is in flight", async () => {
    const { bridge, requests } = bridgeFixture()
    const profiles = createNativeGatewayProfiles({ bridge, storage: new MemoryStorage() })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")
    const validation = deferred<NativeResponsePayload>()
    requests.mockImplementationOnce(() => validation.promise)

    const selecting = profiles.select(beta.id)
    expect(profiles.snapshot()).toMatchObject({ status: "switching", switchingProfileId: beta.id, activeId: alpha.id })

    validation.resolve(response({ authRequired: true, authenticated: true, instance: "beta" }))
    await selecting
    expect(profiles.snapshot().switchingProfileId).toBeUndefined()
  })

  it("reports a remembered gateway that no longer answers instead of trusting storage", async () => {
    const { bridge, requests } = bridgeFixture()
    const storage = new MemoryStorage()
    const alpha = await createNativeGatewayProfiles({ bridge, storage }).pair("http://127.0.0.1:7779", "alpha", { activate: true })

    // A reload: the profile is remembered, the gateway behind it is gone.
    const reloaded = createNativeGatewayProfiles({ bridge, storage })
    expect(reloaded.snapshot()).toMatchObject({ activeId: alpha.id, activeReachable: false })
    requests.mockRejectedValueOnce(new TypeError("connection refused"))

    await expect(reloaded.verifyActive()).resolves.toBeUndefined()

    expect(reloaded.snapshot()).toMatchObject({
      activeId: alpha.id,
      status: "unreachable",
      failedProfileId: alpha.id,
      activeReachable: false,
      error: "connection refused",
    })
  })

  it("proves a remembered gateway that still answers", async () => {
    const { bridge, storage } = { ...bridgeFixture(), storage: new MemoryStorage() }
    await createNativeGatewayProfiles({ bridge, storage }).pair("http://127.0.0.1:7779", "alpha", { activate: true })

    const reloaded = createNativeGatewayProfiles({ bridge, storage })
    await reloaded.verifyActive()

    expect(reloaded.snapshot()).toMatchObject({ status: "ready", activeReachable: true, failedProfileId: undefined })
  })

  it("retries the newly active profile after a failed switch", async () => {
    const { bridge, requests } = bridgeFixture()
    const profiles = createNativeGatewayProfiles({ bridge, storage: new MemoryStorage() })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")
    await profiles.select(beta.id)
    requests.mockRejectedValueOnce(new TypeError("connection refused"))
    await expect(profiles.select(alpha.id)).rejects.toThrow("connection refused")

    await profiles.retry()

    expect(requests).toHaveBeenLastCalledWith(expect.objectContaining({ target: { origin: alpha.origin } }))
    expect(profiles.snapshot()).toMatchObject({ activeId: alpha.id, status: "ready", activeReachable: true })
  })

  it("does not strand a retry when the profile a failed switch named is removed", async () => {
    const { bridge, requests } = bridgeFixture()
    const profiles = createNativeGatewayProfiles({ bridge, storage: new MemoryStorage() })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")
    requests.mockRejectedValueOnce(new TypeError("connection refused"))
    await expect(profiles.select(beta.id)).rejects.toThrow("connection refused")

    await profiles.remove(beta.id)

    expect(profiles.snapshot()).toMatchObject({ status: "ready", failedProfileId: undefined, error: undefined })
    await expect(profiles.retry()).resolves.toBeUndefined()
    expect(requests).toHaveBeenLastCalledWith(expect.objectContaining({ target: { origin: alpha.origin } }))
  })

  it("cannot activate a profile removed while its selection check is in flight", async () => {
    const { bridge, requests } = bridgeFixture()
    const profiles = createNativeGatewayProfiles({ bridge, storage: new MemoryStorage() })
    const alpha = await profiles.pair("http://127.0.0.1:7779", "alpha", { activate: true })
    const beta = await profiles.pair("http://127.0.0.1:7780", "beta")
    const validation = deferred<NativeResponsePayload>()
    requests.mockImplementationOnce(() => validation.promise)

    const selecting = profiles.select(beta.id)
    await profiles.remove(beta.id)
    validation.resolve(response({ authRequired: true, authenticated: true, instance: "beta" }))

    await expect(selecting).rejects.toThrow(`Unknown native gateway profile: ${beta.id}`)
    expect(profiles.snapshot()).toMatchObject({ activeId: alpha.id, profiles: [alpha] })
  })
})
