import { useState } from 'react'

export function parseFenceLang(line: string): string { return line.slice(3).trim().split(/\s+/)[0] ?? '' }
export function CopyText({ text, label = 'Copy' }: { text: string; label?: string }) {
  const [state, setState] = useState(label)
  return <button type="button" className="min-h-9 rounded-lg bg-[var(--fill-secondary)] px-3 text-[length:var(--text-caption1)] text-[var(--text-secondary)]"
    onClick={() => { if (!navigator.clipboard) { setState('Copy unavailable'); return }; void navigator.clipboard.writeText(text).then(() => setState('Copied'), () => setState('Copy unavailable')) }}>{state}</button>
}
export function CodeBlock({ code, lang }: { code: string; lang: string; keyProp: number }) {
  return <div className="my-3 overflow-hidden rounded-xl bg-[var(--bg-secondary)]">
    <div className="flex items-center justify-between gap-2 px-3 py-2"><span className="text-[length:var(--text-caption1)] text-[var(--text-secondary)]">{lang || 'Code'}</span><CopyText text={code} /></div>
    <pre className="overflow-x-auto px-4 pb-4 text-[length:var(--text-footnote)]"><code>{code}</code></pre>
  </div>
}
