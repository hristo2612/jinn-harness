import { useState } from "react"
import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query"
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
    let requestSent = false
    const preparationDeadline = Date.now() + 10_000
    setBusy(true)
    setMessage("Sending request…")
    try {
      await beforeDeadline(client.cancelQueries({ queryKey: NAVIGATION_QUERY_KEY }), preparationDeadline)
      const before = await beforeDeadline(readNavigationSnapshot(), preparationDeadline)
      const ordinal = Math.max(0, ...before.witnessed.map(row => row.ordinal))
      const install = operation === "add" ? navigationInstall(before) : undefined
      requestSent = true
      const write = install ? profileAdmin.addEntry(install)
        : operation === "remove" ? profileAdmin.removeEntry(NAVIGATION_ENTRY)
          : profileAdmin.setDisabled(NAVIGATION_ENTRY, operation === "disable")
      const answer = await beforeDeadline(write, preparationDeadline)
      acceptedRecord = answer["administered-seq"]
      setMessage(`Accepted (record ${answer["administered-seq"]}); waiting for runtime…`)
      const request = { operation, ordinal, seq: answer["administered-seq"] }
      const settled = await awaitSettlement(request, client)
      setMessage(confirmation(request, settled))
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error)
      setMessage(uncertainMessage(acceptedRecord, requestSent, detail))
    } finally {
      setBusy(false)
      void refresh().catch(() => {})
    }
  }
  return { snapshot, busy, message, act, refresh }
}

function confirmation(request: NavigationRequest, settled: boolean): string {
  if (!settled) return `Accepted (record ${request.seq}); runtime unconfirmed. Refresh to inspect current evidence.`
  const observed = { remove: "Removal", disable: "Disposal", enable: "Activation", add: "Activation" }
  return `${observed[request.operation]} witnessed after record ${request.seq}.`
}


function uncertainMessage(record: number | undefined, sent: boolean, detail: string): string {
  if (record !== undefined) return `Accepted (record ${record}); runtime unconfirmed: ${detail}`
  return sent ? `Request sent; acceptance and runtime unconfirmed: ${detail}` : detail
}

// The raced read has no publication side effect; only its timely result is consumed.
function beforeDeadline<T>(work: Promise<T>, deadline: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Observation timed out. Refresh to inspect current evidence.")), Math.max(0, deadline - Date.now()))
    work.then(value => { clearTimeout(timer); resolve(value) }, error => { clearTimeout(timer); reject(error) })
  })
}


async function awaitSettlement(request: NavigationRequest, client: QueryClient): Promise<boolean> {
  const deadline = Date.now() + 10_000
  while (Date.now() < deadline) {
    await new Promise(resolve => setTimeout(resolve, 500))
    const current = await beforeDeadline(readNavigationSnapshot(), deadline)
    client.setQueryData(NAVIGATION_STATE_KEY, current)
    if (navigationSettled(current, request)) return true
  }
  return false
}
