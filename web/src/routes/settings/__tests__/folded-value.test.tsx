import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type React from 'react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import SettingsPage from '../page'

/* UI-2 (docs/plans/ui-malleability-arc.md §9.7 amendment 8(d)): a save is a
 * moment first, and an extension may FOLD the patch. The page must then show
 * what the daemon holds, never say "Saved" over the draft it sent. */

const apiMocks = vi.hoisted(() => ({
  getConfig: vi.fn(),
  updateConfig: vi.fn(),
  getOrg: vi.fn(),
  sttStatus: vi.fn(),
  sttUpdateConfig: vi.fn(),
  sttDownload: vi.fn(),
}))
const fetchTalkCapability = vi.hoisted(() => vi.fn())

vi.mock('@/lib/api', () => ({ api: apiMocks }))
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

const INTERRUPT = { name: 'Interrupt on new message' }

async function renderSettings() {
  render(<MemoryRouter><SettingsPage /></MemoryRouter>)
  return screen.findByRole('switch', INTERRUPT)
}

beforeEach(() => {
  vi.clearAllMocks()
  apiMocks.getConfig.mockResolvedValue({ config: { sessions: { interruptOnNewMessage: true } }, revision: 'rev-1' })
  apiMocks.getOrg.mockResolvedValue({ employees: [] })
  apiMocks.sttStatus.mockResolvedValue({ available: false, model: null, downloading: false, progress: 0, languages: ['en'] })
  apiMocks.sttUpdateConfig.mockResolvedValue({})
  apiMocks.sttDownload.mockResolvedValue({})
  fetchTalkCapability.mockResolvedValue({ configured: true, provider: 'openai', providers: ['openai'] })
})

describe('a save the daemon folded', () => {
  it('shows the folded value, not the draft, once it says saved', async () => {
    // The extension folds the toggle back on: the daemon holds `true`.
    apiMocks.updateConfig.mockResolvedValue({
      revision: 'rev-2',
      config: { sessions: { interruptOnNewMessage: true } },
    })

    const toggle = await renderSettings()
    fireEvent.click(toggle)
    expect(toggle.getAttribute('aria-checked')).toBe('false')

    expect(await screen.findByText('Saved')).toBeTruthy()
    await waitFor(() => expect(screen.getByRole('switch', INTERRUPT).getAttribute('aria-checked')).toBe('true'))
  })

  it('leaves an edit queued during the write alone: that write brings its own answer', async () => {
    let land: (result: { revision: string; config: Record<string, unknown> }) => void = () => {}
    apiMocks.updateConfig
      .mockReturnValueOnce(new Promise((resolve) => { land = resolve }))
      .mockResolvedValueOnce({ revision: 'rev-3', config: { sessions: { interruptOnNewMessage: true } } })

    const toggle = await renderSettings()
    fireEvent.click(toggle)
    await waitFor(() => expect(apiMocks.updateConfig).toHaveBeenCalledTimes(1))
    // A second edit while the first is in flight.
    fireEvent.click(screen.getByRole('switch', INTERRUPT))
    expect(screen.getByRole('switch', INTERRUPT).getAttribute('aria-checked')).toBe('true')

    land({ revision: 'rev-2', config: { sessions: { interruptOnNewMessage: false } } })
    await waitFor(() => expect(apiMocks.updateConfig).toHaveBeenCalledTimes(2))
    // The first answer (false) never replaced the queued edit (true); the second write's answer is what shows.
    await waitFor(() => expect(screen.getByRole('switch', INTERRUPT).getAttribute('aria-checked')).toBe('true'))
    expect((apiMocks.updateConfig.mock.calls[1][0] as { sessions: { interruptOnNewMessage: boolean } }).sessions.interruptOnNewMessage).toBe(true)
  })
})
