import { useMutation, useQueryClient, type QueryClient } from "@tanstack/react-query"
import { api } from "@/lib/api"

/* The one lane that posts a comment on a Todo. The task page's activity feed
 * and the search overlay's workbench both send through it, so a comment written
 * in either place carries its staged files the same way and moves the same
 * caches on the way out. */

export interface AddTodoCommentArgs {
  body: string
  /** Set when the comment is a reply; the workbench composer never threads. */
  parentCommentId?: string
  /** Staged uploads, attached once the comment they belong to exists. */
  files?: File[]
}

/** Everything a comment write moves: the windowed comment page, the attachment
 *  list it may have grown, the detail that carries the tail, and the lists whose
 *  rows count comments. */
export function invalidateTodoComments(queryClient: QueryClient, id: string): void {
  void queryClient.invalidateQueries({ queryKey: ["work-item-comments", id] })
  void queryClient.invalidateQueries({ queryKey: ["work-item-attachments", id] })
  void queryClient.invalidateQueries({ queryKey: ["work-item", id] })
  void queryClient.invalidateQueries({ queryKey: ["work-items"] })
}

export function useAddTodoComment(id: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ body, parentCommentId, files }: AddTodoCommentArgs) => {
      const { comment } = await api.addWorkItemComment(id, body, parentCommentId)
      for (const file of files ?? []) {
        await api.uploadWorkItemAttachment(id, file, comment.id)
      }
      return comment
    },
    onSettled: () => invalidateTodoComments(queryClient, id),
  })
}
