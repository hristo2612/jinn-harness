import { gatewayTransport } from "./gateway-transport"
import { ApiError } from "@/lib/api"

export interface AuthState {
  authRequired: boolean
  authenticated: boolean
  canBootstrapLocal: boolean
  networkExposed: boolean
  /** Instance name serving this page, so pairing hints can name `jinn -i <instance> pair`. */
  instance?: string
}

export interface PairingCode {
  code: string
  expiresAt: string
  ttlSeconds?: number
}

export interface PairedDevice {
  id: string
  name: string
  kind?: "local" | "remote" | "token"
  createdAt?: string
  lastSeenAt?: string
  lastIp?: string
  userAgent?: string
  current?: boolean
}

/**
 * UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 2): the daemon's door is
 * bearer-only. The operator credential — the contents of the file
 * `<data>.operator-token` beside the daemon's data root — lives in
 * sessionStorage for the life of the tab, never in a cookie, and rides on every
 * request as `Authorization: Bearer`. The four-field state is synthesised here:
 * authenticated means `GET /v1/health` with the held bearer answered 200.
 */
const CREDENTIAL_STORAGE_KEY = "jinn-operator-credential"
const WORKSPACE_PAIRING_HASH_KEY = "jinn-pair"

function takeHashValue(key: string): string | undefined {
  if (typeof window === "undefined") return undefined
  const params = new URLSearchParams(window.location.hash.replace(/^#/, ""))
  const value = params.get(key)?.trim()
  if (!value) return undefined
  params.delete(key)
  const nextHash = params.toString()
  window.history.replaceState(
    window.history.state,
    "",
    `${window.location.pathname}${window.location.search}${nextHash ? `#${nextHash}` : ""}`,
  )
  return value
}

/** Consume the short-lived pairing code returned by authenticated workspace
 * creation. URL fragments never reach the server or referrer, and are removed
 * before the code is exchanged for the new instance's HttpOnly cookie. */
export function takeWorkspacePairingCode(): string | undefined {
  return takeHashValue(WORKSPACE_PAIRING_HASH_KEY)
}

function readCredential(): string | null {
  try {
    return window.sessionStorage.getItem(CREDENTIAL_STORAGE_KEY)
  } catch {
    return null
  }
}

function storeCredential(secret: string): void {
  try {
    window.sessionStorage.setItem(CREDENTIAL_STORAGE_KEY, secret)
  } catch {
    /* a tab with no storage pairs for one request at most */
  }
}

function clearCredential(): void {
  try {
    window.sessionStorage.removeItem(CREDENTIAL_STORAGE_KEY)
  } catch {
    /* nothing held */
  }
}

const clearedListeners = new Set<() => void>()

/** Fires when a 401 clears the held credential: the transparent retry of the
 *  cookie era is a transparent sign-out here, and the provider shows the
 *  pairing screen. */
export function onCredentialCleared(listener: () => void): () => void {
  clearedListeners.add(listener)
  return () => {
    clearedListeners.delete(listener)
  }
}

function signOut(): void {
  clearCredential()
  for (const listener of clearedListeners) listener()
}

/** The one shape a call with no `/v1` counterpart rejects with. */
function noCounterpart(path: string): ApiError {
  return new ApiError(501, `no /v1 counterpart in UI-1: ${path}`, "no-counterpart")
}

function withBearer(init: RequestInit): RequestInit {
  const headers = new Headers(init.headers)
  const credential = readCredential()
  if (credential !== null) headers.set("Authorization", `Bearer ${credential}`)
  return { ...init, headers }
}

let knownInstance: string | null = null

export function authUrl(path: string): string {
  return gatewayTransport().httpUrl(path)
}

/** `GET /v1/health` with the held bearer: 200 is authenticated. */
async function healthAnswers(): Promise<boolean> {
  const res = await gatewayTransport().request("/v1/health", withBearer({ method: "GET" }))
  return res.ok
}

export async function getAuthState(): Promise<AuthState> {
  const authenticated = readCredential() !== null && (await healthAnswers())
  return { authRequired: true, authenticated, canBootstrapLocal: false, networkExposed: false }
}

/** The instance name the gateway last reported, for the surfaces that have to
 *  name which Jinn they are on without waiting on a request. Null until the
 *  app's first auth-state read has landed. */
export function lastKnownInstance(): string | null {
  return knownInstance
}

export async function bootstrapLocalAuth(): Promise<boolean> {
  return false
}

/** Store the pasted credential, then prove it against `/v1/health`. Either
 *  mode carries the credential: there is no code to exchange. */
export async function pairBrowser(secret: string, _mode: "code" | "token" = "token"): Promise<void> {
  storeCredential(secret)
  if (await healthAnswers()) return
  clearCredential()
  throw new Error("The operator credential was not accepted: /v1/health did not answer 200 with it")
}

export async function createPairingCode(): Promise<PairingCode> {
  throw noCounterpart("the old gateway's auth/pairing-codes route")
}

export async function listPairedDevices(): Promise<PairedDevice[]> {
  return []
}

export async function unpairDevice(deviceId: string): Promise<void> {
  throw noCounterpart(`the old gateway's auth/devices/${encodeURIComponent(deviceId)} route`)
}

export async function logoutBrowser(): Promise<void> {
  clearCredential()
}

export async function authFetch(input: string, init: RequestInit = {}): Promise<Response> {
  const res = await gatewayTransport().request(input, withBearer(init))
  if (res.status === 401) signOut()
  return res
}
