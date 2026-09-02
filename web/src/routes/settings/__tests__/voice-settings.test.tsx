import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type React from 'react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import SettingsPage from '../page'

/**
 * The Voice section, and the Talk Orb row it cross-references.
 *
 * The account key is the thing under test as much as the controls are: the page
 * is served a sentinel rather than the key, and a save has to hand that sentinel
 * back untouched so the gateway keeps what it has.
 */

const apiMocks = vi.hoisted(() => ({
  getConfig: vi.fn(),
  updateConfig: vi.fn(),
  getOrg: vi.fn(),
  sttStatus: vi.fn(),
  sttUpdateConfig: vi.fn(),
  sttDownload: vi.fn(),
}))

const fetchTalkCapability = vi.hoisted(() => vi.fn())

const settingsMock = vi.hoisted(() => ({
  talkOrb: true,
  talkOrbVariant: 'mist',
  talkOrbIntensity: 'standard',
}))
const setTalkOrbVariant = vi.hoisted(() => vi.fn())
const setTalkOrbIntensity = vi.hoisted(() => vi.fn())

vi.mock('@/lib/api', () => ({ api: apiMocks }))
vi.mock('@/components/talk/orb-canvas', () => ({
  OrbCanvas: ({ variant }: { variant: string }) => <canvas data-test-orb-variant={variant} />,
}))
vi.mock('@/lib/talk-capability', () => ({ fetchTalkCapability }))
vi.mock('@/components/page-layout', () => ({ PageLayout: ({ children }: { children: React.ReactNode }) => <>{children}</> }))
vi.mock('@/routes/providers', () => ({ useTheme: () => ({ theme: 'dark', setTheme: vi.fn() }) }))
vi.mock('@/routes/settings-provider', () => ({
  useSettings: () => ({
    settings: settingsMock,
    setAccentColor: vi.fn(),
    setCompanyName: vi.fn(),
    setPortalName: vi.fn(),
    setPortalSubtitle: vi.fn(),
    setOperatorName: vi.fn(),
    setPortalEmoji: vi.fn(),
    setLanguage: vi.fn(),
    setTalkOrb: vi.fn(),
    setTalkOrbVariant,
    setTalkOrbIntensity,
    resetAll: vi.fn(),
  }),
}))
vi.mock('@/hooks/use-model-registry', () => ({ useModelRegistry: () => ({ data: undefined }) }))
vi.mock('@/hooks/use-onboarding', () => ({ useOnboarding: () => ({ data: undefined }) }))
vi.mock('@/components/ui/emoji-picker', () => ({ EmojiPicker: () => null }))
vi.mock('@/components/auth/remote-access-panel', () => ({ RemoteAccessPanel: () => null }))
vi.mock('@/routes/auth-provider', () => ({
  useAuth: () => ({ authState: {}, devices: [], createPairingCode: vi.fn(), logout: vi.fn(), unpairDevice: vi.fn() }),
}))

const STORED_KEY_SENTINEL = '***'
const NOT_SET_UP = /Voice is not set up yet/

function renderSettings() {
  render(
    <MemoryRouter>
      <SettingsPage />
    </MemoryRouter>,
  )
}

/** There is no Save button: an edit is what asks for a write. */
function save() {
  fireEvent.click(screen.getByRole('switch', { name: 'Interrupt on new message' }))
}

beforeEach(() => {
  settingsMock.talkOrb = true
  setTalkOrbVariant.mockReset()
  setTalkOrbIntensity.mockReset()
  apiMocks.getConfig.mockResolvedValue({ config: { realtime: { provider: 'openai', apiKey: STORED_KEY_SENTINEL } }, revision: 'rev-1' })
  apiMocks.updateConfig.mockResolvedValue({ revision: 'rev-1' })
  apiMocks.getOrg.mockResolvedValue({ employees: [] })
  apiMocks.sttStatus.mockResolvedValue({ available: false, model: null, downloading: false, progress: 0, languages: ['en'] })
  apiMocks.sttUpdateConfig.mockResolvedValue({})
  apiMocks.sttDownload.mockResolvedValue({})
  fetchTalkCapability.mockResolvedValue({
    configured: true,
    provider: 'openai',
    providers: ['openai'],
    voices: ['alloy', 'cedar', 'marin'],
  })
})

