import { request, write } from './request'

/** A Workflow, as both `list` and `get` return it. The two agree on purpose:
 *  `GET /api/workflows/:id` also carries the node graph, which is the Workflow
 *  engine's own vocabulary and not something this contract can spell without
 *  pulling the app's internals into it. */
export interface HostWorkflow {
  id: string
  title: string
  description: string | null
  revision: number
  enabled: boolean
  updatedAt: string
}

/** A run of a Workflow, as `start` returns it once the row exists. */
export interface HostWorkflowRun {
  id: string
  workflowId: string
  status: string
  startedAt: string
}

export interface PluginHostWorkflows {
  /** One page of Workflows, at the gateway's own default size. A verb that
   *  followed the cursor would spend a plugin's authority once per page. */
  list(): Promise<HostWorkflow[]>
  get(workflowId: string): Promise<HostWorkflow>
  /** Start a manual run. `input` is the Workflow's own declared input. */
  start(workflowId: string, input?: Record<string, unknown>): Promise<HostWorkflowRun>
}

/** The summary and the full definition differ only in how they spell an absent
 *  description, so one narrowing serves both routes. */
interface WorkflowWire {
  id: string
  title: string
  description?: string | null
  revision: number
  enabled: boolean
  updatedAt: string
}

function asWorkflow(wire: WorkflowWire): HostWorkflow {
  return {
    id: wire.id,
    title: wire.title,
    description: wire.description ?? null,
    revision: wire.revision,
    enabled: wire.enabled,
    updatedAt: wire.updatedAt,
  }
}

export const workflows: PluginHostWorkflows = {
  async list() {
    const page = await request<{ items: WorkflowWire[] }>('workflows.list', '/api/workflows')
    return page.items.map(asWorkflow)
  },
  async get(workflowId) {
    const definition = await request<WorkflowWire>(
      'workflows.get',
      `/api/workflows/${encodeURIComponent(workflowId)}`,
    )
    return asWorkflow(definition)
  },
  async start(workflowId, input = {}) {
    const run = await write<HostWorkflowRun>(
      'workflows.start',
      `/api/workflows/${encodeURIComponent(workflowId)}/runs`,
      { input },
    )
    // The route answers the whole run, definition graph included. A plugin gets
    // the four fields the contract names and nothing it did not ask for.
    return { id: run.id, workflowId: run.workflowId, status: run.status, startedAt: run.startedAt }
  },
}
