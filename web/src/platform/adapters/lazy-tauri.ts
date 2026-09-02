import type { PlatformAdapter } from "../contracts"

type Loader = () => Promise<PlatformAdapter>

export function createLazyTauriAdapter(
  load: Loader = async () => (await import("./tauri")).createTauriAdapter(),
): PlatformAdapter {
  let pending: Promise<PlatformAdapter> | undefined
  const adapter = () => (pending ??= load())

  return {
    name: "tauri-lazy",
    capability: async (capability, runtime) => runtime.container === "tauri"
      ? (await adapter()).capability(capability, runtime)
      : { supported: false, permission: "not-applicable", configured: false, available: false },
    perform: async (intent, runtime) => runtime.container === "tauri"
      ? (await adapter()).perform(intent, runtime)
      : { status: "unsupported" },
    observe: () => null,
  }
}
