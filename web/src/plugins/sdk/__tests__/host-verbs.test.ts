import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { host } from '../host'
import { PluginSdkError } from '../errors'
import { requestOf, respond, stubFetch } from './host-verbs-harness'

/** The company verbs: Todos, sessions, the org — plus how a refusal reads. */
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

describe('host.todos', () => {
  it('lists over GET /api/work-items and unwraps the page', async () => {
    const item = { id: 'AAA-1', title: 'a todo', status: 'backlog' }
    fetchMock.mockResolvedValue(respond({ workItems: [item], total: 1 }))

    await expect(host.todos.list()).resolves.toEqual([item])

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/work-items')
    expect(init.method ?? 'GET').toBe('GET')
  })

  /* The gateway reads only `rootsOnly=true` literally and ignores anything
   * else, so a boolean that stringified to "false" would silently widen the
   * page rather than narrow it. */
  it('sends only the filters that were set, in the spelling the route reads', async () => {
    fetchMock.mockResolvedValue(respond({ workItems: [] }))

    await host.todos.list({ status: 'executing', rootsOnly: true, limit: 5 })

    expect(onlyRequest().path).toBe('/api/work-items?status=executing&rootsOnly=true&limit=5')
  })

  it('omits a filter left unset rather than sending an empty one', async () => {
    fetchMock.mockResolvedValue(respond({ workItems: [] }))

    await host.todos.list({ rootsOnly: false })

    expect(onlyRequest().path).toBe('/api/work-items')
  })

  it('creates over POST /api/work-items and unwraps the item', async () => {
    const workItem = { id: 'AAA-2', title: 'minted', status: 'backlog' }
    fetchMock.mockResolvedValue(respond({ workItem }))

    await expect(host.todos.create({ title: 'minted' })).resolves.toEqual(workItem)

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/work-items')
    expect(init.method).toBe('POST')
    expect(JSON.parse(String(init.body))).toEqual({ title: 'minted' })
  })

  it('comments over POST /api/work-items/:id/comments and unwraps the comment', async () => {
    const comment = { id: 'wic_0123456789ab', body: 'noted' }
    fetchMock.mockResolvedValue(respond({ comment }))

    await expect(host.todos.comment('AAA-3', 'noted')).resolves.toEqual(comment)

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/work-items/AAA-3/comments')
    expect(init.method).toBe('POST')
    expect(JSON.parse(String(init.body))).toEqual({ body: 'noted' })
  })
})

describe('host.sessions.spawn', () => {
  it('spawns over POST /api/sessions with the request as the body', async () => {
    const session = { id: 'sess-1', engine: 'codex', status: 'running' }
    fetchMock.mockResolvedValue(respond(session))

    await expect(host.sessions.spawn({ prompt: 'draft a reply', employee: 'a-lead' })).resolves.toEqual(
      session,
    )

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/sessions')
    expect(init.method).toBe('POST')
    expect(JSON.parse(String(init.body))).toEqual({ prompt: 'draft a reply', employee: 'a-lead' })
  })
})

describe('host.employees.list', () => {
  it('reads GET /api/org and unwraps the roster', async () => {
    const employee = { name: 'a-lead', displayName: 'A Lead', department: 'ops' }
    fetchMock.mockResolvedValue(respond({ employees: [employee], departments: ['ops'] }))

    await expect(host.employees.list()).resolves.toEqual([employee])

    const { path, init } = onlyRequest()
    expect(path).toBe('/api/org')
    expect(init.method ?? 'GET').toBe('GET')
  })
})

describe('a verb the gateway refuses', () => {
  it('raises the gateway’s own message, not a generic failure', async () => {
    fetchMock.mockResolvedValue(respond({ error: 'title is required' }, { ok: false, status: 400 }))

    await expect(host.todos.create({ title: '' })).rejects.toThrow(
      /host\.todos\.create failed: title is required/,
    )
  })

  it('falls back to the status line when the body is not the error envelope', async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 502,
      statusText: 'Bad Gateway',
      json: async () => {
        throw new Error('not JSON')
      },
    } as unknown as Response)

    await expect(host.employees.list()).rejects.toThrow(PluginSdkError)
    await expect(host.employees.list()).rejects.toThrow(/502 Bad Gateway/)
  })
})
