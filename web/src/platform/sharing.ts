import type { OperationResult } from "./contracts"
import { getPlatform } from "./platform"

export function share(content: { title?: string; text?: string; url?: string }): Promise<OperationResult> {
  return getPlatform().perform({ kind: "sharing.share", ...content })
}

export function copyText(text: string): Promise<OperationResult> {
  return getPlatform().perform({ kind: "clipboard.copy", text })
}

export function openExternal(url: string): Promise<OperationResult> {
  return getPlatform().perform({ kind: "navigation.open-external", url })
}
