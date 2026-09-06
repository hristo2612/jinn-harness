import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { ArrowLeft, ArrowUp, MessageSquare, Plus, Square } from 'lucide-react'
import { PageLayout } from '@/components/page-layout'
import { TextTranscript } from '@/components/chat/text-transcript'
import { useChatSession } from '@/hooks/use-chat-session'
import { createChat, listChats, type ChatSummary } from '@/lib/api-chat'
import { cn } from '@/lib/utils'

const action = 'min-h-10 rounded-xl px-3 text-sm font-medium text-[var(--text-primary)] transition-colors hover:bg-[var(--fill-secondary)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]'

type Chat = ReturnType<typeof useChatSession>
function conversationTitle(chat: Chat) { return chat.record?.log[0]?.message.slice(0, 70) || 'New conversation' }
function visibleTurns(chat: Chat) {
  const saved = chat.record?.log ?? []
  if (chat.pending && !saved.some(turn => turn.seq === chat.pending?.seq)) return [...saved, chat.pending]
  return saved
}
function cannotSend(chat: Chat) {
  return chat.busy || !chat.record || chat.record.status === 'closed' || Boolean(chat.pending) || chat.record.log.some(turn => turn.status === 'running')
}
function composerHint(chat: Chat) {
  if (chat.feed === 'recovering') return 'Reconnecting · your text is kept'
  if (chat.record?.status === 'closed') return 'This conversation is closed'
  return 'Enter to send · Shift + Enter for a new line'
}
function useChatDraft(chat: Chat) {
  const [draft, setDraft] = useState('')
  const composing = useRef(false)
  const submitting = useRef(false)
  const input = useRef<HTMLTextAreaElement>(null)
  const disabled = cannotSend(chat)
  async function submit() {
    if (disabled || submitting.current || !draft.trim()) return
    submitting.current = true
    const text = draft.trim()
    try { if (await chat.send(text)) setDraft(value => value.trim() === text ? '' : value) }
    finally { submitting.current = false; input.current?.focus() }
  }
  return { draft, setDraft, composing, input, disabled, submit }
}
function ChatError({ chat, onNew }: { chat: Chat; onNew: () => void }) {
  if (!chat.error) return null
  return <div role="alert" className="mb-3 rounded-xl bg-[var(--fill-secondary)] p-3 text-sm text-[var(--text-primary)]">{chat.error}<button className={cn(action, 'ml-1')} onClick={chat.retry}>Check again</button>{chat.error.includes('context limit') && <button className={action} onClick={onNew}>New chat</button>}</div>
}
function ChatComposer({ chat, onNew }: { chat: Chat; onNew: () => void }) {
  const { draft, setDraft, composing, input, disabled, submit } = useChatDraft(chat)
  const running = visibleTurns(chat).some(turn => turn.status === 'running')
  return <div className="shrink-0 px-4 pb-[max(12px,var(--safe-bottom))] pt-3 sm:px-8" style={{ paddingBottom: 'max(12px, var(--safe-bottom), var(--keyboard-inset))' }}>
      <div className="mx-auto max-w-[760px]">
        <ChatError chat={chat} onNew={onNew} />
        <form onSubmit={event => { event.preventDefault(); void submit() }} className="rounded-2xl bg-[var(--fill-secondary)] p-3">
          <textarea ref={input} aria-label="Message" placeholder="Message GPT-6 Astra" value={draft} rows={2} className="max-h-40 min-h-14 w-full resize-y bg-transparent px-1 text-[16px] leading-6 text-[var(--text-primary)] outline-none placeholder:text-[var(--text-secondary)]"
            onChange={event => setDraft(event.target.value)} onCompositionStart={() => { composing.current = true }} onCompositionEnd={() => { composing.current = false }}
            onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing && event.nativeEvent.keyCode !== 229 && !composing.current) { event.preventDefault(); void submit() } }} />
          <div className="flex items-center justify-between gap-3"><span className="text-xs text-[var(--text-secondary)]">{composerHint(chat)}</span>
            {running ? <button type="button" disabled={chat.busy} onClick={() => void chat.stop()} className={cn(action, 'flex shrink-0 items-center gap-2 bg-[var(--bg-secondary)]')}><Square size={14} />{chat.busy ? 'Requesting…' : 'Stop'}</button>
              : <button type="submit" aria-label="Send message" disabled={disabled || !draft.trim()} className="flex min-h-10 min-w-10 shrink-0 items-center justify-center rounded-xl bg-[var(--text-primary)] text-[var(--bg)] disabled:bg-[var(--bg-secondary)] disabled:text-[var(--text-secondary)]"><ArrowUp size={20} /></button>}
          </div>
        </form>
      </div>
    </div>
}
function Conversation({ id, onBack, onNew }: { id: string; onBack: () => void; onNew: () => void }) {
  const chat = useChatSession(id)
  const turns = visibleTurns(chat)
  return <section className="flex h-full min-w-0 flex-1 flex-col" aria-label="Conversation">
    <header className="flex shrink-0 items-center gap-2 px-3 py-3 sm:px-6">
      <button aria-label="Back to chats" onClick={onBack} className={cn(action, 'lg:hidden')}><ArrowLeft size={20} /></button>
      <div className="min-w-0 flex-1"><h1 className="text-base font-semibold text-[var(--text-primary)]">{conversationTitle(chat)}</h1><p className="mt-1 text-xs text-[var(--text-secondary)]">GPT-6 Astra · High · Text only</p></div>
      <Link to="/settings/plugins" className={action}>Plugins</Link>
    </header>
    {turns.length ? <TextTranscript turns={turns} /> : <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 text-center"><MessageSquare size={30} className="text-[var(--text-secondary)]" /><h2 className="text-xl font-semibold text-[var(--text-primary)]">What’s on your mind?</h2><p className="max-w-sm text-sm leading-relaxed text-[var(--text-secondary)]">Write, reason, or work through an idea. Completed messages stay in context. Tools and attachments are off.</p></div>}
    <ChatComposer chat={chat} onNew={onNew} />
  </section>
}

export default function ChatPage() {
  const [params, setParams] = useSearchParams()
  const selected = params.get('chat')
  const [chats, setChats] = useState<ChatSummary[]>([])
  const [error, setError] = useState<string | null>(null)
  const [ready, setReady] = useState(false)
  const [creating, setCreating] = useState(false)
  const creatingRef = useRef(false)
  const load = useCallback(async (signal?: AbortSignal) => {
    try { const saved = await listChats(signal); if (signal?.aborted) return; setChats(saved); setReady(true); setError(null) }
    catch (cause) { if (!signal?.aborted) setError(cause instanceof Error ? cause.message : 'Saved chats could not be loaded.') }
  }, [])
  useEffect(() => {
    const controller = new AbortController()
    void load(controller.signal)
    const timer = setInterval(() => { if (document.visibilityState !== 'hidden') void load(controller.signal) }, 5000)
    return () => { controller.abort(); clearInterval(timer) }
  }, [load])
  async function newChat() {
    if (creatingRef.current) return
    creatingRef.current = true; setCreating(true)
    const request = crypto.randomUUID()
    try { const id = await createChat(request); setParams({ chat: id }); await load() }
    catch (cause) {
      try { const saved = await listChats(); const found = saved.find(row => row.metadata?.['client-request'] === request); if (found) { setChats(saved); setParams({ chat: found['session-id'] }); return } } catch { /* The original ambiguity stays visible. */ }
      setError(cause instanceof Error ? cause.message : 'Conversation creation was not confirmed. Check saved chats before trying again.')
    } finally { creatingRef.current = false; setCreating(false) }
  }
  return <PageLayout hideMobileTabBar={Boolean(selected)}><div className="flex h-full min-h-0">
    <aside aria-label="Saved chats" className={cn('flex w-full shrink-0 flex-col px-4 py-5 lg:w-[280px] lg:bg-[var(--bg-secondary)]', selected && 'hidden lg:flex')}>
      <div className="flex items-center justify-between"><h1 className="text-2xl font-bold tracking-tight text-[var(--text-primary)]">Chat</h1><button aria-label="New chat" disabled={!ready || creating} className={action} onClick={() => void newChat()}><Plus size={22} /></button></div>
      <p className="mt-2 text-sm text-[var(--text-secondary)]">A place to think things through.</p>
      {error && <div role="alert" className="mt-4 text-sm text-[var(--text-secondary)]">{error}<button className={action} onClick={() => void load()}>Retry</button></div>}
      <nav className="mt-6 min-h-0 flex-1 space-y-2 overflow-y-auto" aria-label="Conversations">
        {!chats.length && <p className="px-2 text-sm text-[var(--text-secondary)]">{ready ? 'Your conversations will appear here.' : 'Loading saved chats…'}</p>}
        {chats.map(chat => <button key={chat['session-id']} aria-current={selected === chat['session-id'] ? 'page' : undefined} onClick={() => setParams({ chat: chat['session-id'] })} className={cn('min-h-14 w-full rounded-xl px-3 py-3 text-left hover:bg-[var(--fill-secondary)]', selected === chat['session-id'] && 'bg-[var(--fill-secondary)]')}><span className="line-clamp-2 text-sm font-medium text-[var(--text-primary)]">{chat.title || 'New conversation'}</span>{chat.status === 'running' && <span className="mt-1 block text-xs text-[var(--text-secondary)]">Reply in progress</span>}</button>)}
      </nav>
    </aside>
    {selected ? <Conversation key={selected} id={selected} onBack={() => setParams({})} onNew={() => void newChat()} /> : <div className="hidden flex-1 flex-col items-center justify-center gap-4 px-8 text-center lg:flex"><MessageSquare size={36} className="text-[var(--text-secondary)]" /><h2 className="text-2xl font-semibold text-[var(--text-primary)]">Start with a thought</h2><p className="max-w-sm text-sm leading-relaxed text-[var(--text-secondary)]">Ask a question, shape a draft, or explore an idea with GPT-6 Astra.</p><button disabled={!ready || creating} onClick={() => void newChat()} className={cn(action, 'bg-[var(--fill-secondary)]')}>{creating ? 'Creating…' : 'New conversation'}</button></div>}
  </div></PageLayout>
}
