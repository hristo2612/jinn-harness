import { memo, useLayoutEffect, useRef, useState } from 'react'
import { ArrowDown } from 'lucide-react'
import type { ChatTurn } from '@/lib/api-chat'
import { captureVisibleAnchor, restoreVisibleAnchor, type ScrollAnchor } from '@/lib/scroll-anchor'
import { distanceFromBottom, followAfterScroll, STICK_THRESHOLD_PX } from '@/hooks/stick-geometry'
import { formatMessage } from './message-markdown'
import { CopyText } from './text-code-block'

// Primitive props keep settled Markdown bodies out of the token-render path.
const MessageBody = memo(function MessageBody({ text, user = false }: { text: string; user?: boolean }) {
  return <div className="min-w-0 break-words text-[15px] leading-relaxed text-[var(--text-primary)]">{formatMessage(text, { tightLines: user })}</div>
})

function statusText(turn: ChatTurn): string {
  if (turn.status === 'running') return turn.reason || (turn.answer ? 'Replying…' : 'Connecting…')
  if (turn.status === 'done') return ''
  if (turn.status === 'cancelled') return 'Stopped. This partial reply is saved.'
  if (turn.status === 'interrupted') return 'Interrupted by a restart. This reply is incomplete.'
  return 'Reply failed. Any text received is saved.'
}
function TurnView({ turn }: { turn: ChatTurn }) {
  return <article data-turn={turn.seq} className="space-y-5">
          <div className="ml-auto w-fit max-w-[92%] rounded-2xl bg-[var(--fill-secondary)] px-4 py-3"><MessageBody text={turn.message} user /></div>
          <div className="space-y-3">
            <div className="text-xs font-medium text-[var(--text-secondary)]">GPT-6 Astra</div>
            {turn.answer ? <MessageBody text={turn.answer} /> : <p className="text-sm text-[var(--text-secondary)]">{turn.status === 'running' ? 'Waiting for a reply…' : 'No reply text was saved.'}</p>}
            <div className="flex flex-wrap items-center gap-3">
              {turn.status !== 'running' && turn.answer && <CopyText text={turn.answer} label="Copy reply" />}
              <span role={turn.status === 'running' ? 'status' : undefined} className="text-xs text-[var(--text-secondary)]">
                {statusText(turn)}
              </span>
            </div>
            {turn.status !== 'running' && turn.reason && <p className="break-words text-sm text-[var(--text-secondary)]">{turn.reason}</p>}
          </div>
        </article>
}

function useTranscriptPosition(turns: ChatTurn[]) {
  const node = useRef<HTMLDivElement>(null)
  const following = useRef(true)
  const previousGap = useRef(0)
  const writtenTop = useRef<number | null>(null)
  const anchor = useRef<ScrollAnchor | null>(null)
  const [shown, setShown] = useState(20)
  const [detached, setDetached] = useState(false)
  const start = Math.max(0, turns.length - shown)
  const loadEarlier = () => {
    if (node.current) anchor.current = captureVisibleAnchor(node.current, 'data-turn')
    following.current = false
    setShown(value => value + 20)
  }
  const jump = () => {
    if (!node.current) return
    following.current = true
    node.current.scrollTop = node.current.scrollHeight
    writtenTop.current = node.current.scrollTop
    previousGap.current = distanceFromBottom(node.current)
    setDetached(false)
  }
  useLayoutEffect(() => {
    const el = node.current
    if (!el) return
    if (anchor.current) {
      restoreVisibleAnchor(el, anchor.current)
      anchor.current = null
      writtenTop.current = el.scrollTop
      previousGap.current = distanceFromBottom(el)
    } else if (following.current) jump()
    else setDetached(distanceFromBottom(el) > STICK_THRESHOLD_PX)
  }, [turns, shown])
  const onScroll = () => {
        const el = node.current
        if (!el) return
        const gap = distanceFromBottom(el)
        if (writtenTop.current !== null && Math.abs(el.scrollTop - writtenTop.current) < 1) writtenTop.current = null
        else following.current = followAfterScroll(gap, previousGap.current, following.current, STICK_THRESHOLD_PX)
        previousGap.current = gap
        setDetached(!following.current && gap > STICK_THRESHOLD_PX)
  }
  return { node, start, detached, jump, loadEarlier, onScroll }
}

export function TextTranscript({ turns }: { turns: ChatTurn[] }) {
  const { node, start, detached, jump, loadEarlier, onScroll } = useTranscriptPosition(turns)
  return <div className="relative min-h-0 flex-1">
    <div ref={node} data-testid="chat-transcript" className="h-full overflow-y-auto overscroll-contain px-4 py-6 [overflow-anchor:none] sm:px-8"
      onScroll={onScroll}>
      <div className="mx-auto max-w-[760px] space-y-8">
        {start > 0 && <button className="min-h-10 rounded-xl bg-[var(--fill-secondary)] px-4 text-sm text-[var(--text-secondary)]" onClick={loadEarlier}>Load earlier messages · {start} earlier</button>}
        {turns.slice(start).map(turn => <TurnView key={turn.seq} turn={turn} />)}
      </div>
    </div>
    {detached && <button aria-label="Jump to latest message" onClick={jump} className="absolute bottom-4 left-1/2 flex min-h-10 -translate-x-1/2 items-center gap-2 rounded-full bg-[var(--bg-secondary)] px-4 text-sm text-[var(--text-primary)] shadow-lg"><ArrowDown size={16} />Latest</button>}
  </div>
}
