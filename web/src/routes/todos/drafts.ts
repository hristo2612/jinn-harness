import { useEffect, useRef, useState } from 'react'

/** Saved locally on each keystroke so route changes preserve unfinished text. */
export function useTodoDraft<T>(key: string, empty: T) {
  const [value, setValue] = useState<T>(() => {
    try { return JSON.parse(sessionStorage.getItem(key) ?? 'null') as T ?? empty } catch { return empty }
  })
  const current = useRef(value)
  const [error, setError] = useState<string | null>(null)
  function save(next: T) {
    current.current = next; setValue(next)
    try { sessionStorage.setItem(key, JSON.stringify(next)); setError(null) }
    catch { setError('Draft storage is unavailable. Keep this page open to retain your text.') }
  }
  const dirty = JSON.stringify(value) !== JSON.stringify(empty)
  useEffect(() => {
    if (!dirty) return
    const guard = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = '' }
    window.addEventListener('beforeunload', guard)
    return () => window.removeEventListener('beforeunload', guard)
  }, [dirty])
  function clearIf(expected: T) { if (JSON.stringify(current.current) === JSON.stringify(expected)) save(empty) }
  return { value, save, clearIf, error }
}
