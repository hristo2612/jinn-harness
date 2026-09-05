# A reason a finger can read

PLA-356, round 3 — the COO's escalation resolution (one fix-forward round,
one hunk in `web/src/components/chat/mobile-tab-bar.tsx` plus its test).
Round 2 (`2026-09-05-a-reason-a-finger-can-reach.md`) put an absent tab's
reason where a finger reaches it: a visible caption, the control's
accessible description, focusable. The verifier's live DOM then measured
the caption at **1.46:1 dark / 1.58:1 light**: it inherited
`--text-tertiary` (alpha 0.38 / 0.56) and sat under the disabled tab's
`opacity-40`, so the two multiplied. A reason a finger can reach but
cannot read is not delivered (Taste §2).

## What changed

Only the glyph dims now. The caption is outside the opacity-reduced
subtree and carries `--text-secondary` itself:

- the `opacity-40` moved from the disabled tab to a wrapper around its
  glyph, so the dimmed part is the icon alone;
- the caption's class list gained `text-[var(--text-secondary)]`, the
  palette's existing secondary token — no colour was invented and nothing
  hard-coded;
- the maintained comment no longer calls the bar "icon-only"; it states
  that an absent tab renders the caption, that only the glyph dims, and why.

Everything else at `a5b546d` stands: the caption's text, id,
`aria-describedby`, `tabIndex`, the inert tap, the 78 × 60 target.

## Red, then green

The proof extends the existing 390 px test. It reads the two theme blocks
of `web/src/routes/globals.css` (the palette's one home), takes the
caption's text token (its own, else the tab's inherited one), multiplies
its alpha by every `opacity-40` between it and the nav, composites that
over `--material-thick-opaque`, and asserts WCAG contrast ≥ 4.5:1 in both
themes; then, structurally, that no ancestor up to the nav carries an
`opacity-` class and the caption names the secondary token. Proof commit
first, on `a5b546d`; its tail reproduces the verifier's number:

```
AssertionError: dark caption contrast: expected 1.4608957269128413 to be greater than or equal to 4.5
 Test Files  1 failed (1)
      Tests  1 failed | 3 passed (4)
```

The fix commit second. The two tab-bar suites after it:

```
 Test Files  2 passed (2)
      Tests  11 passed (11)
```

Token-derived contrasts, before → after: dark 1.46 → **6.19**, light
1.58 → **5.10**.

## Browser pass, 390 × 844, both themes

The PINNED daemon (`target/composition/pinned-jinnd`, `f8b285b`) booted by
hand in the operator layout over a fresh `ui` kit built at the fix head
(`cargo run -p ui-kit -- kit … --port <free>`), the browser on a throwaway
profile paired through the pairing screen with the root's own credential,
`set device "iPhone 14"` (390 × 844), a real `tap` on Todos. The contrast
is computed on the live DOM the way the browser paints it: the caption's
computed colour, its alpha multiplied by every ancestor's computed opacity
up to the nav, composited over the nav's computed background.

| theme | when | url | visibleReason | focused | caption colour | opacity chain | glyph opacity | composite | contrast |
|---|---|---|---|---|---|---|---|---|---|
| dark | before tap | `/settings` | `true` | `null` | `rgba(232,228,216,0.62)` | none | 0.4 | `151,148,140` on `20,18,15` | **6.19** |
| dark | after tap | `/settings` | `true` | `Todos` | same | none | 0.4 | same | **6.19** |
| light | before tap | `/settings` | `true` | `null` | `rgba(33,30,22,0.66)` | none | 0.4 | `105,102,93` on `244,241,232` | **5.10** |
| light | after tap | `/settings` | `true` | `Todos` | same | none | 0.4 | same | **5.10** |

Todos measured 78 × 60, the caption 78 × 24, `tabindex=0`,
`aria-disabled=true` in every row. A tap on Plugins afterwards landed on
`/settings/plugins`. The nav's computed background was the
`--material-thick` variant (alpha 0.96 / 0.97 over `--bg`, which is within
one unit of it), so the composite against it and against the opaque token
agree to two decimals. Two screenshots are attached to the Todo:
`r3-mobile-390-{dark,light}-settings`. The daemon was stopped by its own
recorded pid; the throwaway profile was removed.

## Meter

Ruled: 140 net TypeScript, binding (wic_fd8661feeeb0). This round's
production delta (`git diff --numstat a5b546d..HEAD -- web/src`, tests
excluded): **+12 −9 = 3 net**, all in `mobile-tab-bar.tsx` — the comment
+7 −4, the control +5 −5 (one class moved to a one-line glyph wrapper, one
class added to the caption). Rust 0. Running total against `main`:
136 + 3 = **139 net**, within the binding 140.
