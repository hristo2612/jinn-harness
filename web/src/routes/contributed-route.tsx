/**
 * The host for the `routes` area: a page a plugin contributes, rendered at a
 * path of its own.
 *
 * It is mounted as the router's LAST child, on the splat path, which is what
 * makes shadowing structurally impossible: React Router matches every static
 * child first, so a contribution can only ever be reached at a path the app
 * itself does not claim. The reserved-segment check below is the same rule said
 * a second time, in the one place that can say WHY a contribution never renders
 * instead of leaving its author to guess.
 */
import { useSyncExternalStore } from 'react'
import { Navigate, useLocation } from 'react-router-dom'
import { PageLayout } from '@/components/page-layout'
import { ContribBoundary } from '@/contrib/boundary'
import { ContributionOutlet } from '@/contrib/slot'
import { AREAS, type ResolvedContribution } from '@/contrib/types'
import { useContributions } from '@/contrib/use-contributions'
import { diskPluginsSettled, subscribeDiskPluginsSettled } from '@/plugins/disk-plugins'
import { RouteParamsProvider } from '@/plugins/sdk/route-params'

/** What a `routes` contribution declares. The element itself comes from
 *  `render()`, like every other UI contribution. */
export interface RouteContributionData {
  /** An absolute path of one or more segments, each either a literal or a
   *  `:name` that captures whatever the URL has there — `/inbox-demo`,
   *  `/inbox-demo/settings`, `/inbox-demo/:messageId`. */
  path: string
}

/** One segment of a contributed path. */
type PathSegment = { kind: 'static'; value: string } | { kind: 'param'; name: string }

/** A parsed path, or the one reason it is not one. */
type ParsedPath = { segments: PathSegment[] } | { problem: string }

const PARAM_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/

/** The first segment of a router path, which is the unit a contributed page
 *  competes for: `/notes/*` and `/todos/:todoId` both claim their whole subtree. */
export function firstSegment(routePath: string): string {
  return `/${routePath.replace(/^\//, '').split('/')[0] ?? ''}`
}

/** The segments the app's own routes claim, from the router's children rather
 *  than from a second list that could drift from them. */
export function reservedSegments(routePaths: readonly (string | undefined)[]): Set<string> {
  return new Set(routePaths.filter((path): path is string => !!path).map(firstSegment))
}

/** Ids already reported, so a rejected contribution is explained once rather
 *  than on every navigation. */
const explained = new Set<string>()

function reject(contribution: ResolvedContribution, problem: string): null {
  const key = `${contribution.source}:${contribution.id}`
  if (!explained.has(key)) {
    explained.add(key)
    console.warn(`[contrib:${contribution.id}] ${problem}`)
  }
  return null
}

/** A contributed path broken into segments, or the reason it cannot be one. */
function parsePath(path: string): ParsedPath {
  if (!path.startsWith('/')) return { problem: `path "${path}" must be absolute, starting with a "/"` }

  const segments: PathSegment[] = []
  const names = new Set<string>()
  for (const raw of path.slice(1).split('/')) {
    if (raw === '') return { problem: `path "${path}" has an empty segment` }
    if (raw.includes('*')) return { problem: `path "${path}" may not use a wildcard segment` }
    if (!raw.startsWith(':')) {
      segments.push({ kind: 'static', value: raw })
      continue
    }
    const name = raw.slice(1)
    if (!PARAM_NAME.test(name)) {
      return { problem: `path "${path}" has "${raw}" where a parameter name of letters, digits and underscores belongs` }
    }
    if (names.has(name)) return { problem: `path "${path}" names the parameter ":${name}" twice` }
    names.add(name)
    segments.push({ kind: 'param', name })
  }
  // A leading parameter captures anything, which would put the contribution in
  // front of every URL the app does not claim — the reserved check below can
  // only speak for the segments the app has actually spelled out.
  if (segments[0]?.kind !== 'static') {
    return { problem: `path "${path}" must begin with a static segment rather than a parameter` }
  }
  return { segments }
}

