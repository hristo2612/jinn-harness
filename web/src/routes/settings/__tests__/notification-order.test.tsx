import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import type React from 'react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, expect, it, vi } from 'vitest'
import SettingsPage from '../page'
import { CONFIG_COMMIT_DEBOUNCE_MS } from '../use-config-commit'

// Exercises the real page, adapter, commit hook and status; only ancillary services are mocked.

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

/** Long enough that a second write, if the page had one queued, would have gone. */
function afterTheWindow() {
  return new Promise((resolve) => setTimeout(resolve, CONFIG_COMMIT_DEBOUNCE_MS + 200))
}

beforeEach(() => {
  vi.clearAllMocks()
  apiMocks.updateConfig.mockResolvedValue({ revision: 'rev-2' })
  apiMocks.getOrg.mockResolvedValue({ employees: [] })
  apiMocks.sttStatus.mockResolvedValue({ available: false, model: null, downloading: false, progress: 0, languages: ['en'] })
  apiMocks.sttUpdateConfig.mockResolvedValue({})
  apiMocks.sttDownload.mockResolvedValue({})
  fetchTalkCapability.mockResolvedValue({ configured: true, provider: 'openai', providers: ['openai'] })
})


const fetchProbe = vi.hoisted(() => vi.fn())
vi.mock('@/lib/auth', () => ({authFetch: (...args: unknown[]) => fetchProbe(...args)}))
const {createConfigApi} = await import('@/lib/api-config')
const wireJson = (body: unknown) => new Response(JSON.stringify(body), {status:200,headers:{'Content-Type':'application/json'}})
it('Reload started during a save cannot erase its newer pending notification', async () => {
 let holdRead=false
 let releaseRead!: (response: Response) => void
 let releasePatch!: (response: Response) => void
 let readStarted!: () => void
 const started=new Promise<void>(resolve=>{readStarted=resolve})
 const oldWire={namespace:'cron',revision:1,settings:{jobs:[{id:'health','every-ms':2000,topic:'cron:health'}]},schema:{properties:{jobs:{kind:'array'}}},notification:{state:'settled',revision:1}}
 fetchProbe.mockImplementation(async(path:string,init?:RequestInit)=>{
  if(path==='/v1/settings')return wireJson({namespaces:{cron:{}}})
  if(init?.method==='PATCH')return new Promise<Response>(resolve=>{releasePatch=resolve})
  if(holdRead){readStarted();return new Promise<Response>(resolve=>{releaseRead=resolve})}
  return wireJson(oldWire)
 })
 const real=createConfigApi({responseError:async()=>new Error('http'),conflict:()=>new Error('conflict'),moment:async(_d,_t,payload)=>wireJson(payload)})
 apiMocks.getConfig.mockImplementation(real.getConfig)
 apiMocks.updateConfig.mockImplementation(real.updateConfig)
 render(<MemoryRouter><SettingsPage /></MemoryRouter>)
 const jobs=await screen.findByRole('textbox',{name:'jobs',exact:true})
 fireEvent.change(jobs,{target:{value:JSON.stringify([{id:'health','every-ms':1000,topic:'cron:health'}])}})
 fireEvent.blur(jobs)
 // Reload starts during the normal 600ms debounce, so its old GET is
 // observed before the new PATCH is sent; only its browser delivery is delayed.
 holdRead=true
 fireEvent.click(screen.getByRole('button',{name:'Reload',exact:true}))
 await started
 await screen.findByText('Saving…')
 await waitFor(()=>expect(releasePatch).toBeTypeOf('function'))
 await act(async()=>releasePatch(wireJson({...oldWire,revision:2,settings:{jobs:[{id:'health','every-ms':1000,topic:'cron:health'}]},notification:{state:'pending',revision:2}})))
 const pending=await screen.findByText(/Saved.*cron notification pending/)
 console.log('AFTER PATCH:',pending.textContent)
 await act(async()=>releaseRead(wireJson(oldWire)))
 await screen.findByRole('textbox',{name:'jobs',exact:true})
 console.log('AFTER LATE RELOAD: pending banner=',screen.queryByText(/cron notification pending/)?.textContent??null,'jobs=',(screen.getByRole('textbox',{name:'jobs',exact:true}) as HTMLTextAreaElement).value,'PATCH count=',fetchProbe.mock.calls.filter(([,i])=>(i as RequestInit|undefined)?.method==='PATCH').length)
 expect(screen.queryByText(/cron notification pending/),'the actual Settings page must preserve uncertainty for revision 2').not.toBeNull()
 expect(JSON.parse((screen.getByRole('textbox',{name:'jobs',exact:true}) as HTMLTextAreaElement).value)).toEqual([{id:'health','every-ms':1000,topic:'cron:health'}])
 await afterTheWindow()
 expect(fetchProbe.mock.calls.filter(([,init])=>(init as RequestInit|undefined)?.method==='PATCH')).toHaveLength(1)
})
