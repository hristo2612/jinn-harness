// The experiment shapes are declared once, in the workspace package both the
// gateway and this app depend on, so the two cannot drift apart. Only the
// response envelopes — which belong to these routes, not to the domain — are
// declared here.
import type { HydratedExperiment } from "@jinn/gateway-events"

export type {
  ExperimentMetric,
  ExperimentReading,
  ExperimentVerdict,
  HydratedExperiment as Experiment,
} from "@jinn/gateway-events"

export interface ExperimentsResponse {
  experiments: HydratedExperiment[]
}

export interface ExperimentResponse {
  experiment: HydratedExperiment
}
