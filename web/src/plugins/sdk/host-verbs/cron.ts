import { request } from './request'

/** A cron job as the read tier exposes it. The prompt, the model and the
 *  delivery target are deliberately absent, exactly as they are from
 *  `GET /api/cron` — a plugin that can list jobs is not thereby able to read
 *  what they say. `lastRun` is dropped for a different reason: it costs the
 *  gateway a file read per job, the in-process half would have to repeat it,
 *  and `runs` already answers the question. */
export interface HostCronJob {
  id: string
  name: string
  schedule: string
  enabled: boolean
  employee: string | null
  engine: string | null
  timezone: string | null
}

/** One past fire of a cron job. Every field is optional because the gateway's
 *  summariser omits anything a run log did not record in the shape it expects,
 *  rather than inventing a value for it. */
export interface HostCronRun {
  id?: string
  jobId?: string
  timestamp?: string
  startedAt?: string
  finishedAt?: string
  sessionKey?: string
  status?: 'success' | 'error' | 'started' | 'skipped' | 'duplicate' | 'expired'
  exitCode?: number
  durationMs?: number
  duration?: string
}

export interface PluginHostCron {
  jobs(): Promise<HostCronJob[]>
  /** The most recent runs of one job, newest first; the gateway defaults to 20. */
  runs(jobId: string, limit?: number): Promise<HostCronRun[]>
}

/** The route also carries `lastRun`, which this contract does not. */
type CronJobWire = HostCronJob & { lastRun?: unknown }

export const cron: PluginHostCron = {
  async jobs() {
    const listed = await request<CronJobWire[]>('cron.jobs', '/api/cron')
    return listed.map(({ id, name, schedule, enabled, employee, engine, timezone }) => ({
      id,
      name,
      schedule,
      enabled,
      employee,
      engine,
      timezone,
    }))
  },
  runs(jobId, limit = 20) {
    return request<HostCronRun[]>(
      'cron.runs',
      `/api/cron/${encodeURIComponent(jobId)}/runs?limit=${limit}`,
    )
  },
}
