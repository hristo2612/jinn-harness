import { Fragment, type ReactNode } from "react"

/**
 * `reason.snippet` is gateway-made text with the hits wrapped in `<mark>`, and
 * everything around them is whatever an operator typed into a Todo. So it is
 * split into nodes rather than handed to innerHTML: a mark tag becomes an
 * element, and any other markup stays exactly what it is — text.
 */
export function MatchSnippet({ snippet }: { snippet: string }) {
  const nodes: ReactNode[] = []
  let marked = false
  snippet.split(/(<mark>|<\/mark>)/).forEach((part, index) => {
    if (part === "<mark>") { marked = true; return }
    if (part === "</mark>") { marked = false; return }
    if (!part) return
    nodes.push(marked
      ? <mark key={index} className="rounded-[3px] bg-[var(--accent-fill)] px-[2px] text-[var(--accent)]">{part}</mark>
      : <Fragment key={index}>{part}</Fragment>)
  })
  return <>{nodes}</>
}
