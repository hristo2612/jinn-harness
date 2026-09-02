import { NewTodoDialog } from "@/routes/todos/new-todo-dialog"
import { useDepartments } from "@/hooks/use-departments"
import { useOrg } from "@/routes/todos/use-todos"

/** The `new` verb's second half: the board's own create dialog, seeded with the
 *  words that followed the verb. It is a component rather than a branch in the
 *  palette so the roster and the department list are asked for only once a
 *  create is actually under way. */
export function QuickCreateTodo({ title, onDone }: { title: string; onDone: () => void }) {
  const org = useOrg()
  const departments = useDepartments()
  return (
    <NewTodoDialog
      onClose={onDone}
      onCreated={onDone}
      defaults={{
        title,
        employees: org.data?.employees ?? [],
        departments: departments.data ?? [],
      }}
    />
  )
}
