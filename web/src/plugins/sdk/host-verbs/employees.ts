import { request } from './request'

export interface HostEmployee {
  name: string
  displayName: string
  department: string
  rank: string
  engine: string
  model: string
  /** The first line of the persona, when it is short enough to be a label. */
  role?: string
}

export interface PluginHostEmployees {
  list(): Promise<HostEmployee[]>
}

export const employees: PluginHostEmployees = {
  async list() {
    const org = await request<{ employees: HostEmployee[] }>('employees.list', '/api/org')
    return org.employees
  },
}
