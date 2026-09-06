import React, { useState } from 'react'
import { ChevronDown } from 'lucide-react'
import { CodeBlock, parseFenceLang } from '@/components/chat/text-code-block'

// The code-fence parser remains shared with the text code block.
export { parseFenceLang } from '@/components/chat/text-code-block'

// Text-only subset of the source formatter: safe links, emphasis and code.
// File viewers and Todo references have no mounted counterpart in text Chat.
const INLINE_RE_SOURCE =
  String.raw`\[([^\]]+)\]\(([^)]+)\)` +                 // [text](url)
  String.raw`|(https?:\/\/[^\s<]+[^\s<.,;:!?)}\]'"])` + // bare URL
  String.raw`|(\*\*(.+?)\*\*)` +                        // **bold**
  '|(`([^`\r\n]+)`)' +                                  // `inline code`
  String.raw`|\*([^*]+)\*`                              // *italic*

function InlineCode({ children }: { children: string }) {
  return (
    <code className="bg-[var(--fill-secondary)] rounded-[5px] py-px px-[5px] text-[0.88em] font-[family-name:var(--font-code)] text-[var(--text-primary)]">
      {children}
    </code>
  )
}

function safeMarkdownHref(href: string): string | null {
  const trimmed = href.trim()
  return /^(https?:\/\/|mailto:)/i.test(trimmed) ? trimmed : null
}

function inlineMatch(match: RegExpExecArray): React.ReactNode {
  if (match[1] && match[2]) {
      // Markdown link: [text](url)
      const href = safeMarkdownHref(match[2])
      return (href
        ? (
          <a
            key={match.index}
            href={href}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--system-blue)] underline underline-offset-2"
          >
            {match[1]}
          </a>
        )
        : match[1])
    } else if (match[3]) {
      // Bare URL
      return (
        <a
          key={match.index}
          href={match[3]}
          target="_blank"
          rel="noopener noreferrer"
          className="text-[var(--system-blue)] underline underline-offset-2"
        >
          {match[3]}
        </a>
      )
    } else if (match[4]) {
      return (<strong key={match.index} className="font-[var(--weight-bold)]">{inlineFormat(match[5])}</strong>)
    } else if (match[6]) {
      return (<InlineCode key={match.index}>{match[7]}</InlineCode>)
    } else if (match[8]) {
      return (<em key={match.index} className="italic opacity-[0.85]">{inlineFormat(match[8])}</em>)
    }

  return null
}

function inlineFormat(text: string): React.ReactNode {
  const parts: React.ReactNode[] = []
  // Fresh regex per call (own lastIndex — inlineFormat recurses for table cells).
  const regex = new RegExp(INLINE_RE_SOURCE, 'g')
  let last = 0
  let match

  while ((match = regex.exec(text)) !== null) {
    if (match.index > last) parts.push(text.slice(last, match.index))
    parts.push(inlineMatch(match))
    last = match.index + match[0].length
  }
  if (last < text.length) parts.push(text.slice(last))
  return parts.length === 1 ? parts[0] : <>{parts}</>
}

// A trace remains an explicit disclosure, including while its text is incomplete.

// Some engines leak their scratchpad tags into the visible answer. Rendering the
// raw `<analysis>` line is worse than useless, and dropping the content would lose
// real text — so fold it into a collapsed disclosure instead.
// `summary` is here because a leaked compaction message is `<analysis>…</analysis>`
// followed by `<summary>…</summary>` — folding only the first half still dumps the
// whole recap into the chat.
export const TRACE_TAGS = new Set(['analysis', 'thinking', 'reasoning', 'reflection', 'scratchpad', 'summary'])
export const TRACE_OPEN_RE = /^<([a-z_]+)>$/i
const TRACE_CLOSE_RE = /^<\/([a-z_]+)>$/i

function TraceBlock({ tag, body }: { tag: string; body: string }) {
  const [open, setOpen] = useState(false)
  const label = tag.charAt(0).toUpperCase() + tag.slice(1)
  if (!body.trim()) return null
  return (
    <div className="my-[var(--space-2)]">
      <button
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="inline-flex items-center gap-[var(--space-1)] rounded-[var(--radius-sm)] border-none bg-transparent min-h-9 px-2 py-1 text-[length:var(--text-caption1)] text-[var(--text-secondary)] transition-colors hover:text-[var(--text-secondary)] cursor-pointer"
      >
        <ChevronDown size={12} className={`transition-transform ${open ? '' : '-rotate-90'}`} />
        {label}
      </button>
      {open && (
        <div className="mt-[var(--space-1)] pl-[var(--space-3)] text-[var(--text-secondary)]">
          {formatMessage(body)}
        </div>
      )}
    </div>
  )
}

function isTableSeparator(line: string): boolean {
  return /^\|[\s:|-]+\|$/.test(line.trim())
}

