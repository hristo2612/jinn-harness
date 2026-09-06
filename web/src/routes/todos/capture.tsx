import { useRef, useState } from 'react'
import { createTodo, listTodos, readTodo, TodoHttpError, type TodoDraft } from '@/lib/api-todo-journey'
import { useTodoDraft } from './drafts'
import { action, caption, input } from './styles'

const empty: TodoDraft = { title: '', body: '', acceptance: '' }
const pendingKey = 'todo-capture-pending'
function pendingCapture() { try { return sessionStorage.getItem(pendingKey) } catch { return null } }
const message = (cause: unknown) => cause instanceof Error ? cause.message : 'Capture was not confirmed.'
function useCapture(onCreated: (id: string) => void) {
  const draft = useTodoDraft('todo-capture-draft', empty)
  const [pending, setPending] = useState(pendingCapture)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const lock = useRef(false)
  function complete(id: string, captured: TodoDraft) { sessionStorage.removeItem(pendingKey); setPending(null); draft.clearIf(captured); onCreated(id) }
  async function reconcile() {
    if (!pending || lock.current) return
    lock.current = true; setBusy(true)
    try {
      for (const row of await listTodos()) {
        const saved = await readTodo(row['todo-id'])
        if (saved.metadata?.['client-request'] === pending) { complete(saved['todo-id'], { title: saved.title, body: saved.body, acceptance: saved.acceptance }); return }
      }
      setError('Capture is still unconfirmed. No matching saved Todo was found. Your draft is kept; nothing will be sent again automatically.')
    } catch (cause) { setError(message(cause)) }
    finally { lock.current = false; setBusy(false) }
  }
  async function submit() {
    if (lock.current || pending || !draft.value.title.trim()) return
    lock.current = true; setBusy(true); setError(null)
    const client = crypto.randomUUID()
    const captured = draft.value
    try {
      sessionStorage.setItem(pendingKey, client); setPending(client)
      complete(await createTodo(captured, client), captured)
    } catch (cause) {
      if (cause instanceof TodoHttpError && cause.rejected) { sessionStorage.removeItem(pendingKey); setPending(null) }
      setError(message(cause))
    } finally { lock.current = false; setBusy(false) }
  }
  return { draft, pending, busy, error, submit, reconcile }
}
export function CaptureTodo({ onCreated }: { onCreated: (id: string) => void }) {
  const capture = useCapture(onCreated)
  const { draft } = capture
  const title = useRef<HTMLInputElement>(null)
  return <section aria-label="Capture work" className="mx-auto w-full max-w-2xl space-y-6 px-4 py-6 sm:px-8">
    <div><h1 className="text-2xl font-semibold text-[var(--text-primary)]">What needs doing?</h1><p className={`${caption} mt-2`}>Capture the outcome, add context, and run a bounded text task when you’re ready.</p></div>
    <form className="space-y-5" onSubmit={event => { event.preventDefault(); void capture.submit() }}>
      <label className={`block space-y-2 ${caption}`}><span>Title</span><input ref={title} autoFocus value={draft.value.title} onChange={event => draft.save({ ...draft.value, title: event.target.value })} className={input} placeholder="Draft a project update" onKeyDown={event => { if (event.key === 'Enter' && event.nativeEvent.isComposing) event.preventDefault() }} /></label>
      <label className={`block space-y-2 ${caption}`}><span>Context</span><textarea rows={4} value={draft.value.body} onChange={event => draft.save({ ...draft.value, body: event.target.value })} className={input} placeholder="What should the task know?" /></label>
      <label className={`block space-y-2 ${caption}`}><span>Acceptance</span><textarea rows={3} value={draft.value.acceptance} onChange={event => draft.save({ ...draft.value, acceptance: event.target.value })} className={input} placeholder="What would a useful result include?" /></label>
      <p className={caption}>Saved text cannot be edited yet. Add later corrections as context comments. Your draft stays editable until capture.</p>
      {(capture.error || draft.error) && <p role="alert" className={caption}>{capture.error || draft.error}</p>}
      {capture.pending ? <div className="space-y-3"><p role="status" className={caption}>Capture is awaiting confirmation. Your text is kept.</p><button type="button" className={action} disabled={capture.busy} onClick={() => void capture.reconcile()}>Check saved Todos</button></div>
        : <button className={action} disabled={capture.busy || !draft.value.title.trim()}>{capture.busy ? 'Capturing…' : 'Capture Todo'}</button>}
    </form>
  </section>
}
