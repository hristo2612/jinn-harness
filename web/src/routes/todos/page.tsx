import { useCallback, useEffect, useRef, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { PageLayout } from '@/components/page-layout'
import { listTodos, statusLabel, type TodoSummary } from '@/lib/api-todo-journey'
import { cn } from '@/lib/utils'
import { CaptureTodo } from './capture'
import { TodoDetail } from './detail'
import { action, caption } from './styles'

function useTodoList() {
  const [rows, setRows] = useState<TodoSummary[]>([])
  const [error, setError] = useState<string | null>(null)
  const [nonce, setNonce] = useState(0)
  useEffect(() => {
    const controller = new AbortController()
    let timer: ReturnType<typeof setTimeout>
    const poll = async () => {
      try { const next = await listTodos(controller.signal); if (!controller.signal.aborted) { setRows(next); setError(null) } }
      catch (cause) { if (!controller.signal.aborted) setError(cause instanceof Error ? cause.message : 'Todos are unavailable.') }
      if (!controller.signal.aborted) timer = setTimeout(() => void poll(), 3000)
    }
    void poll()
    return () => { controller.abort(); clearTimeout(timer) }
  }, [nonce])
  return { rows, error, retry: useCallback(() => setNonce(value => value + 1), []) }
}
export default function TodosPage() {
  const [params, setParams] = useSearchParams()
  const selected = params.get('todo')
  const creating = params.has('new')
  const detail = Boolean(selected) || creating
  const { rows, error, retry } = useTodoList()
  const lastSelected = useRef<string | null>(null)
  const list = useRef<HTMLElement>(null)
  function back() { lastSelected.current = selected; setParams({}); retry() }
  useEffect(() => { if (!detail && lastSelected.current) list.current?.querySelector<HTMLButtonElement>(`[data-todo="${CSS.escape(lastSelected.current)}"]`)?.focus() }, [detail])
  return <PageLayout hideMobileTabBar={detail}><div className="flex h-full min-h-0">
    <aside className={cn('flex w-full shrink-0 flex-col px-4 py-5 lg:w-[300px] lg:bg-[var(--bg-secondary)]', detail && 'hidden lg:flex')}>
      <div className="flex items-center justify-between gap-3"><h1 className="text-2xl font-bold tracking-tight text-[var(--text-primary)]">Todos</h1><button className={action} onClick={() => setParams({ new: '1' })}>New Todo</button></div>
      <p className={`${caption} mt-3`}>Capture work. Read the result. Decide what’s done.</p>
      {error && <p role="alert" className={`${caption} mt-4`}>{error}<button className={action} onClick={retry}>Check again</button></p>}
      <nav ref={list} aria-label="Saved Todos" className="mt-5 min-h-0 flex-1 space-y-2 overflow-y-auto">
        {!rows.length && <p className={caption}>Your captured work will appear here.</p>}
        {rows.map(todo => <button key={todo['todo-id']} data-todo={todo['todo-id']} aria-current={selected === todo['todo-id'] ? 'page' : undefined} onClick={() => setParams({ todo: todo['todo-id'] })} className={cn('min-h-16 w-full space-y-1 rounded-xl px-3 py-3 text-left text-[var(--text-primary)] hover:bg-[var(--fill-secondary)]', selected === todo['todo-id'] && 'bg-[var(--fill-secondary)]')}><span className="line-clamp-2 block text-sm font-medium">{todo.title}</span><span className="block text-xs text-[var(--text-secondary)]">{statusLabel[todo.status]}</span></button>)}
      </nav>
    </aside>
    <main className={cn('min-w-0 flex-1 overflow-y-auto overscroll-contain pb-8', !detail && 'hidden lg:block')}>
      {selected ? <TodoDetail key={selected} id={selected} onBack={back} /> : <><button className={`${action} m-4 lg:hidden`} onClick={back}>Back to Todos</button><CaptureTodo onCreated={id => { setParams({ todo: id }); retry() }} /></>}
    </main>
  </div></PageLayout>
}
