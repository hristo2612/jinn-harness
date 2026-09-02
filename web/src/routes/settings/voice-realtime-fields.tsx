/**
 * The rest of the `realtime` block, as fields.
 *
 * Split from `voice-section.tsx` so that file keeps to one story — the key,
 * which is the only setting on the page whose value it is never given. These
 * are ordinary config fields: what they show is what is in config.yaml.
 */
import type { TalkCapability } from "@/lib/talk-capability"
import { FieldRow, SettingsInput, SettingsSelect } from "./shared"

export type TurnDetection = string | { type?: string; [key: string]: unknown } | undefined

/**
 * The picker speaks names; config.yaml speaks the provider's own union, where
 * `semantic_vad` is only valid as a mapping because it carries an eagerness.
 * These two functions are the whole translation.
 */
export function turnDetectionName(value: TurnDetection): string {
  if (value === undefined) return ""
  if (typeof value === "string") return value
  return typeof value.type === "string" ? value.type : ""
}

export function turnDetectionValue(name: string): TurnDetection {
  if (!name) return undefined
  if (name === "semantic_vad") return { type: "semantic_vad" }
  return name
}

export function ModelField({
  model,
  onChange,
}: {
  model: string
  onChange: (path: string[], value: unknown) => void
}) {
  return (
    <FieldRow label="Model">
      <SettingsInput
        value={model}
        onChange={(value) => onChange(["realtime", "model"], value)}
        placeholder="Provider default"
        ariaLabel="Realtime model"
      />
    </FieldRow>
  )
}

/** Fed by the gateway, which asks the configured provider. An unknown provider
 *  offers nothing, so the field says so instead of showing an empty menu. */
export function VoiceField({
  voice,
  capability,
  onChange,
}: {
  voice: string
  capability: TalkCapability | null
  onChange: (path: string[], value: unknown) => void
}) {
  const voices = capability?.voices ?? []
  if (voices.length === 0) {
    return (
      <FieldRow label="Voice">
        <SettingsInput
          value={voice}
          onChange={(value) => onChange(["realtime", "voice"], value)}
          placeholder="Set a provider to choose a voice"
          ariaLabel="Realtime voice"
        />
      </FieldRow>
    )
  }
  return (
    <FieldRow label="Voice">
      <SettingsSelect
        ariaLabel="Realtime voice"
        value={voice}
        onChange={(value) => onChange(["realtime", "voice"], value)}
        options={[
          { value: "", label: "Provider default" },
          ...voices.map((name) => ({ value: name, label: name })),
        ]}
      />
    </FieldRow>
  )
}

export function TurnDetectionField({
  turnDetection,
  onChange,
}: {
  turnDetection: TurnDetection
  onChange: (path: string[], value: unknown) => void
}) {
  return (
    <FieldRow label="Turn detection">
      <SettingsSelect
        ariaLabel="Realtime turn detection"
        value={turnDetectionName(turnDetection)}
        onChange={(name) => onChange(["realtime", "turnDetection"], turnDetectionValue(name))}
        options={[
          { value: "", label: "Provider default" },
          { value: "semantic_vad", label: "Semantic — waits for a finished thought" },
          { value: "server_vad", label: "Silence — waits for a pause" },
          { value: "none", label: "Off — nothing ends a turn on its own" },
        ]}
      />
    </FieldRow>
  )
}

/**
 * What used to be the browser-local "Microphone" toggle.
 *
 * It was the same `near_field | far_field` enum as `realtime.noiseReduction`,
 * kept in localStorage where the provider could never read it — so the setting
 * that looked like it filtered the microphone did nothing to the session. This
 * is the one the provider is actually told about.
 */
export function NoiseReductionField({
  noiseReduction,
  onChange,
}: {
  noiseReduction: string
  onChange: (path: string[], value: unknown) => void
}) {
  return (
    <FieldRow label="Microphone">
      <SettingsSelect
        ariaLabel="Realtime noise reduction"
        value={noiseReduction}
        onChange={(value) => onChange(["realtime", "noiseReduction"], value || undefined)}
        options={[
          { value: "", label: "Provider default" },
          { value: "far_field", label: "Laptop or room mic" },
          { value: "near_field", label: "Headset or close mic" },
        ]}
      />
    </FieldRow>
  )
}
