import { useQuery } from '@tanstack/react-query'
import { api, type WorkItemOpenDetailWire } from '@/lib/api'

/* Mention previews come from the compact list route the board already uses. A
 * transcript can render dozens of mentions in one pass, so ids are collected
 * across that pass and asked for together instead of one request per mention.
 * Freshness is event-driven, not timed: a resolved preview stands until the
 * gateway says the Todo changed (use-query-invalidation). */

/** The route rejects a query carrying more than 100 ids (gateway api.ts). */
const MAX_IDS_PER_REQUEST = 100

/** Bounds the cache for a long-lived tab; freshness is the event stream's job,
 *  so this only caps memory, and the oldest mention is the coldest. */
const MAX_CACHED_PREVIEWS = 500

export type TodoPreview = WorkItemOpenDetailWire

interface Waiter {
  resolve: (preview: TodoPreview | null) => void
  reject: (error: unknown) => void
}

const previews = new Map<string, Promise<TodoPreview | null>>()
const queued = new Map<string, Waiter>()
let flushScheduled = false

/** Resolves the compact row behind a Todo id, or `null` when the gateway has no
 *  such Todo to show. Repeat calls for an id already asked for share the first
 *  call's result rather than opening a second request. */
export function requestTodoPreview(id: string): Promise<TodoPreview | null> {
  const cached = previews.get(id)
  if (cached) return cached

  const preview = new Promise<TodoPreview | null>((resolve, reject) => {
    queued.set(id, { resolve, reject })
  })
  remember(id, preview)

  if (!flushScheduled) {
    flushScheduled = true
    queueMicrotask(flush)
  }
  return preview
}

/** Drops one Todo's cached preview so the next mention of it asks again. */
export function forgetTodoPreview(id: string): void {
  previews.delete(id)
}

/** The reading view of the cache above, for a surface that shows a preview
 *  rather than only warming it. The key is the one use-query-invalidation
 *  already drops on a live work-item event, so an open preview re-reads itself
 *  when the gateway says that Todo changed and ignores every other Todo. */
export function useTodoPreview(id: string) {
  return useQuery({
    queryKey: ['work-item-preview', id],
    queryFn: () => requestTodoPreview(id),
  })
}

function remember(id: string, preview: Promise<TodoPreview | null>): void {
  previews.set(id, preview)
  while (previews.size > MAX_CACHED_PREVIEWS) {
    const oldest = previews.keys().next()
    if (oldest.done) return
    previews.delete(oldest.value)
  }
}

function flush(): void {
  flushScheduled = false
  const batch = new Map(queued)
  queued.clear()

  const ids = [...batch.keys()]
  for (let start = 0; start < ids.length; start += MAX_IDS_PER_REQUEST) {
    void fetchBatch(ids.slice(start, start + MAX_IDS_PER_REQUEST), batch)
  }
}

async function fetchBatch(ids: string[], batch: Map<string, Waiter>): Promise<void> {
  try {
    const { workItems } = await api.getWorkItems(ids)
    const found = new Map(workItems.map((item) => [item.workItem.id, item]))
    // The route answers with only the Todos it could resolve. An id it leaves
    // out is deleted or invisible to this viewer, and caching that as `null`
    // is what stops a dangling mention asking again on every render.
    for (const id of ids) batch.get(id)?.resolve(found.get(id) ?? null)
  } catch (error) {
    // Forget the failed ids so the next mention can retry: caching a rejection
    // would make one dropped request poison that Todo for the whole session.
    for (const id of ids) {
      previews.delete(id)
      batch.get(id)?.reject(error)
    }
  }
}
