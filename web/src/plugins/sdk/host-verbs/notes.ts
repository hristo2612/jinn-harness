import { request, write } from './request'

/** A note as the list verb returns it: the row a browser renders, without the
 *  body, which `read` fetches on its own. */
export interface HostNote {
  path: string
  title: string
  /** The first lines of the body, as the gateway trims them. */
  preview: string
  /** The empty string for a note at the top level. */
  folder: string
  updatedAt: string
  /** The version an update must name to win; opaque to a plugin. */
  revision: string
}

/** A note with its Markdown, as `read` returns it. */
export interface HostNoteContent extends HostNote {
  body: string
}

export interface HostNoteDraft {
  title: string
  body: string
  /** A folder under the notes root. Omitted, the note lands at the top level. */
  folder?: string
}

export interface PluginHostNotes {
  /** Every note, or only those matching `query` when one is given. */
  list(query?: string): Promise<HostNote[]>
  read(notePath: string): Promise<HostNoteContent>
  /** Answers the note it wrote, body included — the gateway re-reads the file
   *  after writing it, and narrowing that away would hide what it returned. */
  create(draft: HostNoteDraft): Promise<HostNoteContent>
}

export const notes: PluginHostNotes = {
  async list(query) {
    const suffix = query ? `?q=${encodeURIComponent(query)}` : ''
    // The route answers `{ notes, folders }`. The folder tree is a browser's
    // navigation aid, not something a plugin asked for.
    const page = await request<{ notes: HostNote[] }>('notes.list', `/api/notes${suffix}`)
    return page.notes
  },
  async read(notePath) {
    const found = await request<{ note: HostNoteContent }>(
      'notes.read',
      `/api/notes/read?path=${encodeURIComponent(notePath)}`,
    )
    return found.note
  },
  async create(draft) {
    const created = await write<{ note: HostNoteContent }>('notes.create', '/api/notes', draft)
    return created.note
  },
}
