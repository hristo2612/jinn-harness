/**
 * The write-side request shape shared by `api.ts` and `api-moments.ts`: the
 * JSON content type and the optional origin label. Its own module so the two
 * requesters can share it without one importing the other.
 *
 * The surface a write came from, when it is not this UI. The gateway allowlists
 * the values and drops anything else, so declaring one is audit colour and never
 * authority — it buys the caller a label on their own write, nothing more.
 */
export type WriteOriginWire = "talk"

export function writeHeaders(origin?: WriteOriginWire): Record<string, string> {
  return origin
    ? { "Content-Type": "application/json", "X-Jinn-Origin": origin }
    : { "Content-Type": "application/json" };
}
