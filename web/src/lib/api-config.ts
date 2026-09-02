import { authFetch } from "@/lib/auth"

/**
 * UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 1): config.yaml is gone.
 * The document this page edits is synthesised from the settings seam —
 * `GET /v1/settings` names the namespaces, `GET /v1/settings/{ns}` resolves
 * each, and a save is one `PATCH /v1/settings/{ns}` per namespace that
 * changed against the last-read document. There is no revision header; the
 * revision is the resolved map itself, spelled out.
 */

/** The settings seam's `Resolved` answer, the part this adapter reads. */
interface ResolvedNamespaceWire {
  namespace: string
  settings: Record<string, unknown>
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
}

/** What a save leaves behind: the revision the document now has, so the page that
 *  just wrote it is current again rather than stale against its own write. */
export interface ConfigSaveResult {
  revision: string
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
}

/** The document the last `getConfig` read: what the next save is diffed against. */
let lastRead: Record<string, Record<string, unknown>> = {}

function revisionOf(document: Record<string, unknown>): string {
  return JSON.stringify(document)
}

function settingsOf(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {}
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

/** The config slice of the `api` object, spread back in at its old position. */
export function createConfigApi({ responseError, conflict }: ConfigHttp) {
  async function readNamespace(namespace: string): Promise<Record<string, unknown>> {
    const res = await authFetch(`/v1/settings/${encodeURIComponent(namespace)}`)
    if (!res.ok) throw await responseError(res)
    return settingsOf(((await res.json()) as ResolvedNamespaceWire).settings)
  }

  /** The seam's typed `refused` / `invalid` answer is the conflict the notice
   *  reads; any other failure is the ordinary error. */
  async function refusal(res: Response): Promise<Error> {
    const body = (await res.clone().json().catch(() => null)) as SettingsErrorEnvelopeWire | null
    const error = body?.error
    if (!error || (error.code !== "refused" && error.code !== "invalid")) return responseError(res)
    const remedy = error.retryable ? "The refusal is retryable: reload and try again." : undefined
    return conflict(res.status, error.detail ?? `the settings seam answered ${error.code}`, remedy)
  }

  async function patchNamespace(namespace: string, patch: Record<string, unknown>): Promise<void> {
    const res = await authFetch(`/v1/settings/${encodeURIComponent(namespace)}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ patch }),
    })
    if (!res.ok) throw await refusal(res)
    lastRead[namespace] = settingsOf(((await res.json()) as ResolvedNamespaceWire).settings)
  }

  return {
    getConfig: async (): Promise<ConfigDocument> => {
      const res = await authFetch("/v1/settings")
      if (!res.ok) throw await responseError(res)
      const names = Object.keys(((await res.json()) as NamespacesWire).namespaces ?? {})
      const resolved = await Promise.all(names.map(async (name) => [name, await readNamespace(name)] as const))
      lastRead = Object.fromEntries(resolved)
      return { config: { ...lastRead }, revision: revisionOf(lastRead) }
    },
    /** `revision` is the one `getConfig()` handed over; a save built on an older
     *  read is refused as a conflict rather than landed on top of it. */
    updateConfig: async (data: Record<string, unknown>, revision?: string): Promise<ConfigSaveResult> => {
      if (revision && revision !== revisionOf(lastRead)) {
        throw conflict(409, "The settings changed under this page since it last read them.", "Reload to pick up the current document.")
      }
      for (const namespace of new Set([...Object.keys(lastRead), ...Object.keys(data)])) {
        const patch = namespacePatch(lastRead[namespace] ?? {}, settingsOf(data[namespace]))
        if (Object.keys(patch).length > 0) await patchNamespace(namespace, patch)
      }
      return { revision: revisionOf(lastRead) }
    },
  }
}
