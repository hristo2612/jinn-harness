import { authFetch } from "@/lib/auth"
import { writeHeaders } from "@/lib/api-write"

/**
 * UI-2 (docs/plans/ui-malleability-arc.md §9.2 item 13): a MOMENT — one
 * `POST /v1/moments/<domain>/<topic>` with the moment's payload, answered with
 * the payload as the daemon's extensions folded it. `momentResponse` is the one
 * requester every surface's moment goes through: `api.moment` parses its answer
 * here, and the settings adapter (`api-config.ts`) classifies its refusal — a
 * refused walk is a typed 503 naming the refusal, never the unmodified payload.
 */
export function momentResponse(domain: string, topic: string, payload: unknown): Promise<Response> {
  return authFetch(`/v1/moments/${encodeURIComponent(domain)}/${encodeURIComponent(topic)}`, {
    method: "POST",
    headers: writeHeaders(),
    body: JSON.stringify(payload),
  })
}

/** The moment slice of the `api` object. */
export function createMomentApi({ responseError }: { responseError: (res: Response) => Promise<Error> }) {
  return {
    /** The folded payload of one moment, or the seam's error. */
    moment: async <T extends object>(domain: string, topic: string, payload: T): Promise<T> => {
      const res = await momentResponse(domain, topic, payload)
      if (!res.ok) throw await responseError(res)
      return (await res.json()) as T
    },
  }
}
