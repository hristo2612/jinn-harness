/**
 * The speech-to-text slice of the `api` object, spread back in at its old
 * position. Split out of api.ts, which was at its size baseline.
 *
 * Transcription is the one call here that does not go through the JSON verbs:
 * it posts a raw audio blob and needs its own abort timer, so it takes
 * `authFetch` directly rather than a typed wrapper that would have to be
 * widened to carry a Blob body.
 */

export interface SttHttp {
  get: <T>(path: string) => Promise<T>
  post: <T>(path: string, body?: unknown) => Promise<T>
  put: <T>(path: string, body: unknown) => Promise<T>
  authFetch: (path: string, init?: RequestInit) => Promise<Response>
}

/** Whisper can take a while on a long dictation; five minutes is the ceiling
 *  after which something is wrong rather than slow. */
const TRANSCRIBE_TIMEOUT_MS = 5 * 60_000

export function createSttApi({ get, post, put, authFetch }: SttHttp) {
  return {
    sttStatus: () =>
      get<{ available: boolean; model: string | null; downloading: boolean; progress: number; languages: string[] }>("/api/stt/status"),
    sttDownload: () =>
      post<{ status: string; model: string }>("/api/stt/download", {}),
    sttTranscribe: async (audioBlob: Blob, language?: string): Promise<{ text: string }> => {
      const params = language ? `?language=${encodeURIComponent(language)}` : ""
      const controller = new AbortController()
      const timeout = setTimeout(() => controller.abort(), TRANSCRIBE_TIMEOUT_MS)
      try {
        const res = await authFetch(`/api/stt/transcribe${params}`, {
          method: "POST",
          headers: { "Content-Type": audioBlob.type || "audio/webm" },
          body: audioBlob,
          signal: controller.signal,
        })
        if (!res.ok) throw new Error(`API error: ${res.status}`)
        return res.json()
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") {
          throw new Error("Transcription timed out (5 min)")
        }
        throw err
      } finally {
        clearTimeout(timeout)
      }
    },
    sttUpdateConfig: (languages: string[]) =>
      put<{ status: string; languages: string[] }>("/api/stt/config", { languages }),
  }
}
