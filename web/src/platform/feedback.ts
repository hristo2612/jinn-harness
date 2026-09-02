import { getPlatform } from "./platform"

export function selectionFeedback(): void {
  void getPlatform().perform({ kind: "feedback.selection" })
}
