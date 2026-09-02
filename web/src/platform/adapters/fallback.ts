import type { PlatformAdapter } from "../contracts"

export function createFallbackAdapter(name = "fallback"): PlatformAdapter {
  return {
    name,
    capability: async () => ({
      supported: false,
      permission: "not-applicable",
      configured: false,
      available: false,
    }),
    perform: async () => ({ status: "unsupported" }),
    observe: () => null,
  }
}
