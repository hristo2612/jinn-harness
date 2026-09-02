/**
 * The shapes that travel over the gateway → browser wire. Types only, no
 * behaviour: index.ts owns the event protocol that carries them and the
 * runtime that decodes it.
 *
 * A shape lives here rather than on either side because both sides must agree
 * on it. Declaring it twice is how a gateway and a browser drift apart.
 */

export type JsonPrimitive = string | number | boolean | null
export type JsonValue = JsonPrimitive | JsonObject | JsonValue[]
export interface JsonObject { [key: string]: JsonValue }

export interface MessageMediaWire {
  type: "image" | "audio" | "video" | "file"
  url: string
  name?: string
  mimeType?: string
  size?: number
}

export interface TalkProactiveUiEffect {
  type: "refresh" | "highlight"
  target: string
}

export interface TalkProactiveCuePayload {
  receiptId: string
  talkSessionId: string
  topicId: string | null
  disposition: "quiet" | "spoken"
  urgency: "routine" | "urgent"
  summary: string
  uiEffect: TalkProactiveUiEffect | null
}

export type CompanyChangedEvent =
  | { entity: "todo"; action: string; id: string; sessionId?: string; version: number; value?: JsonObject }
  | { entity: "workflow-definition"; id: string; revision: number }
  | { entity: "workflow-run"; workflowId: string; runId: string }

export interface ExperimentMetric {
  name: string
  unit?: string
  howToMeasure: string
}

export interface ExperimentReading {
  id: string
  experimentId: string
  at: string
  metric: string
  value: number
  note?: string
}

export interface ExperimentVerdict {
  outcome: "win" | "loss" | "inconclusive"
  note: string
  concludedAt: string
}

/** The stored experiment. Every read returns a {@link HydratedExperiment}. */
export interface Experiment {
  id: string
  name: string
  hypothesis: string
  status: "running" | "concluded"
  startedAt: string
  horizonDays: number
  baseline: Record<string, number>
  metrics: ExperimentMetric[]
  readings: ExperimentReading[]
  verdict?: ExperimentVerdict
  checkInCronJobId?: string
  todoId?: string
  owner?: string
}

/** What every experiment read carries. The two horizon facts are derived from
 *  `startedAt` and `horizonDays` on each read rather than stored, so they are not
 *  part of the stored shape above. */
export interface HydratedExperiment extends Experiment {
  horizonEndsAt: string
  overdue: boolean
}
