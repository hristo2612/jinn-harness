import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import type { ProfileEntryWire } from "@/lib/profile-admin"
import type { PluginCatalogEntryWire } from "@/lib/api"
import { api } from "@/lib/api"
import { useFeatures } from "@/hooks/use-features"
import { useProvidedNavigation } from "@/lib/use-provided-navigation"
import { NAVIGATION_ENTRY, TOOLS_FIRST_SOURCE } from "@/lib/navigation-extension"
import { observeNavigationSource, type SourceObservation, type NavigationSnapshot } from "./navigation-state"
import { useNavigationAdmin } from "./use-navigation-admin"

const buttonClass = "min-h-10 rounded-xl bg-[var(--fill-secondary)] px-3 py-2 text-[var(--text-primary)] disabled:cursor-not-allowed"

function NavigationHistory() {
  const history = useQuery({ queryKey: ["plugin-history", "main", NAVIGATION_ENTRY], queryFn: () => api.pluginHistory("main", NAVIGATION_ENTRY), retry: false })
  if (history.isPending) return <p>Reading retained history…</p>
  if (history.isError) return <p role="alert">{history.error.message}</p>
  return <div className="space-y-2">
    <p>{history.data.qualifier}</p>
    {history.data.lines.length === 0 && <p>No records in the window read.</p>}
    <ol>{history.data.lines.map(line => <li key={line.seq}>Record {line.seq}: {line.kind}</li>)}</ol>
    {history.data.window?.truncated && <p>Older records exist outside this bounded window.</p>}
  </div>
}

export function NavigationExtension() {
  const { data: features } = useFeatures()
  const navigation = useProvidedNavigation(features?.notesEnabled === true)
  const { snapshot, busy, message, act, refresh } = useNavigationAdmin()
  const [historyOpen, setHistoryOpen] = useState(false)
  const document = snapshot.data?.entries.find(entry => entry.id === NAVIGATION_ENTRY)
  const runtime = snapshot.data?.catalog.find(entry => entry.id === NAVIGATION_ENTRY)
  const occupied = !!document && document.package !== "ext/jinn-ext-js-boa"
  return <section aria-label="Navigation customization" className="space-y-4 [overflow-wrap:anywhere] rounded-2xl bg-[var(--fill-quaternary)] p-4 text-[length:var(--text-subheadline)] text-[var(--text-primary)]">
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="font-semibold">Navigation customization</h2><p className="text-[var(--text-secondary)]">Tools first preset</p></div>
      <button className={buttonClass} disabled={busy} onClick={() => void refresh()}>Refresh navigation</button>
    </div>
    <p>The preset keeps available destinations, moves Plugins first and calls it “My tools”. An agent can replace the stored source without rebuilding this workspace.</p>
    <p role="status">{message || installationText(snapshot.status, !!document)}</p>
    {snapshot.isError && <p role="alert">{snapshot.error.message}</p>}
    <RuntimeReading runtime={runtime} />
    <SourceRuntime snapshot={snapshot.data} />
    <p>{navigation.notice ?? navigation.difference}</p>
    <p className="text-[var(--text-secondary)]">Active describes the observed runtime, not successful delivery of this source. A throwing listener may leave navigation unchanged; this API cannot report each listener’s success.</p>
    {occupied && <p role="alert">The ext-navigation ID belongs to {document.package}. Installation is refused; inspect it in the catalog.</p>}
    <NavigationControls document={document} occupied={occupied} busy={busy} ready={snapshot.isSuccess} act={act} historyOpen={historyOpen} onHistory={() => setHistoryOpen(!historyOpen)} />
    <NavigationInspection document={document} runtime={runtime} />
    <p className="text-[var(--text-secondary)]">Disable withdraws the listener and keeps its source/config. Remove deletes the entry. Retained audit records (including prior source/config) and the shared Boa artifact remain. This preset writes no application data or external effects; removal does not undo unrelated past actions.</p>
    {historyOpen && <NavigationHistory />}
  </section>
}

function installationText(status: string, installed: boolean) {
  if (status === "pending") return "Reading installation and runtime…"
  if (status === "error") return "Installation unconfirmed."
  return installed ? "Installed in the profile document." : "Not installed in the profile document."
}

function RuntimeReading({ runtime }: { runtime?: PluginCatalogEntryWire }) {
  if (!runtime) return null
  return <p>Observed runtime: {runtime.lifecycle.state}; incarnation {runtime.incarnation ?? "none"}. {runtime.lifecycle.reason === undefined ? null : JSON.stringify(runtime.lifecycle.reason)}</p>
}

function NavigationInspection({ document, runtime }: { document?: ProfileEntryWire; runtime?: PluginCatalogEntryWire }) {
  return (
    <details className="space-y-3">
      <summary className="min-h-10 cursor-pointer py-2">Inspect source and access</summary>
      <p>Declared origin: {String(document?.config.data?.origin ?? (document ? "not declared" : "agent (preset)"))}</p>
      <p className="break-all">Catalog source digest: {runtime?.attestation?.source ?? "not observed"}</p>
      <SourcePreview document={document} />
      {runtime?.grants ? <>
        <p>Actual grants · source: {runtime.grants.source}</p>
        <pre className="whitespace-pre-wrap break-words">{JSON.stringify(runtime.grants.values, null, 2)}</pre>
        <p>{runtime.grants.qualifier}</p>
      </> : <p>Actual grants have not been observed. Installation requests only the navigation topic and clock.</p>}
      <p>The Boa source receives destination IDs, labels and availability. Its JavaScript has no host calls; the provider reads the clock. It receives no profile contents, credentials or sessions.</p>
    </details>
  )
}

function NavigationControls({ document, occupied, busy, ready, act, historyOpen, onHistory }: {
  document?: ProfileEntryWire
  occupied: boolean
  busy: boolean
  ready: boolean
  act: ReturnType<typeof useNavigationAdmin>["act"]
  historyOpen: boolean
  onHistory: () => void
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {!document && <button className={buttonClass} disabled={busy || !ready} onClick={() => void act("add")}>Add extension</button>}
      {document && !occupied && <>
        <button className={buttonClass} disabled={busy} onClick={() => void act(document.disabled ? "enable" : "disable")}>{document.disabled ? "Enable extension" : "Disable extension"}</button>
        <button className={buttonClass} disabled={busy} onClick={() => void act("remove")}>Remove extension</button>
      </>}
      <button className={buttonClass} aria-expanded={historyOpen} onClick={onHistory}>History</button>
    </div>
  )
}

function SourcePreview({ document }: { document?: ProfileEntryWire }) {
  const source = document?.config.data?.source
  const text = typeof source === "string" ? source : document ? "No source string in this entry." : TOOLS_FIRST_SOURCE
  return <pre className="whitespace-pre-wrap break-words rounded-xl bg-[var(--fill-secondary)] p-3 text-[length:var(--text-caption1)]">{text}</pre>
}

function SourceRuntime({ snapshot }: { snapshot?: NavigationSnapshot }) {
  const [held, setHeld] = useState<{ snapshot: NavigationSnapshot; observation: SourceObservation }>()
  if (!snapshot) return null
  const observation = held?.snapshot === snapshot ? held.observation : observeNavigationSource(held?.observation, snapshot)
  if (held?.snapshot !== snapshot) setHeld({ snapshot, observation })
  return <p className="text-[var(--text-secondary)]">{observation.message}</p>
}
