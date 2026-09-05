import { useFeatures } from "@/hooks/use-features"
import { useProvidedNavigation } from "@/lib/use-provided-navigation"

export function NavigationNotice() {
  const { data: features } = useFeatures()
  const { notice, refresh } = useProvidedNavigation(features?.notesEnabled === true)
  if (!notice) return null
  return <div role="status" className="flex shrink-0 flex-wrap items-center gap-2 bg-[var(--fill-secondary)] px-4 py-2 text-[length:var(--text-caption1)] text-[var(--text-primary)]">
    <span className="min-w-0 flex-1 break-words">{notice}</span>
    <button className="min-h-10 rounded-xl px-3" onClick={() => void refresh()}>Retry navigation</button>
  </div>
}
