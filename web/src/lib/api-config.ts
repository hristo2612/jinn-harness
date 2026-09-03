import { authFetch } from "@/lib/auth"

/**
 * UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 1): config.yaml is gone.
 * The document this page edits is synthesised from the settings seam —
 * `GET /v1/settings` names the namespaces, `GET /v1/settings/{ns}` resolves
 * each, and a save is one `PATCH /v1/settings/{ns}` per namespace that
 * changed against the last-read document. There is no revision header; the
 * revision is the resolved map itself, spelled out.
 *
 * §8 amendment 4: the page renders only what the namespace schema declares,
 * and a save carries only declared keys. An undeclared key is dropped before
 * the wire rather than sent for the daemon to refuse with 422.
 *
 * UI-2 (§9.2 item 13): a save is a MOMENT first. Each namespace's patch goes
 * through `POST /v1/moments/ui/before-patch-settings` as `{ namespace, patch }`
 * BEFORE its `PATCH /v1/settings/{ns}`, and the PATCH carries the FOLDED patch
 * — what the daemon's extensions made of it. A refused walk (a typed 503, the
 * extension mid-restart) is surfaced as the page's conflict notice reading the
 * refusal; this adapter does not retry on its own (the retry-once belongs with
 * the composer, UI-6).
 */

/** One property of a namespace schema, as `GET /v1/settings/{ns}` declares it.
 *  `kind` is the wire's kebab-case name: `bool`, `integer`, `number`, `string`,
 *  `array`, `object` or `secret-ref`. */
export interface DeclaredProperty {
  kind: string
  required: boolean
}

/** What one namespace declares: its properties, and whether a key outside them
 *  is accepted (`additional`). A namespace that declared nothing accepts nothing. */
export interface DeclaredNamespace {
  properties: Record<string, DeclaredProperty>
  additional: boolean
}

/** The settings seam's `Resolved` answer, the part this adapter reads. An absent
 *  `schema` means nothing is declared. */
interface ResolvedNamespaceWire {
  namespace: string
  settings: Record<string, unknown>
  schema?: { properties?: Record<string, { kind?: string; required?: boolean }>; additional?: boolean }
}

/** The settings seam's `Namespaces` answer, the part this adapter reads. */
interface NamespacesWire {
  namespaces: Record<string, unknown>
}

/** The settings seam's error envelope: `{ error: { code, detail, shadowed? } }`. */
interface SettingsErrorEnvelopeWire {
  error?: { code?: string; detail?: string; retryable?: boolean }
}

/** The resolved settings, by namespace, and which document it was. */
export interface ConfigDocument {
  config: Record<string, unknown>
  /** Opaque. Hand it back to `updateConfig` and a save that would overwrite
   *  somebody else's hand edit is refused instead of landing. */
  revision: string
  /** The schema each namespace declared: what the page may render, and what a
   *  save may carry. */
  declared: Record<string, DeclaredNamespace>
}

/** What a save leaves behind: the revision the document now has, so the page that
 *  just wrote it is current again rather than stale against its own write, and
 *  the document AS THE DAEMON ANSWERED IT — a moment may have folded the patch
 *  (§9.7 amendment 8(d)), so the page shows this, never the draft it sent. */
export interface ConfigSaveResult {
  revision: string
  config: Record<string, unknown>
}

/**
 * Turning a failed response into the thrown error, passed in rather than
 * imported so this module stays a leaf: `api.ts` depends on it, never the reverse.
 * `conflict` builds the error the conflict notice reads (`code: "CONFIG_CONFLICT"`)
 * out of the seam's typed `refused` / `invalid` answer.
 */
export interface ConfigHttp {
  responseError: (res: Response) => Promise<Error>
  conflict: (status: number, message: string, remedy?: string) => Error
  /** The one requester of `POST /v1/moments/<domain>/<topic>` (`api.ts`). */
  moment: (domain: string, topic: string, payload: unknown) => Promise<Response>
}

/** The moment's domain and topic this adapter dispatches (inventory §4.3 moment 19). */
export const PATCH_SETTINGS_MOMENT = { domain: "ui", topic: "before-patch-settings" } as const

/** What the moment answers: the payload as the daemon's extensions folded it. */
interface PatchSettingsMomentWire {
  namespace?: string
  patch?: Record<string, unknown>
}

/** The document the last `getConfig` read: what the next save is diffed against. */
let lastRead: Record<string, Record<string, unknown>> = {}
/** The schemas the last `getConfig` read: what the next save is filtered by. */
let lastDeclared: Record<string, DeclaredNamespace> = {}

function revisionOf(document: Record<string, unknown>): string {
  return JSON.stringify(document)
}

function settingsOf(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {}
}

function declaredOf(schema: ResolvedNamespaceWire["schema"]): DeclaredNamespace {
  const properties: Record<string, DeclaredProperty> = {}
  for (const [key, property] of Object.entries(schema?.properties ?? {})) {
    properties[key] = { kind: property?.kind ?? "object", required: property?.required === true }
  }
  return { properties, additional: schema?.additional === true }
}

/** RFC 7396 merge patch turning `previous` into `next`, one level deep: a key the
 *  next document dropped is a `null`, a key whose value moved is its new value. */
