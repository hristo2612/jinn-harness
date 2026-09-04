import { useState, type FormEvent } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { profileAdmin, type AdministeredWire } from "@/lib/profile-admin"
import { cn } from "@/lib/utils"
import { CONTROL_CLASS } from "../shared"
import { PLUGIN_INVENTORY_KEY } from "./inventory"
import type { CatalogRow } from "./plugin-row"

/**
 * Pin-bump 10 (jinnd M2-K23, FINDINGS #37 closed at `f8b285b`; UI-2 plan
 * §9.5): the four #37 pills that were disabled on every extension row are
 * live actions, each ONE `jinn:profile-admin` write through the transport —
 * install (add an entry with its grants), remove, widen topics (a grants
 * change, applied through the restart), swap engine (package + hash). The
 * fifth, disable, is the row's switch. A refusal is rendered inline in the
 * kernel's words, typed with its class; nothing is retried or reworded.
 */

type Mode = "install" | "remove" | "widen" | "swap"

const ACTIONS: ReadonlyArray<{ mode: Mode; label: string; title: string }> = [
  { mode: "install", label: "Install", title: "Add an entry like this one, with its grants (add-entry)" },
  { mode: "remove", label: "Remove", title: "Remove this entry — a leaf, withdrawn on the record (remove-entry)" },
  { mode: "widen", label: "Widen topics", title: "Widen this entry's grants; applied through its restart (set-grants)" },
  { mode: "swap", label: "Swap engine", title: "Re-pin this entry's package and hash (swap-plugin)" },
]

// The settings page's own pill button (34 px, no hairline at rest) and its
// control recipe (`CONTROL_CLASS`, the accent focus ring its only ring).
const PILL =
  "inline-flex h-[34px] cursor-pointer items-center rounded-full border-none bg-[var(--fill-tertiary)] px-4 text-[length:var(--text-footnote)] font-[var(--weight-medium)] text-[var(--text-secondary)] transition-colors hover:bg-[var(--fill-secondary)] hover:text-[var(--text-primary)]"

const INPUT = cn(CONTROL_CLASS, "h-[34px] min-w-0 flex-1 font-[family-name:var(--font-code)]")

function splitList(text: string): string[] {
  return text
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean)
}

/** One write, then the inventory re-read whatever the answer was. */
export function useAdminister() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (run: () => Promise<AdministeredWire>) => run(),
    onSettled: () => void qc.invalidateQueries({ queryKey: PLUGIN_INVENTORY_KEY }),
  })
}

function Field({
  id,
  label,
  value,
  onChange,
}: {
  id: string
  label: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <span className="flex min-w-0 items-center gap-2">
      <label htmlFor={id} className="w-[64px] flex-none text-[length:var(--text-caption1)] text-[var(--text-tertiary)]">
        {label}
      </label>
      <input id={id} className={INPUT} value={value} onChange={(event) => onChange(event.target.value)} spellCheck={false} />
    </span>
  )
}

function defaults(mode: Mode, plugin: CatalogRow): Record<string, string> {
  const grants = JSON.stringify(plugin.grants ?? [])
  switch (mode) {
    case "install":
      return { id: `${plugin.id}-copy`, package: plugin.package ?? "", hash: "", grants, config: '{"data":{}}' }
    case "swap":
      return { package: plugin.package ?? "", hash: "" }
    case "widen":
      return { topics: "" }
    case "remove":
      return {}
  }
}

/** The write a submitted form is, built from its fields. */
function writeOf(mode: Mode, plugin: CatalogRow, fields: Record<string, string>): () => Promise<AdministeredWire> {
  switch (mode) {
    case "install":
      return () =>
        profileAdmin.addEntry({
          id: fields.id,
          package: fields.package,
          hash: fields.hash,
          grants: JSON.parse(fields.grants || "[]") as unknown[],
          config: JSON.parse(fields.config || "{}") as Record<string, unknown>,
        })
    case "remove":
      return () => profileAdmin.removeEntry(plugin.id)
    case "widen": {
      const held = (plugin.grants ?? []).slice()
      for (const topic of splitList(fields.topics)) if (!held.includes(topic)) held.push(topic)
      return () => profileAdmin.setGrants(plugin.id, held)
    }
    case "swap":
      return () => profileAdmin.swapPlugin(plugin.id, fields.package, fields.hash)
  }
}

