import { request } from './request'

export interface HostKnowledgeResult {
  path: string
  title: string
  /** A window around the match, with the matched token wrapped in «». */
  snippet: string
  /** Occurrences across the path and the content, which is the sort key. */
  matchCount: number
}

export interface PluginHostKnowledge {
  search(query: string): Promise<HostKnowledgeResult[]>
}

export const knowledge: PluginHostKnowledge = {
  async search(query) {
    const found = await request<{ results: HostKnowledgeResult[] }>(
      'knowledge.search',
      `/api/knowledge/search?q=${encodeURIComponent(query)}`,
    )
    return found.results
  },
}