describe('the Voice section', () => {
  /**
   * Every field of `RealtimeConfig` reaches config.yaml, and the key is the one
   * that does not travel as a value. `semantic_vad` is written as a mapping
   * because that is the only form the provider's union accepts.
   */
  it('round-trips every realtime field, and never writes the sentinel as a key', async () => {
    renderSettings()
    await screen.findByText('Stored')

    fireEvent.change(screen.getByRole('textbox', { name: 'Realtime model' }), {
      target: { value: 'gpt-realtime' },
    })
    fireEvent.change(screen.getByRole('combobox', { name: 'Realtime voice' }), {
      target: { value: 'marin' },
    })
    fireEvent.change(screen.getByRole('combobox', { name: 'Realtime turn detection' }), {
      target: { value: 'semantic_vad' },
    })
    fireEvent.change(screen.getByRole('combobox', { name: 'Realtime noise reduction' }), {
      target: { value: 'near_field' },
    })
    save()

    await waitFor(() =>
      expect(apiMocks.updateConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          realtime: {
            provider: 'openai',
            apiKey: STORED_KEY_SENTINEL,
            model: 'gpt-realtime',
            voice: 'marin',
            turnDetection: { type: 'semantic_vad' },
            noiseReduction: 'near_field',
          },
        }),
        'rev-1',
      ),
    )
  })

  it('offers the provider voices the gateway reports, and nothing invented', async () => {
    renderSettings()

    const voice = await screen.findByRole('combobox', { name: 'Realtime voice' })
    const offered = Array.from(voice.querySelectorAll('option')).map((option) => option.textContent)
    expect(offered).toEqual(['Provider default', 'alloy', 'cedar', 'marin'])
  })

  it('falls back to a free field when the provider offers no voices', async () => {
    fetchTalkCapability.mockResolvedValue({ configured: true, provider: 'gemini', providers: ['openai'], voices: [] })
    renderSettings()

    expect(await screen.findByRole('textbox', { name: 'Realtime voice' })).not.toBeNull()
    expect(screen.queryByRole('combobox', { name: 'Realtime voice' })).toBeNull()
  })

  it('reads a mapping turn detection back as its own name', async () => {
    apiMocks.getConfig.mockResolvedValue({
      config: { realtime: { provider: 'openai', apiKey: STORED_KEY_SENTINEL, turnDetection: { type: 'semantic_vad' } } },
      revision: 'rev-1',
    })
    renderSettings()

    const turn = await screen.findByRole('combobox', { name: 'Realtime turn detection' })
    expect((turn as HTMLSelectElement).value).toBe('semantic_vad')
  })

  it('offers the providers the gateway reports, and nothing invented', async () => {
    renderSettings()

    const provider = await screen.findByRole('combobox', { name: 'Voice provider' })
    const offered = Array.from(provider.querySelectorAll('option')).map((option) => option.textContent)
    expect(offered).toEqual(['Not set', 'openai'])
    expect((provider as HTMLSelectElement).value).toBe('openai')
  })

  it('says a key is stored without putting one on the screen', async () => {
    renderSettings()

    expect(await screen.findByText('Stored')).not.toBeNull()
    expect(screen.queryByLabelText('Voice API key')).toBeNull()
    expect(document.body.textContent).not.toContain(STORED_KEY_SENTINEL)
  })

  it('hands the sentinel back untouched, so saving does not overwrite the key', async () => {
    renderSettings()
    await screen.findByText('Stored')

    save()

    await waitFor(() =>
      expect(apiMocks.updateConfig).toHaveBeenCalledWith(
        expect.objectContaining({ realtime: { provider: 'openai', apiKey: STORED_KEY_SENTINEL } }),
        'rev-1',
      ),
    )
  })

  // An undefined provider is dropped by JSON.stringify, and a body with no
  // provider in it is how the gateway is told to keep the one it has — so the
  // page would report a save that had put the old provider straight back.
  it('clears the provider when it is set back to Not set', async () => {
    renderSettings()

    const provider = await screen.findByRole('combobox', { name: 'Voice provider' })
    fireEvent.change(provider, { target: { value: '' } })
    save()

    await waitFor(() =>
      expect(apiMocks.updateConfig).toHaveBeenCalledWith(
        expect.objectContaining({ realtime: { provider: null, apiKey: STORED_KEY_SENTINEL } }),
        'rev-1',
      ),
    )
  })

  it('sends a replacement key, and can be talked out of replacing', async () => {
    renderSettings()
    await screen.findByText('Stored')

    fireEvent.click(screen.getByRole('button', { name: 'Replace' }))
    fireEvent.change(screen.getByLabelText('Voice API key'), { target: { value: '${OPENAI_API_KEY}' } })
    save()

    await waitFor(() =>
      expect(apiMocks.updateConfig).toHaveBeenCalledWith(
        expect.objectContaining({ realtime: { provider: 'openai', apiKey: '${OPENAI_API_KEY}' } }),
        'rev-1',
      ),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Keep the current key' }))
    save()

    await waitFor(() =>
      expect(apiMocks.updateConfig).toHaveBeenLastCalledWith(
        expect.objectContaining({ realtime: { provider: 'openai', apiKey: STORED_KEY_SENTINEL } }),
        'rev-1',
      ),
    )
  })
})

describe('the Talk Orb row', () => {
  it('offers the four calm orb styles in Voice settings', async () => {
    renderSettings()

    const styles = await screen.findByRole('radiogroup', { name: 'Talk orb style' })
    expect(Array.from(styles.querySelectorAll('[data-orb-variant-option]')).map((option) =>
      option.getAttribute('data-orb-variant-option'))).toEqual(['mist', 'coin', 'ring', 'pulse'])

    fireEvent.click(screen.getByRole('radio', { name: 'Ring orb' }))
    expect(setTalkOrbVariant).toHaveBeenCalledWith('ring')
  })

  it('says voice is not set up when the orb is on and the gateway cannot open one', async () => {
    fetchTalkCapability.mockResolvedValue({ configured: false, provider: null, providers: ['openai'], voices: [] })
    renderSettings()

    expect(await screen.findByText(NOT_SET_UP)).not.toBeNull()
  })

  it('says nothing once voice is configured', async () => {
    renderSettings()
    await screen.findByText('Stored')

    expect(screen.queryByText(NOT_SET_UP)).toBeNull()
  })

  it('says nothing when the orb itself is switched off', async () => {
    settingsMock.talkOrb = false
    fetchTalkCapability.mockResolvedValue({ configured: false, provider: null, providers: ['openai'], voices: [] })
    renderSettings()

    await screen.findByRole('combobox', { name: 'Voice provider' })
    expect(screen.queryByText(NOT_SET_UP)).toBeNull()
  })
})
