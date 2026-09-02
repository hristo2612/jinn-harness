import { Component, Suspense, useSyncExternalStore, type ComponentType, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import { Navigate, Outlet, RouterProvider, createBrowserRouter, redirect, type RouteObject } from 'react-router-dom'
import { ClientProviders } from './routes/client-providers'
import { ContributedRoute, reservedSegments } from './routes/contributed-route'
import { registerHostNavigator } from './plugins/sdk/host-bridge'
import { lazyRoute } from './lib/lazy-route'
import { registerRoutePrefetch } from './lib/route-prefetch'
import { startKeyboardInset } from './platform'
import { RouteLoading } from './components/route-loading'
import { APP_ROUTES, type AppRouteId } from './lib/app-routes'
import type { NativeGatewayProfiles, NativeGatewayProfilesSnapshot } from './lib/native-gateway-profiles'
import { nativeBridge } from './platform/native-bridge'
import './routes/globals.css'

let profiles: NativeGatewayProfiles | undefined
let initialNativeOrigin: string | undefined
let PairingScreen: ComponentType<Record<string, never>> | undefined

// UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 5): the route table is
// Settings, Plugins, the plugin splat and two redirects; every other lazy route,
// its prefetch registration, the Talk navigator and the service worker are gone.
const SettingsPage = lazyRoute(() => import('./routes/settings/page'), 'settings')
const PluginsSettingsPage = lazyRoute(() => import('./routes/settings/plugins/page'), 'settings-plugins')

registerRoutePrefetch('/settings', SettingsPage.prefetch)

class AppErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  override state: { error: Error | null } = { error: null }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  override componentDidCatch(error: Error) {
    console.error('[AppErrorBoundary]', error)
  }

  override render() {
    if (!this.state.error) return this.props.children
    return (
      <div className="flex h-dvh flex-col items-center justify-center gap-3 bg-background p-6 text-center">
        <div className="text-subheadline font-medium text-foreground">Web UI needs a refresh</div>
        <button
          className="rounded-md bg-[var(--accent)] px-4 py-2 text-subheadline font-medium text-white active:scale-[0.96] transition-transform"
          onClick={() => window.location.reload()}
        >
          Refresh
        </button>
      </div>
    )
  }
}

function AppShell() {
  const generation = useSyncExternalStore(
    profiles?.subscribe ?? (() => () => {}),
    () => profiles?.snapshot().generation ?? 0,
    () => 0,
  )
  return (
    <ClientProviders key={`gateway:${generation}`}>
      <Suspense fallback={<RouteLoading />}>
        <Outlet />
      </Suspense>
    </ClientProviders>
  )
}

const routeElements: Partial<Record<AppRouteId, ReactNode>> = {
  "root-redirect": <Navigate to="/settings" replace />,
  "more-redirect": <Navigate to="/settings" replace />,
  settings: <SettingsPage />,
  "settings-plugins": <PluginsSettingsPage />,
}

// Redirect routes resolve at the ROUTER level so they never commit an
// intermediate frame (inventory §2.18). An element-level <Navigate> renders null
// for a full commit — the mobile "tab bar flashes out on the way to Todos" bug.
const settingsRedirectLoader: RouteObject["loader"] = () => redirect("/settings")

const routeLoaders: Partial<Record<AppRouteId, RouteObject["loader"]>> = {
  "root-redirect": settingsRedirectLoader,
  "more-redirect": settingsRedirectLoader,
}

const appRoutes: RouteObject[] = APP_ROUTES.flatMap((route) => {
  if (route.id === "plugin-contributed") return []
  const element = routeElements[route.id]
  const loader = routeLoaders[route.id]
  return element ? [{ path: route.path, element, ...(loader ? { loader } : {}) }] : []
})

const router = createBrowserRouter([
  {
    element: <AppShell />,
    children: [
      ...appRoutes,
      // A plugin's page, last and on the splat so the app's own routes are
      // matched first — a contribution can never shadow one of them.
      {
        path: '*',
        element: <ContributedRoute reserved={reservedSegments(APP_ROUTES.filter((route) => route.id !== "plugin-contributed").map((route) => route.path))} />,
      },
    ],
  },
])

// A plugin navigates from an event handler or a backend callback rather than
// from a render, so it reaches the router through a module-level handle. The
// promise is dropped rather than handed back: a plugin has no latency clock to
// time against the landing.
registerHostNavigator((path) => void router.navigate(path))

/**
 * Whether the native window has to show its own gateway surface instead of the
 * app. A remembered gateway that stopped answering counts: the app behind it can
 * only reach a dead origin, and the browser's pairing screen it would fall
 * through to belongs to a gateway that replied. A failed SWITCH does not count:
 * the working gateway stays active there and the switcher reports it in place.
 */
function nativeGatewayBlocked(snapshot: NativeGatewayProfilesSnapshot | undefined, origin: string | undefined): boolean {
  if (!origin) return true
  return snapshot ? !snapshot.activeReachable : false
}

function App() {
  const snapshot = useSyncExternalStore(
    profiles?.subscribe ?? (() => () => {}),
    () => profiles?.snapshot(),
    () => undefined,
  )
  const nativeOrigin = snapshot?.profiles.find((profile) => profile.id === snapshot.activeId)?.origin ?? initialNativeOrigin
  if (nativeBridge() && PairingScreen && nativeGatewayBlocked(snapshot, nativeOrigin)) return <PairingScreen />
  return (
    <AppErrorBoundary>
      <RouterProvider router={router} />
    </AppErrorBoundary>
  )
}

// Runs for the life of the document, so the unsubscribe is deliberately dropped.
startKeyboardInset()

const rootEl = document.getElementById('root')
if (!rootEl) throw new Error('Root element #root not found')

async function mount(): Promise<void> {
  if (nativeBridge()) {
    const [bootstrap, pairing] = await Promise.all([
      import('./lib/native-gateway-bootstrap'),
      import('./components/auth/native-pairing-screen'),
    ])
    initialNativeOrigin = bootstrap.installSavedNativeGateway()
    profiles = bootstrap.nativeGatewayProfiles()
    PairingScreen = pairing.NativePairingScreen
    // Synchronous up to its first await, so the first paint is already the
    // connecting state rather than a router mounted on an origin nobody proved.
    void profiles?.verifyActive()
  }
  createRoot(rootEl!).render(<App />)
}

void mount()
