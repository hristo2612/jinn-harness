/**
 * The Todo approval gate's wire shapes.
 *
 * Moved out of `api.ts` unchanged so the gate's own fields could be declared on
 * the full row without the file growing: the gate is a self-contained lane — one
 * row shape and the state it is in — and `api.ts` re-exports both, so no caller
 * had to move with them. The state moved along with the row rather than staying
 * behind: a module that had to import it back would close a cycle, and a
 * circular type resolves to `any`, which is how a whole surface silently loses
 * its types.
 */

export type ApprovalStateWire = "pending" | "approved" | "rejected"

/** One approval history row (Todos v2 slice 4). The legacy approval* fields on
 *  the work item mirror the CURRENT row (pending, else latest decided). */
export interface WorkItemApprovalWire {
  id: string
  workItemId: string
  state: ApprovalStateWire
  request: string
  ref: string | null
  /** Offered variants when this gate asks for a PICK, not a plain yes/no. */
  options: string[] | null
  /** The picked option, once approved. */
  choice: string | null
  target: string | null
  targetKind: string | null
  requestedBy: string
  requestedAt: string
  escalatedAt: string | null
  decidedBy: string | null
  decidedAt: string | null
  note: string | null
}
