/**
 * Whether this gateway could open a voice session at all.
 *
 * Its own module because two unrelated surfaces ask: the orb, before it mints
 * anything, and the Settings page, to say whether the configuration it is
 * showing would actually work. Neither can answer from the config block alone —
 * a key held as `${ENV_VAR}` only resolves where the gateway runs.
 */
import { authFetch } from "./auth"

export interface TalkCapability {
  configured: boolean
  provider: string | null
  /** The provider names this gateway implements. */
  providers: string[]
  /** The configured provider's voices. Empty until a known provider is set. */
  voices: string[]
}

/** Asks, and opens nothing. The answer never carries the provider key. */
export async function fetchTalkCapability(): Promise<TalkCapability> {
  const response = await authFetch("/api/talk/config", { method: "GET" })
  if (!response.ok) throw new Error(`The gateway answered ${response.status} for the voice capability.`)
  const probed = (await response.json()) as Partial<TalkCapability>
  return {
    configured: probed.configured === true,
    provider: probed.provider ?? null,
    providers: Array.isArray(probed.providers) ? probed.providers : [],
    voices: Array.isArray(probed.voices) ? probed.voices : [],
  }
}
