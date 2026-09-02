import { expect, vi } from 'vitest'

/**
 * The stubbed `fetch` both host-verb suites drive.
 *
 * Counting calls is as much the point as reading them: a verb that fanned out
 * into two requests, or that retried, would spend a plugin's authority twice for
 * one call.
 */
export function stubFetch(): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn()
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

export function respond(body: unknown, init: { ok?: boolean; status?: number } = {}): Response {
  return {
    ok: init.ok ?? true,
    status: init.status ?? 200,
    statusText: '',
    json: async () => body,
  } as unknown as Response
}

/** The single request the verb made, with its URL made relative again — the
 *  app prefixes a base the plugin never sees. */
export function requestOf(fetchMock: ReturnType<typeof vi.fn>): {
  path: string
  init: RequestInit
} {
  expect(fetchMock).toHaveBeenCalledTimes(1)
  const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
  return { path: url.replace(/^https?:\/\/[^/]+/, ''), init }
}
