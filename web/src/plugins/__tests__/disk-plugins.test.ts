import { beforeEach, describe, expect, it, vi } from 'vitest'
import { diskPluginsSettled, scanDiskPlugins, subscribeDiskPluginsSettled } from '../disk-plugins'

const authFetch = vi.fn()
vi.mock('@/lib/auth', () => ({ authFetch: (...args: unknown[]) => authFetch(...args) }))

beforeEach(() => {
  authFetch.mockReset()
})

// UI-1 §4.2 item 12: the daemon serves no client halves (FINDINGS #37 / KG-1),
// so the disk door resolves EMPTY client-side and issues no request. The ten
// tests of the old gateway's reconcile went with it. What stays is the fact the
// contributed route waits on: a pass settles.
describe('one pass', () => {
  it('issues no request and settles', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch')
    const settledListener = vi.fn()
    subscribeDiskPluginsSettled(settledListener)
    expect(diskPluginsSettled()).toBe(false)

    await expect(scanDiskPlugins()).resolves.toBeUndefined()

    expect(authFetch).not.toHaveBeenCalled()
    expect(fetchSpy).not.toHaveBeenCalled()
    expect(diskPluginsSettled()).toBe(true)
    expect(settledListener).toHaveBeenCalledTimes(1)
  })
})
