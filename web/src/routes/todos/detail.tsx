import { useRef, useState } from 'react'
import { useTodoJourney } from '@/hooks/use-todo-journey'
import { lawful, reviewable, statusLabel, TASK_ROUTE, todoPrompt, type Inspection, type TodoRecord } from '@/lib/api-todo-journey'
import { formatMessage } from '@/components/chat/message-markdown'
import { CopyText } from '@/components/chat/text-code-block'
import { useTodoDraft } from './drafts'
import { action, caption, input } from './styles'

type Journey = ReturnType<typeof useTodoJourney>
function closed(todo: TodoRecord) { return ['done', 'cancelled'].includes(todo.status) }
function endedSession(journey: Journey) { return Boolean(journey.turn && journey.turn.status !== 'running') }
function stopDisabled(journey: Journey) { return journey.busy || Boolean(journey.pending) }
function hasLiveLink(todo: TodoRecord) { return Boolean(todo.dispatches.at(-1)?.['session-id']) }
function canStart(todo: TodoRecord) { return todo.status === 'executing' || lawful(todo.status, 'executing') }
function canWrite(journey: Journey) { return journey.connected && !journey.busy && !journey.pending }
function Prompt({ todo }: { todo: TodoRecord }) {
  const prompt = todoPrompt(todo)
  return <details className="rounded-2xl bg-[var(--bg-secondary)] p-4"><summary className={`min-h-10 cursor-pointer ${caption}`}>Next task prompt · {new TextEncoder().encode(prompt).length.toLocaleString()} / 32,768 bytes</summary><pre className={`mt-3 whitespace-pre-wrap break-words ${caption}`}>{prompt}</pre></details>
}
function Context({ todo, journey }: { todo: TodoRecord; journey: Journey }) {
  const draft = useTodoDraft(`todo-context:${todo['todo-id']}`, '')
  const field = useRef<HTMLTextAreaElement>(null)
  async function append() { const text = draft.value; if (await journey.mutate('comments', { body: text }, todo.revision)) draft.clearIf(text); field.current?.focus() }
  return <section className="space-y-4" aria-label="Context comments">
    <h2 className="text-base font-semibold">Context</h2>
    {todo.comments.map(comment => <article key={comment['comment-id']} className="space-y-2 rounded-2xl bg-[var(--bg-secondary)] p-4"><p className={caption}>Comment {comment.seq + 1}</p><div className="break-words text-[15px] leading-relaxed">{formatMessage(comment.body)}</div></article>)}
    {!todo.comments.length && <p className={caption}>Add corrections or facts to include in the next task.</p>}
    {!closed(todo) && <form className="space-y-3" onSubmit={event => { event.preventDefault(); void append() }}><label className={`block space-y-2 ${caption}`}><span>Add context</span><textarea ref={field} className={input} rows={3} value={draft.value} onChange={event => draft.save(event.target.value)} /></label><button className={action} disabled={!canWrite(journey) || !draft.value.trim()}>Add context</button>{draft.error && <p role="alert" className={caption}>{draft.error}</p>}</form>}
  </section>
}
function RunTask({ todo, journey }: { todo: TodoRecord; journey: Journey }) {
  const latest = todo.dispatches.at(-1)
  const running = latest?.status === 'running'
  const oversized = new TextEncoder().encode(todoPrompt(todo)).length > 32768
  const allowed = canStart(todo)
  return <section className="space-y-3" aria-label="Run task"><Prompt todo={todo} /><p className={caption}>GPT-6 Astra · High · Text only · Up to 2 minutes. Tools are off. Chat and tasks share one worker; a busy worker refuses a new run.</p>
    {running ? <div className="space-y-3"><p role="status" className={caption}>{endedSession(journey) ? 'The session has ended. Waiting for the Todo’s saved outcome.' : 'Task running. Model completion will still need your review.'}</p><button className={action} disabled={stopDisabled(journey) || !hasLiveLink(todo)} onClick={() => void journey.stop()}>Stop task</button>{!latest['session-id'] && <p className={caption}>The live task link is unavailable. Worker outcome is unconfirmed.</p>}</div>
      : allowed && <button className={action} disabled={!canWrite(journey) || oversized} onClick={() => void journey.mutate('dispatch', { dispatch: TASK_ROUTE }, todo.revision)}>{latest ? 'Run another task' : 'Run text task'}</button>}
    {oversized && <p role="alert" className={caption}>This prompt exceeds 32 KiB. Nothing will run. Capture a smaller Todo; saved context cannot be removed yet.</p>}
  </section>
}
function resultLabel(todo: TodoRecord) {
  if (todo.status === 'done') return 'Accepted result'
  const status = todo.dispatches.at(-1)?.status ?? 'running'
  return { running: 'Live result', done: 'Result complete · awaiting your decision', failed: 'Task failed', cancelled: 'Task stopped', interrupted: 'Task interrupted · outcome incomplete' }[status]
}
function ResultActions({ todo, journey, inspect, answer }: { todo: TodoRecord; journey: Journey; inspect: (value: Inspection) => void; answer: string | undefined }) {
  const latest = todo.dispatches.at(-1)
  return <div className="flex flex-wrap gap-3">{answer && <CopyText text={answer} label="Copy result" />}{latest?.status === 'done' && !closed(todo) && <button className={action} disabled={!canWrite(journey)} onClick={() => inspect({ revision: todo.revision, dispatch: latest['dispatch-id'] })}>I’ve inspected this result</button>}</div>
}
function Result({ todo, journey, inspect }: { todo: TodoRecord; journey: Journey; inspect: (value: Inspection) => void }) {
  const latest = todo.dispatches.at(-1)
  if (!latest) return null
  const answer = latest.status === 'running' ? journey.turn?.answer : latest.answer
  const label = resultLabel(todo)
  return <section className="space-y-4" aria-label="Latest result"><div><h2 className="text-base font-semibold">{label}</h2><p className={caption}>{latest['dispatch-id']}</p></div>
    {answer ? <div className="break-words text-[15px] leading-relaxed">{formatMessage(answer)}</div> : <p className={caption}>No answer text is available.</p>}
    {latest.reason && <p className={caption}>{latest.reason}</p>}
    {latest.status === 'interrupted' && <p className={caption}>The task was interrupted by a restart. Its live session link and partial answer may be missing; nothing is restarted automatically.</p>}
        <ResultActions todo={todo} journey={journey} inspect={inspect} answer={answer} />
  </section>
}
function decisionBody(status: string, note: string, inspected: Inspection | null) {
  return { status, note, ...(['in-review', 'done'].includes(status) ? { 'reviewed-dispatch': inspected?.dispatch } : {}) }
}
function Review({ todo, journey, inspected }: { todo: TodoRecord; journey: Journey; inspected: Inspection | null }) {
  const reason = useTodoDraft(`todo-review:${todo['todo-id']}`, '')
  const ready = reviewable(todo, inspected) && canWrite(journey)
  async function decide(status: string) { const text = reason.value; if (await journey.mutate('status', decisionBody(status, text, inspected), todo.revision)) reason.clearIf(text) }
  if (['done', 'cancelled'].includes(todo.status)) return <p className={caption}>{todo.status === 'done' ? 'You accepted this result and closed the Todo.' : 'This Todo is cancelled. Cancelling its status does not prove a worker stopped.'}</p>
  return <section className="space-y-3" aria-label="Operator review"><h2 className="text-base font-semibold">Your decision</h2><p className={caption}>Inspect the latest successful result before submitting or accepting it. You are reviewing model text; a completed task does not close the Todo.</p>
    <div className="flex flex-wrap gap-3">{lawful(todo.status, 'in-review') && <button className={action} disabled={!ready} onClick={() => void decide('in-review')}>Submit for review</button>}{lawful(todo.status, 'done') && <button className={action} disabled={!ready} onClick={() => void decide('done')}>Accept and close</button>}</div>
    <NeedsWork todo={todo} journey={journey} reason={reason} decide={decide} />
  </section>
}
function NeedsWork({ todo, journey, reason, decide }: { todo: TodoRecord; journey: Journey; reason: ReturnType<typeof useTodoDraft<string>>; decide: (status: string) => Promise<void> }) {
  if (todo.dispatches.at(-1)?.status === 'running') return null
  return <><label className={`block space-y-2 ${caption}`}><span>Needs-work reason</span><textarea className={input} rows={2} value={reason.value} onChange={event => reason.save(event.target.value)} /></label><div className="flex flex-wrap gap-3">{todo.status === 'in-review' && <button className={action} disabled={!canWrite(journey) || !reason.value.trim()} onClick={() => void decide('executing')}>Needs work</button>}{lawful(todo.status, 'blocked') && <button className={action} disabled={!canWrite(journey) || !reason.value.trim()} onClick={() => void decide('blocked')}>Mark blocked</button>}{lawful(todo.status, 'cancelled') && <button className={action} disabled={!canWrite(journey)} onClick={() => void decide('cancelled')}>Cancel Todo</button>}</div><p className={caption}>A needs-work decision records your reason. Another task runs only when you explicitly start it.</p></>
}
function History({ todo }: { todo: TodoRecord }) {
  return <details className="space-y-4"><summary className={`min-h-10 cursor-pointer ${caption}`}>History · {todo.history.length} status changes · {todo.refused.length} refused moves</summary>
    {todo.history.map(change => <div key={change.seq} className={caption}><p>{statusLabel[change.from]} → {statusLabel[change.to]}</p>{change.note && <p className="whitespace-pre-wrap break-words">{change.note}</p>}{change['reviewed-dispatch'] && <p>Inspected result: {change['reviewed-dispatch']}</p>}</div>)}
    {todo.refused.map(change => <p key={`refused-${change.seq}`} className={caption}>Refused: {change.from} → {change.to}</p>)}
    {todo.dispatches.slice(0, -1).map(dispatch => <details key={dispatch['dispatch-id']}><summary className={`min-h-10 cursor-pointer ${caption}`}>{dispatch['dispatch-id']} · {dispatch.status}</summary><div className="break-words text-[15px]">{formatMessage(dispatch.answer || 'No saved answer.')}</div>{dispatch.reason && <p className={caption}>{dispatch.reason}</p>}</details>)}
  </details>
}
export function TodoDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const journey = useTodoJourney(id)
  const [inspected, inspect] = useState<Inspection | null>(null)
  const todo = journey.record
  return <div className="mx-auto w-full max-w-3xl space-y-7 px-4 py-5 text-[var(--text-primary)] sm:px-8">
    <button className={action} onClick={onBack}>Back to Todos</button>
    {journey.error && <div role="alert" className={`rounded-xl bg-[var(--fill-secondary)] p-4 ${caption}`}>{journey.error}<button className={`${action} mt-2`} onClick={journey.retry}>Check again</button></div>}
    {journey.pending && <p role="status" className={caption}>Waiting for a recorded outcome. Further writes are held; this action will not be sent again automatically.</p>}
    {!todo ? <p className={caption}>Loading Todo…</p> : <><header className="space-y-3"><p className={caption}>{statusLabel[todo.status]} · Revision {todo.revision}</p><h1 className="break-words text-2xl font-semibold tracking-tight">{todo.title}</h1>{todo['status-reason'] && <p className={caption}>{todo['status-reason']}</p>}</header>
      {todo.body && <div className="break-words text-[15px] leading-relaxed">{formatMessage(todo.body)}</div>}
      {todo.acceptance && <section className="space-y-2"><h2 className="text-base font-semibold">Acceptance</h2><div className="break-words text-[15px] leading-relaxed">{formatMessage(todo.acceptance)}</div></section>}
      <p className={caption}>{closed(todo) ? "Saved text and history remain available. Capture a new Todo for further work." : "Saved text is fixed for now. Add corrections as context; each new task includes comments in order and recorded decision notes."}</p>
      <Result todo={todo} journey={journey} inspect={inspect} /><Review todo={todo} journey={journey} inspected={inspected} /><Context todo={todo} journey={journey} /><RunTask todo={todo} journey={journey} /><History todo={todo} />
    </>}
  </div>
}