function FormFields({
  mode,
  plugin,
  fields,
  setFields,
}: {
  mode: Mode
  plugin: CatalogRow
  fields: Record<string, string>
  setFields: (fields: Record<string, string>) => void
}) {
  const field = (name: string, label: string) => (
    <Field
      id={`plugin-${plugin.id}-${name}`}
      label={label}
      value={fields[name] ?? ""}
      onChange={(value) => setFields({ ...fields, [name]: value })}
    />
  )
  switch (mode) {
    case "install":
      return (
        <>
          {field("id", "Id")}
          {field("package", "Package")}
          {field("hash", "Hash")}
          {field("grants", "Grants")}
          {field("config", "Config")}
        </>
      )
    case "swap":
      return (
        <>
          {field("package", "Package")}
          {field("hash", "Hash")}
        </>
      )
    case "widen":
      return field("topics", "Topics")
    case "remove":
      return (
        <span className="text-[length:var(--text-caption1)] text-[var(--text-secondary)]">
          Remove {plugin.name}? Its fiber is withdrawn on the record; the inverse write stays on the ledger.
        </span>
      )
  }
}

function Form({
  mode,
  plugin,
  fields,
  setFields,
  onSubmit,
  onCancel,
  busy,
}: {
  mode: Mode
  plugin: CatalogRow
  fields: Record<string, string>
  setFields: (fields: Record<string, string>) => void
  onSubmit: (event: FormEvent) => void
  onCancel: () => void
  busy: boolean
}) {
  return (
    <form
      data-testid={`plugin-action-form-${plugin.id}`}
      onSubmit={onSubmit}
      className="mt-1 flex basis-full flex-col gap-1.5 rounded-[10px] bg-[var(--fill-quaternary)] px-3 py-2"
    >
      <FormFields mode={mode} plugin={plugin} fields={fields} setFields={setFields} />
      <span className="flex gap-2 pt-0.5">
        <button type="submit" disabled={busy} className={`${PILL} bg-[var(--accent-fill)] text-[var(--accent)]`}>
          Apply
        </button>
        <button type="button" onClick={onCancel} className={PILL}>
          Cancel
        </button>
      </span>
    </form>
  )
}

/** The four pills; the open one reads pressed. */
function ActionPills({ id, mode, onPick }: { id: string; mode: Mode | null; onPick: (mode: Mode) => void }) {
  return (
    <span data-testid={`plugin-actions-${id}`} className="flex flex-wrap items-center gap-1.5 pt-0.5">
      {ACTIONS.map((action) => (
        <button
          key={action.mode}
          type="button"
          title={action.title}
          aria-pressed={mode === action.mode}
          onClick={() => onPick(action.mode)}
          className={PILL}
        >
          {action.label}
        </button>
      ))}
    </span>
  )
}

/** The four live actions on one row, the open form, and the refusal. */
export function RowActions({ plugin, refusal }: { plugin: CatalogRow; refusal?: string }) {
  const [mode, setMode] = useState<Mode | null>(null)
  const [fields, setFields] = useState<Record<string, string>>({})
  const administer = useAdminister()

  const pick = (next: Mode) => {
    if (mode === next) return setMode(null)
    administer.reset()
    setFields(defaults(next, plugin))
    setMode(next)
  }
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (mode) administer.mutate(writeOf(mode, plugin, fields), { onSuccess: () => setMode(null) })
  }
  const message = administer.error instanceof Error ? administer.error.message : refusal

  return (
    <>
      <ActionPills id={plugin.id} mode={mode} onPick={pick} />
      {mode && (
        <Form
          mode={mode}
          plugin={plugin}
          fields={fields}
          setFields={setFields}
          onSubmit={submit}
          onCancel={() => setMode(null)}
          busy={administer.isPending}
        />
      )}
      {message && (
        <span
          role="alert"
          data-testid={`plugin-refusal-${plugin.id}`}
          className="basis-full text-[length:var(--text-caption1)] leading-[1.4] text-[var(--system-red)]"
        >
          {message}
        </span>
      )}
    </>
  )
}
