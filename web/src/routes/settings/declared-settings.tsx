import { useState } from "react"
import type { DeclaredNamespace, DeclaredProperty } from "@/lib/api-config"
import { CONTROL_CLASS, FieldRow, Section, SettingsInput, ToggleSwitch } from "./shared"

/**
 * UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 1, §8 amendment 4): the
 * settings this profile declares, rendered from the namespace schema the seam
 * answered and from nothing else. One `Section` per namespace, one `FieldRow`
 * per declared property, the control chosen by the property's `kind`. A
 * `secret-ref` is shown and never edited: the seam owns secrets, and this page
 * neither reads their value nor writes one.
 */

interface FieldProps {
  name: string
  property: DeclaredProperty
  value: unknown
  onCommit: (value: unknown) => void
}

function recordOf(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {}
}

/** The number the text commits as, or null when it is not one the kind takes: an
 *  `integer` is a whole number and never negative. */
function numberOf(text: string, integer: boolean): number | null {
  if (text.trim() === "") return null
  const parsed = Number(text)
  if (!Number.isFinite(parsed)) return null
  if (integer && (!Number.isInteger(parsed) || parsed < 0)) return null
  return parsed
}

function NumberField({ name, property, value, onCommit }: FieldProps) {
  const integer = property.kind === "integer"
  return (
    <SettingsInput
      type="number"
      value={typeof value === "number" ? String(value) : ""}
      min={integer ? 0 : undefined}
      ariaLabel={name}
      onChange={(text) => {
        const parsed = numberOf(text, integer)
        if (parsed !== null) onCommit(parsed)
      }}
    />
  )
}

/** Whether parsed JSON has the shape the kind names. */
function fitsKind(parsed: unknown, kind: string): boolean {
  if (kind === "array") return Array.isArray(parsed)
  return parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)
}

/** Pretty JSON, committed on blur and only when it parses to the kind's shape;
 *  otherwise a hint stays under the field and nothing is written. */
function JsonField({ name, property, value, onCommit }: FieldProps) {
  const [draft, setDraft] = useState(() => JSON.stringify(value ?? (property.kind === "array" ? [] : {}), null, 2))
  const [hint, setHint] = useState<string | null>(null)
  function commit() {
    let parsed: unknown
    try {
      parsed = JSON.parse(draft)
    } catch {
      setHint("Not valid JSON, so it was not saved.")
      return
    }
    if (!fitsKind(parsed, property.kind)) {
      setHint(`Not ${property.kind === "array" ? "an array" : "an object"}, so it was not saved.`)
      return
    }
    setHint(null)
    onCommit(parsed)
  }
  return (
    <div>
      <textarea
        value={draft}
        aria-label={name}
        spellCheck={false}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        className={`${CONTROL_CLASS} min-h-[96px] resize-y font-mono`}
      />
      {hint && (
        <div className="mt-[var(--space-1)] text-[length:var(--text-caption1)] text-[var(--system-red)]">{hint}</div>
      )}
    </div>
  )
}

function SecretRefField({ value }: { value: unknown }) {
  const secret = recordOf(value).$secret
  return (
    <div className="text-[length:var(--text-footnote)] text-[var(--text-tertiary)]">
      secret reference{typeof secret === "string" && secret ? `: ${secret}` : ""}
    </div>
  )
}

function DeclaredField(props: FieldProps) {
  const { name, property, value, onCommit } = props
  switch (property.kind) {
    case "bool":
      return <ToggleSwitch checked={value === true} ariaLabel={name} onChange={onCommit} />
    case "integer":
    case "number":
      return <NumberField {...props} />
    case "string":
      return <SettingsInput value={typeof value === "string" ? value : ""} ariaLabel={name} onChange={onCommit} />
    case "array":
    case "object":
      // Keyed on the value so a reload, or the commit itself, resets the draft.
      return <JsonField key={JSON.stringify(value)} {...props} />
    case "secret-ref":
      return <SecretRefField value={value} />
    default:
      return (
        <div className="text-[length:var(--text-footnote)] text-[var(--text-tertiary)]">
          {property.kind}: not editable here
        </div>
      )
  }
}

export function DeclaredSettings({
  config,
  declared,
  onChange,
}: {
  config: Record<string, unknown>
  declared: Record<string, DeclaredNamespace>
  onChange: (path: string[], value: unknown) => void
}) {
  return (
    <>
      {Object.entries(declared).map(([namespace, schema]) => (
        <Section key={namespace} title={namespace}>
          {Object.entries(schema.properties).map(([key, property]) => (
            <FieldRow key={key} label={property.required ? `${key} (required)` : key}>
              <DeclaredField
                name={key}
                property={property}
                value={recordOf(config[namespace])[key]}
                onCommit={(value) => onChange([namespace, key], value)}
              />
            </FieldRow>
          ))}
        </Section>
      ))}
    </>
  )
}
