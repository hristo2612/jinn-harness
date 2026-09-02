import { write } from './request'

export interface HostSessionSpawn {
  prompt: string
  /** An employee from the org. Omitted, the session runs on gateway defaults. */
  employee?: string
  engine?: string
  model?: string
}

export interface HostSession {
  id: string
  engine: string
  employee: string | null
  status: string
  title: string | null
}

export interface PluginHostSessions {
  spawn(request: HostSessionSpawn): Promise<HostSession>
}

export const sessions: PluginHostSessions = {
  spawn(spawnRequest) {
    return write<HostSession>('sessions.spawn', '/api/sessions', spawnRequest)
  },
}
