export const INTENT_FAMILIES = [
  "feedback",
  "notifications",
  "badges",
  "sharing",
  "lifecycle",
  "navigation",
  "viewport",
  "clipboard",
  "files",
  "install",
  "window",
  "device",
] as const

export type IntentFamily = (typeof INTENT_FAMILIES)[number]
export type PermissionState = "granted" | "prompt" | "denied" | "not-applicable"

export interface Runtime {
  container: "browser" | "pwa" | "tauri"
  os: "android" | "ios" | "linux" | "macos" | "windows" | "unknown"
  engine: "blink" | "gecko" | "webkit" | "unknown"
  secureContext: boolean
  appVersion: string
  userAgent: string
}

export type PlatformIntent =
  | { kind: "feedback.selection" }
  | { kind: "feedback.impact"; style?: "light" | "medium" | "heavy" }
  | { kind: "feedback.notify"; level?: "success" | "warning" | "error" }
  | { kind: "notifications.present"; title: string; body?: string }
  | { kind: "notifications.request-permission"; userGesture: boolean }
  | { kind: "badges.set"; count: number }
  | { kind: "badges.clear" }
  | { kind: "sharing.share"; title?: string; text?: string; url?: string }
  | { kind: "navigation.open-external"; url: string }
  | { kind: "viewport.set-orientation"; orientation: "portrait" | "landscape" | "any" }
  | { kind: "clipboard.copy"; text: string }
  | { kind: "files.pick"; accepts: readonly string[]; multiple?: boolean }
  | { kind: "files.save"; name: string; data: Blob | string }
  | { kind: "install.request" }
  | { kind: "install.check-update" }
  | { kind: "window.set-state"; state: "minimized" | "maximized" | "normal" | "fullscreen" }
  | { kind: "window.set-title"; title: string }
  | { kind: "device.biometrics"; reason: string }
  | { kind: "device.secure-store"; key: string; value: string }

export type PlatformCapability =
  | PlatformIntent["kind"]
  | "lifecycle.resume"
  | "lifecycle.pause"
  | "lifecycle.online"
  | "viewport.keyboard-inset"

export type PlatformEvent =
  | { kind: "lifecycle.resume" }
  | { kind: "lifecycle.pause" }
  | { kind: "lifecycle.online"; online: boolean }
  | { kind: "viewport.keyboard-inset"; inset: number }

export interface Capability {
  supported: boolean
  permission: PermissionState
  configured: boolean
  available: boolean
}

export type OperationResult<T = unknown> =
  | { status: "performed"; value?: T }
  | { status: "unsupported"; reason?: string }
  | { status: "permission-required"; permission: PermissionState }
  | { status: "denied"; permission: PermissionState }
  | { status: "cancelled" }
  | { status: "failed"; error: { code: string; message: string } }

export type PlatformEventListener = (event: PlatformEvent) => void
export type Unsubscribe = () => void

export interface PlatformAdapter {
  name: string
  capability(capability: PlatformCapability, runtime: Runtime): Promise<Capability>
  perform(intent: PlatformIntent, runtime: Runtime): Promise<OperationResult>
  observe(capability: PlatformCapability, listener: PlatformEventListener, runtime: Runtime): Unsubscribe | null
}

export interface Platform {
  readonly runtime: Runtime
  capability(capability: PlatformCapability): Promise<Capability>
  perform<T = unknown>(intent: PlatformIntent): Promise<OperationResult<T>>
  observe(capability: PlatformCapability, listener: PlatformEventListener): Unsubscribe
}

const unsupportedCapability: Capability = {
  supported: false,
  permission: "not-applicable",
  configured: false,
  available: false,
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

export function createPlatform(options: { runtime: Runtime; adapters: PlatformAdapter[] }): Platform {
  const adapters = [...options.adapters]

  return {
    runtime: options.runtime,

    async capability(capability) {
      for (const adapter of adapters) {
        try {
          const result = await adapter.capability(capability, options.runtime)
          if (result.supported) return result
        } catch {
          // Capability discovery is advisory and must never break a caller.
        }
      }
      return unsupportedCapability
    },

    async perform<T>(intent: PlatformIntent): Promise<OperationResult<T>> {
      for (const adapter of adapters) {
        try {
          const result = await adapter.perform(intent, options.runtime)
          if (result.status !== "unsupported") return result as OperationResult<T>
        } catch (error) {
          return {
            status: "failed",
            error: { code: "adapter-error", message: errorMessage(error) },
          }
        }
      }
      return { status: "unsupported" }
    },

    observe(capability, listener) {
      for (const adapter of adapters) {
        try {
          const unsubscribe = adapter.observe(capability, listener, options.runtime)
          if (unsubscribe) return unsubscribe
        } catch {
          // Observation is optional. Continue to the next adapter if one fails.
        }
      }
      return () => {}
    },
  }
}
