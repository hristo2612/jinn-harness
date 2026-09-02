import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { host } from '../host'
import { requestOf, respond, stubFetch } from './host-verbs-harness'

/** The instance verbs: Workflows, notes, connectors, cron reads, knowledge
 *  search. Each one names its method, its path spelling, and what it unwraps. */
let fetchMock: ReturnType<typeof vi.fn>

function onlyRequest() {
  return requestOf(fetchMock)
}

beforeEach(() => {
  fetchMock = stubFetch()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('host.workflows', () => {
  const wire = {
    id: 'nightly',
    title: 'Nightly digest',
    revision: 3,
    enabled: true,
    updatedAt: '2026-01-02T00:00:00.000Z',
  }
  const narrowed = { ...wire, description: null }

  it('lists over GET /api/workflows and unwraps the page', async () => {
    fetchMock.mockResolvedValue(respond({ items: [wire], nextCursor: null }))

    await expect(host.workflows.list()).resolves.toEqual([narrowed])

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/workflows')
    expect(init.method ?? 'GET').toBe('GET')
  })

  /* The route answers the whole definition, node graph included. A plugin gets
   * the six fields the contract names and nothing it did not ask for. */
  it('gets one over GET /api/workflows/:id and narrows the definition', async () => {
    fetchMock.mockResolvedValue(respond({ ...wire, nodes: [{ id: 'n1' }], edges: [] }))

    await expect(host.workflows.get('nightly')).resolves.toEqual(narrowed)

    expect(onlyRequest().path).toBe('/api/workflows/nightly')
  })

  it('starts a run over POST /api/workflows/:id/runs and keeps only the run', async () => {
    fetchMock.mockResolvedValue(
      respond({
        id: 'run-1',
        workflowId: 'nightly',
        status: 'running',
        startedAt: '2026-01-03T00:00:00.000Z',
        definition: wire,
      }),
    )

    await expect(host.workflows.start('nightly', { since: 'yesterday' })).resolves.toEqual({
      id: 'run-1',
      workflowId: 'nightly',
      status: 'running',
      startedAt: '2026-01-03T00:00:00.000Z',
    })

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/workflows/nightly/runs')
    expect(init.method).toBe('POST')
    expect(JSON.parse(String(init.body))).toEqual({ input: { since: 'yesterday' } })
  })

  /* The Workflow API says `{ code, message }` where the rest of the gateway says
   * `{ error }`, so reading only the latter would lose every Workflow reason. */
  it('raises the Workflow API’s own message, in its own envelope', async () => {
    fetchMock.mockResolvedValue(
      respond(
        { code: 'bad-input', message: 'Workflow does not have an enabled manual trigger.' },
        { ok: false, status: 422 },
      ),
    )

    await expect(host.workflows.start('nightly')).rejects.toThrow(
      /host\.workflows\.start failed: Workflow does not have an enabled manual trigger\./,
    )
  })
})

describe('host.notes', () => {
  const note = {
    path: 'knowledge/a.md',
    title: 'A',
    preview: 'a body',
    folder: '',
    updatedAt: '2026-01-01T00:00:00.000Z',
    revision: 'f'.repeat(64),
  }

  it('lists over GET /api/notes and drops the folder tree', async () => {
    fetchMock.mockResolvedValue(respond({ notes: [note], folders: [{ path: 'x', name: 'x', count: 1 }] }))

    await expect(host.notes.list()).resolves.toEqual([note])

    expect(onlyRequest().path).toBe('/api/notes')
  })

  it('sends a query as ?q= when one was given', async () => {
    fetchMock.mockResolvedValue(respond({ notes: [] }))

    await host.notes.list('two words')

    expect(onlyRequest().path).toBe('/api/notes?q=two%20words')
  })

  it('reads over GET /api/notes/read?path= and unwraps the note', async () => {
    fetchMock.mockResolvedValue(respond({ note: { ...note, body: 'a body' } }))

    await expect(host.notes.read('knowledge/a.md')).resolves.toMatchObject({ body: 'a body' })

    expect(onlyRequest().path).toBe('/api/notes/read?path=knowledge%2Fa.md')
  })

  it('creates over POST /api/notes and unwraps the written note', async () => {
    fetchMock.mockResolvedValue(respond({ note: { ...note, body: 'a body' } }))

    await expect(host.notes.create({ title: 'A', body: 'a body' })).resolves.toMatchObject({
      path: 'knowledge/a.md',
      body: 'a body',
    })

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/notes')
    expect(init.method).toBe('POST')
    expect(JSON.parse(String(init.body))).toEqual({ title: 'A', body: 'a body' })
  })
})

describe('host.connectors.send', () => {
  it('posts to /api/connectors/:name/send and resolves with nothing to read', async () => {
    fetchMock.mockResolvedValue(respond({ status: 'sent' }))

    await expect(
      host.connectors.send('slack', { channel: 'C1', text: 'hello', thread: '17.1' }),
    ).resolves.toBeUndefined()

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/connectors/slack/send')
    expect(init.method).toBe('POST')
    expect(JSON.parse(String(init.body))).toEqual({ channel: 'C1', text: 'hello', thread: '17.1' })
  })
})

describe('host.cron', () => {
  it('lists over GET /api/cron and keeps only the read tier', async () => {
    fetchMock.mockResolvedValue(
      respond([
        {
          id: 'digest',
          name: 'Daily digest',
          schedule: '0 9 * * *',
          enabled: true,
          employee: 'a-lead',
          engine: null,
          timezone: 'UTC',
          lastRun: { id: 'run-9', status: 'success' },
        },
      ]),
    )

    await expect(host.cron.jobs()).resolves.toEqual([
      {
        id: 'digest',
        name: 'Daily digest',
        schedule: '0 9 * * *',
        enabled: true,
        employee: 'a-lead',
        engine: null,
        timezone: 'UTC',
      },
    ])

    expect(onlyRequest().path).toBe('/api/cron')
  })

  it('reads run history over GET /api/cron/:id/runs?limit=', async () => {
    const run = { id: 'run-9', jobId: 'digest', status: 'success' }
    fetchMock.mockResolvedValue(respond([run]))

    await expect(host.cron.runs('digest')).resolves.toEqual([run])

    expect(onlyRequest().path).toBe('/api/cron/digest/runs?limit=20')
  })

  it('passes a limit the caller set', async () => {
    fetchMock.mockResolvedValue(respond([]))

    await host.cron.runs('digest', 5)

    expect(onlyRequest().path).toBe('/api/cron/digest/runs?limit=5')
  })
})

describe('host.knowledge.search', () => {
  it('reads GET /api/knowledge/search?q= and unwraps the hits', async () => {
    const hit = { path: 'knowledge/birds.md', title: 'Birds', snippet: '«kestrel»', matchCount: 1 }
    fetchMock.mockResolvedValue(respond({ query: 'kestrel', results: [hit] }))

    await expect(host.knowledge.search('kestrel')).resolves.toEqual([hit])

    expect(onlyRequest().path).toBe('/api/knowledge/search?q=kestrel')
  })
})

