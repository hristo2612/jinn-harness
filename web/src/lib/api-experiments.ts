import type {
  ExperimentReading,
  ExperimentResponse,
  ExperimentsResponse,
  ExperimentVerdict,
} from "@/routes/experiments/types"

/**
 * The HTTP verbs this surface needs, passed in rather than imported, so the
 * experiments module stays a leaf: `api.ts` depends on it and never the reverse.
 */
export interface ExperimentsHttp {
  get: <T>(path: string) => Promise<T>
  post: <T>(path: string, body?: unknown) => Promise<T>
}

/** The experiments slice of the `api` object, spread back in at its old position. */
export function createExperimentsApi({ get, post }: ExperimentsHttp) {
  return {
    listExperiments: (status?: "running" | "concluded") =>
      get<ExperimentsResponse>(`/api/experiments${status ? `?status=${status}` : ""}`),
    getExperiment: (id: string) =>
      get<ExperimentResponse>(`/api/experiments/${encodeURIComponent(id)}`),
    /** Append one measurement. There is no delete route: a reading is a permanent
     *  point on a series, which is why nothing offers to take one back. */
    recordExperimentReading: (id: string, input: { at: string; metric: string; value: number; note?: string }) =>
      post<{ reading: ExperimentReading }>(`/api/experiments/${encodeURIComponent(id)}/readings`, input),
    concludeExperiment: (id: string, input: { outcome: ExperimentVerdict["outcome"]; note: string }) =>
      post<ExperimentResponse>(`/api/experiments/${encodeURIComponent(id)}/conclude`, input),
  }
}
