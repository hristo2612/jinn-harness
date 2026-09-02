import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type React from 'react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import SettingsPage from '../page'
import { CONFIG_COMMIT_DEBOUNCE_MS } from '../use-config-commit'

/* Somebody hand-edited config.yaml while this page was open. The page must say so
 * and stop, rather than PUT the document it read before the edit existed. */

const apiMocks = vi.hoisted(() => ({
  getConfig: vi.fn(),
  updateConfig: vi.fn(),
  getOrg: vi.fn(),
  sttStatus: vi.fn(),
  sttUpdateConfig: vi.fn(),
  sttDownload: vi.fn(),
}))
const ApiError = vi.hoisted(() => class ApiError extends Error {
  status: number
  code?: string
  remedy?: string
  constructor(status: number, message: string, code?: string, remedy?: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
    this.remedy = remedy
  }
})
const fetchTalkCapability = vi.hoisted(() => vi.fn())

vi.mock('@/lib/api', () => ({ api: apiMocks, ApiError }))
vi.mock('@/lib/talk-capability', () => ({ fetchTalkCapability }))
vi.mock('@/components/page-layout', () => ({ PageLayout: ({ children }: { children: React.ReactNode }) => <>{children}</> }))
vi.mock('@/routes/providers', () => ({ useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }) }))
vi.mock('@/routes/settings-provider', () => ({
  useSettings: () => ({
    settings: {},
    setAccentColor: vi.fn(), setCompanyName: vi.fn(), setPortalName: vi.fn(),
    setPortalSubtitle: vi.fn(), setOperatorName: vi.fn(), setPortalEmoji: vi.fn(),
    setLanguage: vi.fn(), resetAll: vi.fn(),
  }),
}))
vi.mock('@/hooks/use-model-registry', () => ({ useModelRegistry: () => ({ data: undefined }) }))
vi.mock('@/hooks/use-onboarding', () => ({ useOnboarding: () => ({ data: undefined }) }))
vi.mock('@/components/ui/emoji-picker', () => ({ EmojiPicker: () => null }))
vi.mock('@/components/auth/remote-access-panel', () => ({ RemoteAccessPanel: () => null }))
vi.mock('@/routes/auth-provider', () => ({
  useAuth: () => ({ authState: {}, devices: [], createPairingCode: vi.fn(), logout: vi.fn(), unpairDevice: vi.fn() }),
}))

const NOTICE = /changed on disk/i

function conflict() {
  return new ApiError(409, 'config.yaml changed on disk after this page loaded it', 'CONFIG_CONFLICT', 'Reload and reapply.')
}

async function renderSettings() {
  render(<MemoryRouter><SettingsPage /></MemoryRouter>)
  await screen.findByRole('switch', { name: 'Interrupt on new message' })
}

/** There is no Save button: making an edit is what asks for a write. */
function editConfig() {
  fireEvent.click(screen.getByRole('switch', { name: 'Interrupt on new message' }))
}

beforeEach(() => {
  vi.clearAllMocks()
  apiMocks.getConfig.mockResolvedValue({ config: { logging: { level: 'info' } }, revision: 'rev-1' })
  apiMocks.updateConfig.mockResolvedValue({})
  apiMocks.getOrg.mockResolvedValue({ employees: [] })
  apiMocks.sttStatus.mockResolvedValue({ available: false, model: null, downloading: false, progress: 0, languages: ['en'] })
  apiMocks.sttUpdateConfig.mockResolvedValue({})
  apiMocks.sttDownload.mockResolvedValue({})
  fetchTalkCapability.mockResolvedValue({ configured: true, provider: 'openai', providers: ['openai'] })
})

describe('config revision', () => {
  it('sends the revision it loaded with, so the gateway can judge staleness', async () => {
    await renderSettings()
    editConfig()

    await waitFor(() => expect(apiMocks.updateConfig).toHaveBeenCalled())
    expect(apiMocks.updateConfig.mock.calls[0][1]).toBe('rev-1')
  })
})

/** Long enough that a queued write, if one survived, would have gone out. */
function afterTheWindow() {
  return new Promise((resolve) => setTimeout(resolve, CONFIG_COMMIT_DEBOUNCE_MS + 200))
}

describe('a reload with an edit still queued', () => {
  it('drops the queued edit rather than sending it under the revision the reload adopted', async () => {
    await renderSettings()
    editConfig()

    // The file moved on, and the operator reloads inside the debounce window.
    apiMocks.getConfig.mockResolvedValue({ config: { logging: { level: 'error' } }, revision: 'rev-2' })
    const reloads = screen.getAllByRole('button', { name: 'Reload' })
    fireEvent.click(reloads[reloads.length - 1])
    await waitFor(() => expect(apiMocks.getConfig).toHaveBeenCalledTimes(2))

    // Sending it now would pass the staleness check and overwrite what the reload fetched.
    await afterTheWindow()
    expect(apiMocks.updateConfig).not.toHaveBeenCalled()
  })
})

describe('a hand edit under an open page', () => {
  it('renders a conflict notice instead of the generic failure, and does not retry', async () => {
    apiMocks.updateConfig.mockRejectedValueOnce(conflict())
    await renderSettings()
    editConfig()

    expect(await screen.findByText(NOTICE)).toBeTruthy()
    // Not the error toast every other failure gets.
    expect(screen.queryByText(/Failed to save/)).toBeNull()
    // One PUT. A retry is exactly the clobber the notice exists to prevent.
    expect(apiMocks.updateConfig).toHaveBeenCalledTimes(1)
  })

  it('reloads the file the operator actually edited, and clears the notice', async () => {
    apiMocks.updateConfig.mockRejectedValueOnce(conflict())
    await renderSettings()
    editConfig()
    await screen.findByText(NOTICE)

    apiMocks.getConfig.mockResolvedValue({ config: { logging: { level: 'debug' } }, revision: 'rev-2' })
    fireEvent.click(screen.getByRole('button', { name: 'Reload config' }))

    await waitFor(() => expect(screen.queryByText(NOTICE)).toBeNull())
    // Saving again carries the revision the reload adopted, so it lands.
    editConfig()
    await waitFor(() => expect(apiMocks.updateConfig).toHaveBeenCalledTimes(2))
    expect(apiMocks.updateConfig.mock.calls[1][1]).toBe('rev-2')
    expect((apiMocks.updateConfig.mock.calls[1][0] as any).logging.level).toBe('debug')
  })

  it('still shows the ordinary failure message for every other rejection', async () => {
    apiMocks.updateConfig.mockRejectedValueOnce(new Error('Invalid config: engines.claude.fallback[0] "nope"'))
    await renderSettings()
    editConfig()

    expect(await screen.findByText(/Failed to save: Invalid config/)).toBeTruthy()
    expect(screen.queryByText(NOTICE)).toBeNull()
  })
})