function parseTableRow(line: string): string[] {
  return line.trim().replace(/^\|/, '').replace(/\|$/, '').split('|').map(c => c.trim())
}

function TableBlock({ headerLine, rows, keyProp }: { headerLine: string; rows: string[]; keyProp: number }) {
  const headers = parseTableRow(headerLine)
  const bodyRows = rows.map(parseTableRow)

  return (
    <div key={keyProp} className="my-[var(--space-3)] rounded-[var(--radius-md)] overflow-hidden shadow-[var(--shadow-subtle)]">
      <div className="overflow-x-auto [WebkitOverflowScrolling:touch]">
        <table className="border-collapse text-[length:var(--text-footnote)] leading-[1.6] w-full min-w-max">
          <thead>
            <tr className="bg-[var(--fill-tertiary)]">
              {headers.map((h, hi) => (
                <th key={hi} className="text-left py-2.5 px-4 font-semibold text-[var(--text-primary)] max-w-[280px] break-words">{inlineFormat(h)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {bodyRows.map((row, ri) => (
              <tr key={ri} className={ri % 2 === 1 ? 'bg-[var(--fill-quaternary)]' : 'bg-transparent'}>
                {row.map((cell, ci) => (
                  <td key={ci} className="py-2.5 px-4 text-[var(--text-primary)] max-w-[280px] break-words">{inlineFormat(cell)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

type Block = { node: React.ReactNode; next: number }
function codeAt(lines: string[], start: number): Block {
  let end = start + 1
  while (end < lines.length && !lines[end].startsWith('```')) end++
  const code = lines.slice(start + 1, end).join('\n')
  return { node: <CodeBlock key={`code-${start}`} keyProp={start} code={code} lang={parseFenceLang(lines[start])} />, next: end + 1 }
}
function traceAt(lines: string[], start: number, tag: string): Block {
  let end = start + 1
  while (end < lines.length && lines[end].trim().toLowerCase() !== `</${tag}>`) end++
  return { node: <TraceBlock key={`trace-${start}`} tag={tag} body={lines.slice(start + 1, end).join('\n')} />, next: end + 1 }
}
function tableLine(line: string | undefined): boolean {
  return Boolean(line?.trim().startsWith('|') && line?.trim().endsWith('|'))
}
function tableAt(lines: string[], start: number): Block {
  let end = start + 2
  while (tableLine(lines[end]) && !isTableSeparator(lines[end])) end++
  return { node: <TableBlock key={`table-${start}`} keyProp={start} headerLine={lines[start]} rows={lines.slice(start + 2, end)} />, next: end }
}
function blockAt(lines: string[], start: number): Block | null {
  const line = lines[start]
  if (line.startsWith('```')) return codeAt(lines, start)
  const trace = line.trim().match(TRACE_OPEN_RE)
  if (trace && TRACE_TAGS.has(trace[1].toLowerCase())) return traceAt(lines, start, trace[1].toLowerCase())
  if (tableLine(line) && start + 1 < lines.length && isTableSeparator(lines[start + 1])) return tableAt(lines, start)
  return null
}
function plainLine(line: string, index: number, tight: boolean): React.ReactNode {
  const close = line.trim().match(TRACE_CLOSE_RE)
  if (close && TRACE_TAGS.has(close[1].toLowerCase())) return null
  if (!line.trim()) return <div key={`space-${index}`} className={tight ? 'h-2' : 'h-1.5'} />
  const bullet = line.match(/^[-*] /)
  const numbered = line.match(/^(\d+)\. /)
  if (bullet || numbered) return <div key={index} className="mb-1 flex gap-[var(--space-2)]"><span className="min-w-4 shrink-0 text-[var(--text-secondary)]">{numbered ? `${numbered[1]}.` : '•'}</span><span>{inlineFormat(line.replace(/^([-*] |\d+\. )/, ''))}</span></div>
  const heading = line.match(/^(#{1,3}) /)
  if (heading) return <div key={index} className="mb-[var(--space-2)] mt-[var(--space-4)] font-semibold" style={{ fontSize: [20, 18, 16][heading[1].length - 1] }}>{inlineFormat(line.slice(heading[0].length))}</div>
  return <div key={index} className={tight ? undefined : 'mb-[var(--space-2)] last:mb-0'}>{inlineFormat(line)}</div>
}

/** User single newlines are line breaks; assistant paragraphs retain source rhythm. */
export function formatMessage(content: string, opts?: { tightLines?: boolean }): React.ReactNode {
  if (!content) return null
  const lines = content.split('\n')
  const result: React.ReactNode[] = []
  let index = 0
  while (index < lines.length) {
    const block = blockAt(lines, index)
    if (block) { result.push(block.node); index = block.next; continue }
    result.push(plainLine(lines[index], index, opts?.tightLines === true))
    index++
  }
  return <>{result}</>
}
