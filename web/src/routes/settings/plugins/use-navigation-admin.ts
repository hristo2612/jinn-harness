import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { profileAdmin } from "@/lib/profile-admin"
import { NAVIGATION_ENTRY, NAVIGATION_QUERY_KEY } from "@/lib/navigation-extension"
import { navigationInstall, navigationSettled, readNavigationSnapshot, NAVIGATION_STATE_KEY, type NavigationRequest } from "./navigation-state"

export function useNavigationAdmin() {
  const client = useQueryClient()
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState("")
  const snapshot = useQuery({ queryKey: NAVIGATION_STATE_KEY, queryFn: readNavigationSnapshot, retry: false })
  const refresh = async () => {
    await client.cancelQueries({ queryKey: NAVIGATION_QUERY_KEY })
    await Promise.all([
      client.invalidateQueries({ queryKey: NAVIGATION_QUERY_KEY }),
      client.invalidateQueries({ queryKey: NAVIGATION_STATE_KEY }),
      client.invalidateQueries({ queryKey: ["plugin-inventory"] }),
    ])
  }
  const act = async (operation: NavigationRequest["operation"]) => {
    let acceptedRecord: number | undefined
    setBusy(true)
    setMessage("Sending request…")
    try {
      await client.cancelQueries({ queryKey: NAVIGATION_QUERY_KEY })
      const before = await readNavigationSnapshot()
      const ordinal = Math.max(0, ...before.witnessed.map(row => row.ordinal))
      const answer = operation === "add" ? await profileAdmin.addEntry(navigationInstall(before))
        : operation === "remove" ? await profileAdmin.removeEntry(NAVIGATION_ENTRY)
          : await profileAdmin.setDisabled(NAVIGATION_ENTRY, operation === "disable")
      acceptedRecord = answer["administered-seq"]
      setMessage(`Accepted (record ${answer["administered-seq"]}); waiting for runtime…`)
      const request = { operation, ordinal, seq: answer["administered-seq"] }
      const deadline = Date.now() + 10_000
      let settled = false
      while (Date.now() < deadline) {
        await new Promise(resolve => setTimeout(resolve, 500))
        const current = await readNavigationSnapshot()
        client.setQueryData(NAVIGATION_STATE_KEY, current)
        if (navigationSettled(current, request)) { settled = true; break }
      }
      setMessage(confirmation(request, settled))
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error)
      setMessage(acceptedRecord === undefined ? detail : `Accepted (record ${acceptedRecord}); runtime unconfirmed: ${detail}`)
    } finally {
      await refresh()
      setBusy(false)
    }
  }
  return { snapshot, busy, message, act, refresh }
}

function confirmation(request: NavigationRequest, settled: boolean): string {
  if (!settled) return `Accepted (record ${request.seq}); runtime unconfirmed. Refresh to inspect current evidence.`
  const observed = { remove: "Removal", disable: "Disposal", enable: "Activation", add: "Activation" }
  return `${observed[request.operation]} witnessed after record ${request.seq}.`
}