/** The segments a contribution may be rendered at, or null with the reason logged. */
function claimedSegments(
  contribution: ResolvedContribution,
  reserved: ReadonlySet<string>,
): PathSegment[] | null {
  const path = (contribution.data as Partial<RouteContributionData> | undefined)?.path
  if (typeof path !== 'string') {
    return reject(contribution, 'a routes contribution needs data.path as an absolute path')
  }
  const parsed = parsePath(path)
  if ('problem' in parsed) return reject(contribution, parsed.problem)
  if (typeof contribution.render !== 'function') {
    return reject(contribution, `path "${path}" has no render(), so there is nothing to show there`)
  }
  if (reserved.has(firstSegment(path))) {
    return reject(contribution, `path "${path}" is one of the app's own routes and will not be served`)
  }
  return parsed.segments
}

/** What `pathname` gives these segments — the parameters they capture, empty
 *  when they declare none — or null when the two do not line up. */
function capture(segments: readonly PathSegment[], pathname: string): Record<string, string> | null {
  const parts = pathname.split('/').slice(1)
  if (parts.length !== segments.length || parts.includes('')) return null

  const params: Record<string, string> = {}
  for (const [index, segment] of segments.entries()) {
    const part = parts[index]!
    if (segment.kind === 'param') params[segment.name] = part
    else if (segment.value !== part) return null
  }
  return params
}

/** Whether `candidate` beats `incumbent` at a pathname they both match: the
 *  first segment where the two differ decides it, and a literal beats a
 *  capture. Both matched the same pathname, so they are the same length. */
function isMoreSpecific(candidate: readonly PathSegment[], incumbent: readonly PathSegment[]): boolean {
  for (const [index, segment] of candidate.entries()) {
    const other = incumbent[index]!
    if (segment.kind !== other.kind) return segment.kind === 'static'
  }
  return false
}

/** What a pathname resolved to: the contribution that owns it, and what its
 *  path captured. */
export interface ContributedRouteMatch {
  contribution: ResolvedContribution
  params: Record<string, string>
}

/**
 * The contribution that owns `pathname`, or null when none does.
 *
 * Where two paths both fit — `/x/settings` and `/x/:id` at `/x/settings` — the
 * more specific one wins, so a detail page never swallows a sibling that was
 * spelled out. Equally specific ties go to the first registered, so a second
 * plugin claiming a taken path cannot displace it.
 */
export function contributedRouteFor(
  pathname: string,
  candidates: readonly ResolvedContribution[],
  reserved: ReadonlySet<string>,
): ContributedRouteMatch | null {
  let best: (ContributedRouteMatch & { segments: readonly PathSegment[] }) | null = null
  for (const contribution of candidates) {
    const segments = claimedSegments(contribution, reserved)
    if (!segments) continue
    const params = capture(segments, pathname)
    if (params && (!best || isMoreSpecific(segments, best.segments))) {
      best = { contribution, params, segments }
    }
  }
  return best ? { contribution: best.contribution, params: best.params } : null
}

/**
 * The splat route. A plugin's page when one claims this path, and otherwise the
 * app's answer to a URL nobody owns — the same redirect `/chat` gives, rather
 * than the router's bare error screen.
 *
 * Nothing is decided until the plugins have been looked for: a bookmarked
 * plugin page is rendered before the first scan has run, and redirecting in that
 * window would bounce every one of them to chat.
 */
export function ContributedRoute({ reserved }: { reserved: ReadonlySet<string> }) {
  const pathname = useLocation().pathname
  const settled = useSyncExternalStore(subscribeDiskPluginsSettled, diskPluginsSettled, () => true)
  const match = contributedRouteFor(pathname, useContributions(AREAS.routes), reserved)
  if (!match) return settled ? <Navigate to="/" replace /> : null

  // The app's chrome and the scroll container come from the host, not from the
  // plugin. `PageLayout` is not on the SDK's export list, so a contributed page
  // that had to supply its own would be a page with no way back to the rest of
  // the app.
  return (
    <PageLayout>
      <div className="h-full overflow-y-auto" data-scrollable>
        <ContribBoundary id={match.contribution.id} variant="pane">
          <RouteParamsProvider params={match.params}>
            <ContributionOutlet contribution={match.contribution} />
          </RouteParamsProvider>
        </ContribBoundary>
      </div>
    </PageLayout>
  )
}
