import { authFetch } from "@/lib/auth"
import { ApiError } from "@/lib/api"

/**
 * The composition's shape through the operator API (pin `f8b285b`, jinnd
 * M2-K23 `jinn:profile-admin`; FINDINGS #37 closed by pin-bump 10). Each call
 * is ONE admin write on the transport: `POST /v1/profile/entries`,
 * `DELETE /v1/profile/entries/{id}`, and the three `PATCH` shapes
 * (`{disabled}`, `{grants}`, `{package, hash}`). The answer names the
 * `ProfileAdministered` row's sequence — the intent, landed before the
 * commit; the restart, spawn or disposal it schedules is followed on the
 * ledger. A refusal is the kernel's, typed: `refused` with its class
 * (`unauthorized` | `malformed` | `conflict` | `irreversible`) in the detail,
 * surfaced here verbatim as the error's message.
 *
 * A separate module from `api.ts` on purpose: that file is at its ratchet
 * budget, and this surface is one seam's.
 */

export interface AdministeredWire {
  "api-version": string
  id: string
  write: string
  "administered-seq": number
}

/** The 0.2.0 `entry` record `add-entry` takes: `grants` beside `config`. */
export interface EntryRecordWire {
  id: string
  package: string
  version?: string
  hash: string
  grants: unknown[]
  config: Record<string, unknown>
  disabled?: boolean
  parent?: string | null
}

const ENTRIES = "/v1/profile/entries"

async function refusal(res: Response): Promise<ApiError> {
  let message = `API error: ${res.status}`
  let code: string | undefined
  try {
    const body = await res.json()
    if (body.error && typeof body.error === "object") {
      if (typeof body.error.detail === "string") message = body.error.detail
      if (typeof body.error.code === "string") code = body.error.code
    }
  } catch {
    // Not JSON; the status stays the discriminator.
  }
  return new ApiError(res.status, message, code)
}

async function write(method: "POST" | "DELETE" | "PATCH", path: string, body?: unknown): Promise<AdministeredWire> {
  const res = await authFetch(path, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!res.ok) throw await refusal(res)
  return res.json()
}

export const profileAdmin = {
  /** `add-entry`: a new entry, its grants beside its config. */
  addEntry: (record: EntryRecordWire) => write("POST", ENTRIES, record),
  /** `remove-entry`: a leaf entry, withdrawn on the record. */
  removeEntry: (id: string) => write("DELETE", `${ENTRIES}/${encodeURIComponent(id)}`),
  /** `set-disabled`: `true` disposes, `false` spawns a fresh incarnation. */
  setDisabled: (id: string, disabled: boolean) => write("PATCH", `${ENTRIES}/${encodeURIComponent(id)}`, { disabled }),
  /** `set-grants`: the whole list, applied through the entry's restart. */
  setGrants: (id: string, grants: unknown[]) => write("PATCH", `${ENTRIES}/${encodeURIComponent(id)}`, { grants }),
  /** `swap-plugin`: the entry's pin; the old incarnation is disposed, a
   *  successor spawned under the same id (the stated 0.1.0 window). */
  swapPlugin: (id: string, pkg: string, hash: string) =>
    write("PATCH", `${ENTRIES}/${encodeURIComponent(id)}`, { package: pkg, hash }),
}

/** A scoped read of the document; config is kept separate from runtime state. */
export interface ProfileEntryWire {
  id: string
  package: string
  hash: string
  disabled?: boolean
  config: { data?: Record<string, unknown>; grants?: unknown[] }
}

export async function readProfile(): Promise<{ profile: { entries: ProfileEntryWire[] } }> {
  const res = await authFetch("/v1/profile")
  if (!res.ok) throw await refusal(res)
  return res.json()
}
