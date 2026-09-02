import type { WorkflowDefinitionWire } from "@/lib/api"

/**
 * The one write helper this surface needs, passed in rather than imported, so the
 * lifecycle module stays a leaf: `api.ts` depends on it and never the reverse.
 */
export interface WorkflowLifecycleHttp {
  workflowWrite: <T>(path: string, method: "POST" | "PUT", body: unknown) => Promise<T>
}

/** Everything that changes a Workflow's standing rather than its graph, spread
 *  back into the `api` object at its old position. Each write is guarded by the
 *  revision the caller read, so a stale one comes back as a 409. */
export function createWorkflowLifecycleApi({ workflowWrite }: WorkflowLifecycleHttp) {
  const workflow = (id: string) => `/api/workflows/${encodeURIComponent(id)}`
  return {
    setWorkflowEnabledV2: (id: string, enabled: boolean, expectedRevision: number) =>
      workflowWrite<WorkflowDefinitionWire>(`${workflow(id)}/${enabled ? "enable" : "disable"}`, "POST", { expectedRevision }),
    /** Archive and unarchive. Unarchiving always returns the Workflow disabled —
     *  the pre-archive state is not stored, and coming back live would re-arm its
     *  triggers without anyone asking. */
    setWorkflowRetiredV2: (id: string, retired: boolean, expectedRevision: number) =>
      workflowWrite<WorkflowDefinitionWire>(`${workflow(id)}/${retired ? "retire" : "unretire"}`, "POST", { expectedRevision }),
    /** Copies the graph under a new ID, disabled, at revision 1. The copy carries
     *  no history and no retirement, so a duplicate of an archived Workflow is live. */
    duplicateWorkflowV2: (sourceId: string, input: { id: string; title: string }) =>
      workflowWrite<WorkflowDefinitionWire>(`${workflow(sourceId)}/duplicate`, "POST", input),
  }
}
