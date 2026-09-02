/**
 * Whether this gateway could open a voice session at all.
 *
 * Its own module because two unrelated surfaces ask: the orb, before it mints
 * anything, and the Settings page, to say whether the configuration it is
 * showing would actually work. Neither can answer from the config block alone —
 * a key held as `${ENV_VAR}` only resolves where the gateway runs.
 *
 * UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 11, §8 amendment 6): Talk
 * is out of scope (§7) and the daemon has no talk route, so the capability
 * resolves ABSENT client-side and issues no request. The mounted Settings page
 * asks on every visit; the answer is the same one the old gateway gave for a
 * gateway with no provider configured.
 */

export interface TalkCapability {
  configured: boolean
  provider: string | null
  /** The provider names this gateway implements. */
  providers: string[]
  /** The configured provider's voices. Empty until a known provider is set. */
  voices: string[]
}

/** Asks nothing, and opens nothing: on the daemon, voice is not configured. */
export function fetchTalkCapability(): Promise<TalkCapability> {
  return Promise.resolve({ configured: false, provider: null, providers: [], voices: [] })
}
