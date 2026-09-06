import { useRef, useState } from "react"
import { api } from "@/lib/api"
import { SupersededConfigRead, type ConfigDocument } from "@/lib/api-config"

/** Only the current Reload can publish to the page; edits and saves supersede it. */
export function useConfigLoad(onLoaded: (document: ConfigDocument) => void) {
  const current = useRef(0)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  function invalidateLoad() {
    ++current.current
    setLoading(false)
  }

  function loadConfig() {
    const load = ++current.current
    setLoading(true)
    api.getConfig()
      .then((document) => {
        if (load !== current.current) return
        onLoaded(document)
        setError(null)
      })
      .catch((err) => {
        if (load === current.current && !(err instanceof SupersededConfigRead)) setError(err.message)
      })
      .finally(() => { if (load === current.current) setLoading(false) })
  }

  return { loading, error, loadConfig, invalidateLoad }
}
