# A reason a finger can reach

PLA-356, round 2. Round 1 (`2026-09-05-a-surface-is-what-the-bundle-routes.md`)
made the rail derive from the route table and left an absent destination in
its slot, disabled, with `not in this profile` as its reason. On the desktop
rail that reason is the row's hover pill: real text a pointer reveals. On the
mobile bar the same reason lived only in a `title` on an unfocusable `<span>`
— a hover-ism no touch ever sees. The verifier's 390 px transcript said so:
`{"url":"/settings","visibleReason":false,"focused":null}`. Taste §2, mobile
is first-class; the COO ruled the fix a one-file delta inside
`web/src/components/chat/mobile-tab-bar.tsx` plus its test.

## What changed

An absent tab now says why where a finger and a keyboard reach it:

- the reason is visible text, a two-line 10 px caption under the glyph
  reading `not in this profile` — the one text the bar carries; live tabs
  stay icons-only (HIG icons-over-labels, GRS-022, unchanged);
- the caption is the control's accessible description
  (`aria-describedby` → the caption's id); the accessible name stays the
  destination (`aria-label="Todos"`), so a screen reader hears "Todos, link,
  unavailable, not in this profile";
- the control is focusable (`tabIndex=0`) though `aria-disabled`, so a switch
  or a keyboard reaches the reason; a tap or a key navigates nowhere (no
  `href`, no handler);
- the `title` stays as a pointer extra; it is never the reason.

No pill chrome on the mobile caption: the desktop pill's 0.5 px hairline is
hover-only, and a resting hairline on three tabs would break "no hairlines at
rest". A pill reading the full phrase is also 106 px wide against a 78 px
slot, so three adjacent ones would overlap; the caption wraps instead.

The file's maintained comment now states what the bar renders: the primary
slots provided or not, then the surfaces this bundle routes where More would
sit (Settings and Plugins at the shipped table), no More tab, nothing
redirects, and how an absent tab explains itself.

## Red, then green

Proof commit first (`test(web): red proof — an absent mobile tab shows its
reason as visible text, focusable, a tap stays put at 390 px`), on the
round-1 head `da7dbd8`:

```
TestingLibraryElementError: Unable to find an element with the text: not in this profile.
 Test Files  1 failed (1)
      Tests  1 failed | 3 passed (4)
```

Implementation commit second. The tab-bar suites after it:

```
 Test Files  5 passed (5)
      Tests  71 passed (71)
```

Full web run at the implementation head: `Test Files 105 passed (105)`,
`Tests 864 passed (864)` (863 + this proof); typecheck and lint exit 0;
ratchet `346 files scanned, 28 baselined files, 3993 budgeted lines (limit
300)`.

## Browser pass, 390 × 844

On the PINNED daemon (`target/composition/pinned-jinnd`, `f8b285b`) booted by
hand from a fresh copy of the `ui` kit rebuilt at this head, on a free
loopback port, the browser on a throwaway profile paired through the pairing
screen with the root's own credential; `agent-browser set device "iPhone 14"`
(390 × 844, touch), and a real `tap` on the Todos tab. The same measurement
before and after the tap, in each theme (the JSON, abridged to the fields the
ruling names):

| theme | when | url | visibleReason | focused | Todos box | caption box | tabindex | aria-disabled |
|---|---|---|---|---|---|---|---|---|
| light | before tap | `/settings` | `true` | `null` | 78 × 60 | 78 × 24 | `0` | `true` |
| light | after tap | `/settings` | `true` | `Todos` | 78 × 60 | 78 × 24 | `0` | `true` |
| dark | before tap | `/settings` | `true` | `null` | 78 × 60 | 78 × 24 | `0` | `true` |
| dark | after tap | `/settings` | `true` | `Todos` | 78 × 60 | 78 × 24 | `0` | `true` |

`visibleReason` is computed on the live DOM: the element `aria-describedby`
names exists, has a non-zero box inside the viewport, is neither
`display:none` nor `visibility:hidden`, carries no `aria-hidden`, and its
text is exactly `not in this profile`. Every tab measured 78 × 60 (the bar
grew from 49 to 60 with the caption; the ruling's floor is 34). Chat, Todos
and Workflows carry the caption text; Settings (`aria-current="page"`) and
Plugins carry none. `element.focus()` on the tab then
`document.activeElement === tab` answered `true` in both themes. Two
screenshots are attached to the Todo: `r2-mobile-390-{light,dark}-settings`.

## Meter

Estimate for this round was the ruling's own "a few lines" of TypeScript;
the production delta (`git diff --numstat da7dbd8..HEAD -- web/src`, tests
excluded) is **+30 −8 = 22 net**, all in `mobile-tab-bar.tsx` — a miss
against that phrase, reported with its breakdown: the comment rewrite +7
(the Minor), the control +15 (one attribute per line on the disabled span,
plus the three-line caption). Rust 0. Running total against `main`:
+163 −27 = 136 net against round 1's ≤ 120 estimate (114 at round 1, ruled
within). The file is 137 lines.

## Out of scope, unchanged

The global search's static pages still offer the absent surfaces from Cmd-K
(named in the round-1 note). The desktop rail is untouched.
