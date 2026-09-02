import { useMemo, useState, type FormEvent } from "react"
import { KeyRound, ShieldCheck, Wifi } from "lucide-react"
import type { AuthState } from "@/lib/auth"
import { AuthStateIcon, AuthStateLabel } from "./auth-motion"

type PairingMode = "code" | "token"

interface PairingScreenProps {
  authState: Partial<AuthState> | null
  pairing: boolean
  error?: string | null
  onPair: (secret: string, mode: PairingMode) => void
}

/** UI-1 (docs/plans/ui-malleability-arc.md §4.2 item 2): the daemon's door is
 *  bearer-only, so the one way in is to paste the operator credential — the
 *  contents of `<data>.operator-token` beside the daemon's data root. No code
 *  mode, no QR, no devices. */
export function PairingScreen({ authState, pairing, error, onPair }: PairingScreenProps) {
  const [code, setCode] = useState("")
  const networkLabel = authState?.networkExposed ? "Private network" : "Local gateway"
  const visibleError = useMemo(() => {
    if (!error) return null
    return /expired|invalid|not accepted/i.test(error)
      ? `${error}. The credential is the contents of the file <data>.operator-token beside the daemon's data root — paste it whole.`
      : error
  }, [error])
  const errorId = visibleError ? "jinn-pairing-error" : undefined

  function submit(e: FormEvent) {
    e.preventDefault()
    const trimmed = code.trim()
    if (!trimmed || pairing) return
    onPair(trimmed, "token")
  }

  return (
    <main className="h-dvh overflow-y-auto bg-[var(--bg)] text-[var(--text-primary)] flex items-start sm:items-center justify-center px-[var(--space-4)] py-[max(var(--safe-top),var(--space-8))]">
      <section className="w-full max-w-[560px] rounded-[var(--radius-xl)] bg-[var(--material-regular)] shadow-[var(--shadow-card)] p-[var(--space-6)]">
        <div className="mb-[var(--space-5)] flex items-center gap-[var(--space-3)] px-[var(--space-4)]">
          <div className="size-11 rounded-full bg-[var(--accent-fill)] text-[var(--accent)] flex items-center justify-center">
            <ShieldCheck size={22} />
          </div>
          <div>
            <h1 className="text-balance text-[length:var(--text-title3)] font-[var(--weight-semibold)] tracking-[var(--tracking-normal)]">
              Pair This Browser
            </h1>
            <div className="mt-1 inline-flex items-center gap-1.5 text-[length:var(--text-caption1)] text-[var(--text-tertiary)]">
              <Wifi size={13} />
              {networkLabel}
            </div>
          </div>
        </div>

        <div className="mb-[var(--space-5)] rounded-[var(--radius-lg)] bg-[var(--bg-secondary)] p-[var(--space-4)]">
          <p className="text-pretty text-[length:var(--text-subheadline)] leading-[var(--leading-relaxed)] text-[var(--text-secondary)]">
            This browser is not paired yet. Paste the operator credential to continue.
          </p>
          <div id="jinn-pair-credential-flow" className="mt-[var(--space-3)] rounded-[var(--radius-md)] bg-[var(--fill-tertiary)] px-[var(--space-3)] py-[var(--space-3)] shadow-[inset_0_0_0_1px_var(--separator)] text-[length:var(--text-footnote)] leading-[var(--leading-relaxed)] text-[var(--text-secondary)]">
            <ol className="flex flex-col gap-1.5 text-pretty">
              <li>1. On the computer running the daemon, open the file beside its data root:</li>
              <li>
                2. <span className="font-[var(--font-code)] text-[var(--text-primary)]">&lt;data&gt;.operator-token</span>
              </li>
              <li>3. Copy its contents and paste them below.</li>
            </ol>
          </div>
          <p className="mt-[var(--space-3)] text-pretty text-[length:var(--text-caption1)] text-[var(--text-tertiary)]">
            The credential is held for this tab only and is sent as a bearer token on every request.
          </p>
        </div>

        <form onSubmit={submit} className="flex flex-col gap-[var(--space-3)] rounded-[var(--radius-lg)] bg-[var(--bg-secondary)] p-[var(--space-4)]">
          <label className="text-[length:var(--text-caption1)] font-[var(--weight-semibold)] text-[var(--text-tertiary)]" htmlFor="jinn-pairing-code">
            Operator credential
          </label>
          <div className="flex h-12 w-full items-center gap-2 rounded-[var(--radius-md)] bg-[var(--fill-tertiary)] px-3 shadow-[inset_0_0_0_1px_var(--separator)] transition-[box-shadow] duration-150 [transition-timing-function:var(--ease-smooth)] focus-within:shadow-[inset_0_0_0_1px_var(--accent),0_0_0_4px_var(--accent-fill)] sm:h-11">
            <KeyRound size={16} className="shrink-0 text-[var(--text-tertiary)]" />
            <input
              id="jinn-pairing-code"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              type="password"
              autoComplete="off"
              spellCheck={false}
              aria-invalid={Boolean(visibleError)}
              aria-describedby={errorId}
              className="min-w-0 flex-1 bg-transparent text-[length:var(--text-body)] font-[var(--font-code)] tracking-[0.06em] text-[var(--text-primary)] outline-none placeholder:text-[var(--text-tertiary)]"
              placeholder="Paste the operator credential"
              disabled={pairing}
            />
          </div>

          {visibleError && (
            <div id={errorId} role="alert" aria-live="polite" className="animate-auth-reveal rounded-[var(--radius-md)] bg-[color-mix(in_srgb,var(--system-red)_12%,transparent)] px-[var(--space-3)] py-[var(--space-2)] text-[length:var(--text-footnote)] text-[var(--system-red)]">
              {visibleError}
            </div>
          )}

          <button
            type="submit"
            disabled={pairing || code.trim().length === 0}
            aria-label={pairing ? "Pairing browser" : "Pair Browser"}
            className="mt-[var(--space-2)] inline-flex h-11 items-center justify-center gap-2 rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-4)] text-[length:var(--text-subheadline)] font-[var(--weight-semibold)] text-[var(--accent-contrast)] transition-[transform,filter,opacity] duration-150 [transition-timing-function:var(--ease-snappy)] hover:brightness-[1.04] active:scale-[0.96] disabled:opacity-55 disabled:hover:brightness-100 disabled:active:scale-100"
          >
            <AuthStateIcon busy={pairing} idleIcon={KeyRound} size={16} />
            <AuthStateLabel busy={pairing} idle="Pair Browser" busyText="Pairing..." className="min-w-[6.75rem]" />
          </button>
        </form>
      </section>
    </main>
  )
}