function namespacePatch(previous: Record<string, unknown>, next: Record<string, unknown>): Record<string, unknown> {
  const patch: Record<string, unknown> = {}
  for (const key of Object.keys(previous)) if (!(key in next)) patch[key] = null
  for (const [key, value] of Object.entries(next)) {
    if (JSON.stringify(previous[key]) !== JSON.stringify(value)) patch[key] = value
  }
  return patch
}

/** The part of a patch the seam will take. A key the namespace does not declare
 *  is dropped rather than sent (unless the namespace accepts `additional` keys),
 *  and a `secret-ref` key is never written from this page at all. */
function declaredPatch(patch: Record<string, unknown>, declared: DeclaredNamespace | undefined): Record<string, unknown> {
  const kept: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(patch)) {
    const property = declared?.properties[key]
    if (property?.kind === "secret-ref") continue
    if (property === undefined && !(declared?.additional ?? false)) continue
    kept[key] = value
  }
  return kept
}

/** The seam's typed `refused` / `invalid` answer is the conflict the notice
 *  reads; any other failure is the ordinary error. */
async function refusal(res: Response, { responseError, conflict }: ConfigHttp): Promise<Error> {
  const body = (await res.clone().json().catch(() => null)) as SettingsErrorEnvelopeWire | null
  const error = body?.error
  if (!error || (error.code !== "refused" && error.code !== "invalid")) return responseError(res)
  const remedy = error.retryable ? "The refusal is retryable: reload and try again." : undefined
  return conflict(res.status, error.detail ?? `the settings seam answered ${error.code}`, remedy)
}

/** A refused walk is the seam's typed `unavailable` naming the refusal (`restarting`,
 *  `gone`, `suspended`, `stalled`, `cycle`) — the conflict the notice reads, with the
 *  refusal's own word as the remedy's cue; any other failure is the ordinary error. */
async function momentRefusal(res: Response, { responseError, conflict }: ConfigHttp): Promise<Error> {
  const body = (await res.clone().json().catch(() => null)) as SettingsErrorEnvelopeWire | null
  const error = body?.error
  if (!error || error.code !== "unavailable") return responseError(res)
  return conflict(
    res.status,
    error.detail ?? "the moment's walk was refused",
    "An extension refused the moment whole (it may be restarting). Nothing was saved; try the save again.",
  )
}

/** The moment, then the patch: what the PATCH carries is what the extensions folded. */
async function foldPatch(http: ConfigHttp, namespace: string, patch: Record<string, unknown>): Promise<Record<string, unknown>> {
  const res = await http.moment(PATCH_SETTINGS_MOMENT.domain, PATCH_SETTINGS_MOMENT.topic, { namespace, patch })
  if (!res.ok) throw await momentRefusal(res, http)
  const folded = (await res.json()) as PatchSettingsMomentWire
  return settingsOf(folded.patch)
}

/** The config slice of the `api` object, spread back in at its old position. */
export function createConfigApi(http: ConfigHttp) {
  async function readNamespace(namespace: string): Promise<ResolvedNamespaceWire> {
    const res = await authFetch(`/v1/settings/${encodeURIComponent(namespace)}`)
    if (!res.ok) throw await http.responseError(res)
    return (await res.json()) as ResolvedNamespaceWire
  }

  async function patchNamespace(namespace: string, unfolded: Record<string, unknown>): Promise<void> {
    const patch = await foldPatch(http, namespace, unfolded)
    const res = await authFetch(`/v1/settings/${encodeURIComponent(namespace)}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ patch }),
    })
    if (!res.ok) throw await refusal(res, http)
    lastRead[namespace] = settingsOf(((await res.json()) as ResolvedNamespaceWire).settings)
  }

  return {
    getConfig: async (): Promise<ConfigDocument> => {
      const res = await authFetch("/v1/settings")
      if (!res.ok) throw await http.responseError(res)
      const names = Object.keys(((await res.json()) as NamespacesWire).namespaces ?? {})
      const resolved = await Promise.all(names.map(async (name) => [name, await readNamespace(name)] as const))
      lastRead = Object.fromEntries(resolved.map(([name, wire]) => [name, settingsOf(wire.settings)]))
      lastDeclared = Object.fromEntries(resolved.map(([name, wire]) => [name, declaredOf(wire.schema)]))
      return { config: { ...lastRead }, revision: revisionOf(lastRead), declared: { ...lastDeclared } }
    },
    /** `revision` is the one `getConfig()` handed over; a save built on an older
     *  read is refused as a conflict rather than landed on top of it. */
    updateConfig: async (data: Record<string, unknown>, revision?: string): Promise<ConfigSaveResult> => {
      if (revision && revision !== revisionOf(lastRead)) {
        throw http.conflict(409, "The settings changed under this page since it last read them.", "Reload to pick up the current document.")
      }
      for (const namespace of new Set([...Object.keys(lastRead), ...Object.keys(data)])) {
        const patch = namespacePatch(lastRead[namespace] ?? {}, settingsOf(data[namespace]))
        const declared = declaredPatch(patch, lastDeclared[namespace])
        if (Object.keys(declared).length > 0) await patchNamespace(namespace, declared)
      }
      return { revision: revisionOf(lastRead), config: { ...lastRead } }
    },
  }
}
