import type { PlatformAdapter } from "../contracts"
import { nativeBridge } from "../native-bridge"

/** The shell transport is live in S3, but no user-facing platform intent is
 * implemented through it yet. Keeping those intents explicitly unsupported is
 * part of the contract: transport availability must never imply permission. */
export function createTauriAdapter(): PlatformAdapter {
  return {
    name: "tauri",
    capability: async () => ({
      supported: false,
      permission: "not-applicable",
      configured: nativeBridge() !== undefined,
      available: false,
    }),
    perform: async () => ({ status: "unsupported", reason: "not-implemented" }),
    observe: () => null,
  }
}
