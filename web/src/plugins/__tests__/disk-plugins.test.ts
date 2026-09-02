import { beforeAll, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest'
import { contributions } from '@/contrib/registry'
import { scanDiskPlugins } from '../disk-plugins'
import { useDataUrlModules } from './data-url-modules'

const authFetch = vi.fn()
vi.mock('@/lib/auth', () => ({ authFetch: (...args: unknown[]) => authFetch(...args) }))

let warn: MockInstance<typeof console.warn>

beforeAll(() => {
  useDataUrlModules()
})

beforeEach(() => {
  authFetch.mockReset()
  vi.spyOn(console, 'error').mockImplementation(() => {})
  warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
})

// The registry is a singleton — and now the only one in play, since the
// dashboard keeps no enablement store of its own — so each test works under a
// folder name and an area of its own.
let counter = 0
function fresh(): { folder: string; area: string } {
  counter += 1
  return { folder: `folder-${counter}`, area: `disk.plugins.area.${counter}` }
}

/** One row as the gateway lists it. Only `id` is read by the code under test;
 *  the rest is what `/settings/plugins` renders off the same response. */
interface Row {
  id: string
  name: string
  status: 'loaded' | 'disabled' | 'error'
  error?: string
}

function row(id: string, patch: Partial<Row> = {}): Row {
  return { id, name: id, status: 'loaded', ...patch }
}

function chipPlugin(id: string, area: string, local = 'chip'): string {
  return `export default {
  id: ${JSON.stringify(id)},
  register(ctx) { ctx.contribute({ id: ${JSON.stringify(local)}, area: ${JSON.stringify(area)} }); },
};
`
}

/** Answer as the gateway does: the servable subset under `plugins`, the full
 *  listing under `inventory` (which this side ignores), and a client half per
 *  folder that has one. A folder mapped to null 404s, which is what the gateway
 *  says for unknown, disabled, or missing. */
function gatewayServes(inventory: Row[], clients: Record<string, string | null>): void {
  authFetch.mockImplementation((path: string) => {
    if (path === '/api/plugins') {
      const servable = inventory.filter((entry) => entry.status === 'loaded')
      return Promise.resolve(Response.json({ plugins: servable, inventory }))
    }

    const id = /^\/api\/plugins\/(.+)\/client$/.exec(path)?.[1]
    const source = id ? clients[id] : undefined
    return Promise.resolve(
      source == null ? new Response('', { status: 404 }) : new Response(source, { status: 200 }),
    )
  })
}

describe('one pass', () => {
  // `config.yaml` is where enablement is decided, and the gateway's servable
  // list is that decision arriving. Nothing in the app has to opt in beside it.
  it('loads what the gateway serves', async () => {
    const { folder, area } = fresh()
    gatewayServes([row(folder)], { [folder]: chipPlugin(folder, area) })

    await scanDiskPlugins()

    expect(contributions.getArea(area).map((entry) => entry.id)).toEqual([`${folder}:chip`])
  })

  it('leaves what is loaded alone when the gateway cannot be read', async () => {
    const { folder, area } = fresh()
    gatewayServes([row(folder)], { [folder]: chipPlugin(folder, area) })
    await scanDiskPlugins()

    authFetch.mockRejectedValue(new Error('gateway is down'))
    await expect(scanDiskPlugins()).resolves.toBeUndefined()

    expect(contributions.getArea(area)).toHaveLength(1)
  })
})

describe('a hot edit that changes the plugin id', () => {
  it('disposes the previous id, not just the new one', async () => {
    const { folder, area } = fresh()
    const renamed = `${folder}-renamed`
    gatewayServes([row(folder)], { [folder]: chipPlugin(folder, area) })
    await scanDiskPlugins()
    expect(contributions.getArea(area)).toHaveLength(1)

    gatewayServes([row(folder)], { [folder]: chipPlugin(renamed, area) })
    await scanDiskPlugins()

    // The previous incarnation is gone, and the renamed one registered in its
    // place: the gateway served this folder, and that is the whole decision.
    expect(contributions.getArea(area).map((entry) => entry.id)).toEqual([`${renamed}:chip`])
  })

  it('registers the fixing save even when it loads under another id', async () => {
    const { folder, area } = fresh()
    const fixed = `${folder}-fixed`
    gatewayServes([row(folder)], { [folder]: 'export default 1;' })
    await scanDiskPlugins()
    expect(contributions.getArea(area)).toEqual([])

    gatewayServes([row(folder)], { [folder]: chipPlugin(fixed, area) })
    await scanDiskPlugins()

    expect(contributions.getArea(area).map((entry) => entry.id)).toEqual([`${fixed}:chip`])
  })
})

describe('folders that go away', () => {
  it('unloads a folder that disappears mid-pass rather than blaming it', async () => {
    const { folder, area } = fresh()
    gatewayServes([row(folder)], { [folder]: chipPlugin(folder, area) })
    await scanDiskPlugins()

    // Listed, then 404 on the client fetch — deleted between the two calls.
    gatewayServes([row(folder)], {})
    await scanDiskPlugins()

    expect(contributions.getArea(area)).toEqual([])
  })

  it('unloads a folder that is gone from the next listing', async () => {
    const { folder, area } = fresh()
    gatewayServes([row(folder)], { [folder]: chipPlugin(folder, area) })
    await scanDiskPlugins()

    gatewayServes([], {})
    await scanDiskPlugins()

    expect(contributions.getArea(area)).toEqual([])
  })

  it('unregisters a running plugin that is disabled at the gateway', async () => {
    const { folder, area } = fresh()
    gatewayServes([row(folder)], { [folder]: chipPlugin(folder, area) })
    await scanDiskPlugins()

    // Still on disk, still compiling — but no longer served, which is the only
    // signal that counts.
    gatewayServes([row(folder, { status: 'disabled' })], { [folder]: chipPlugin(folder, area) })
    await scanDiskPlugins()

    expect(contributions.getArea(area)).toEqual([])
  })
})

describe('a client half the gateway could not compile', () => {
  const REASON = 'client.js:4:22: Unexpected end of file'

  /** The 422 the client route answers for a plugin whose JSX will not parse:
   *  installed, served, and broken — which is not the same as absent. */
  function gatewayRefusesToCompile(folder: string): void {
    authFetch.mockImplementation((path: string) => {
      if (path === '/api/plugins') {
        return Promise.resolve(Response.json({ plugins: [row(folder)], inventory: [row(folder)] }))
      }
      return Promise.resolve(Response.json({ error: REASON }, { status: 422 }))
    })
  }

  it('warns with the file and line its author has to fix', async () => {
    const { folder } = fresh()
    gatewayRefusesToCompile(folder)

    await scanDiskPlugins()

    expect(warn).toHaveBeenCalledWith(
      `[plugins] ${folder} is installed and will not compile: ${REASON}`,
    )
  })

  it('leaves the running plugin registered rather than unloading it as missing', async () => {
    const { folder, area } = fresh()
    gatewayServes([row(folder)], { [folder]: chipPlugin(folder, area) })
    await scanDiskPlugins()
    expect(contributions.getArea(area)).toHaveLength(1)

    gatewayRefusesToCompile(folder)
    await scanDiskPlugins()

    expect(contributions.getArea(area)).toHaveLength(1)
  })
})

describe('the scanning guard', () => {
  it('makes a rescan during a pass a no-op rather than an overlap', async () => {
    const { folder, area } = fresh()
    let release = () => {}
    const held = new Promise<void>((resolve) => {
      release = resolve
    })
    const listing = Response.json({ plugins: [row(folder)], inventory: [row(folder)] })
    authFetch.mockImplementation(async (path: string) => {
      if (path !== '/api/plugins') return new Response(chipPlugin(folder, area), { status: 200 })
      await held
      return listing
    })

    const first = scanDiskPlugins()
    await expect(scanDiskPlugins()).resolves.toBeUndefined()

    // The second call returned without so much as asking the gateway.
    expect(authFetch).toHaveBeenCalledTimes(1)

    release()
    await first
    expect(authFetch).toHaveBeenCalledWith('/api/plugins')
  })
})
