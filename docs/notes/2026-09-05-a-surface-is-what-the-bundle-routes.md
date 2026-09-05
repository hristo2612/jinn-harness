# A surface is what the bundle routes — the rail, derived

*Harness packet UI-NAV-RAIL (PLA-356), on harness main `3bb6e4e` at pin
`f8b285b`; no pin change, no kernel change. Plan: `docs/plans/ui-malleability-arc.md`
§9.7 amendment 10, adaptation 15.*

## What the operator saw

"Nothing works aside from settings page." True: the ported rail
(`lib/nav.ts`, verbatim) listed ten destinations, nine of which landed on
the plugin splat and were sent to `/settings` without a word, and the one
other surface the bundle renders, `/settings/plugins`, was a link below
the fold of Settings. A rail that offers what it cannot deliver is a lie
told ten times per page.

## The one decision: which fact says a surface is provided

Two candidates were on the card. **The plugin catalog** (`GET
/v1/plugins/main`): the seams behind the pages. **The route table**
(`lib/app-routes.ts`, adaptation 5): the bundle's own statement of what
it renders. The route table won, for two reasons.

1. A surface is provided by the bundle that routes it, not by the seam its
   page reads. Settings renders the seam's absence itself (one caption,
   adaptation 1); a rail that hid Settings because the settings seam was
   down would hide the page that explains it.
2. The catalog is a reading at one instant, and a plugin cannot observe
   the composition, only poll it (FINDINGS #40). The rail would have to
   poll, flicker on every disposal, and show a loading state before its
   first byte. The route table is a fact of the bundle: known before the
   first render, changed only by a bundle swap, which is a restart.

So `lib/nav-provided.ts` reads the table: a route is a surface unless it
is a redirect (its `surface` names another route's — the descriptor's own
rule, not a naming convention) or the plugin splat. A contributed row is
provided by the plugin that contributed it. `nav.ts` stays verbatim and
stays the inventory; the derivation marks each row and adds one, Plugins,
after Settings.

The mutant the card asked for: replace the derivation with the two
hardcoded hrefs and three tests go red — the table that renders one more
surface, the table without the plugins page, the old gateway's full table
(where the mobile bar must come back verbatim).

## What "disabled" is

A `role="link"` with no `href`, `aria-disabled="true"`, `title="not in
this profile"`, the same 44 px row (mobile: the same ≥ 49 px tab), at 40 %
opacity, and its label pill reading `Todos · not in this profile` on
hover. It navigates nowhere: no `Link`, no handler, no prefetch. The
router's `/` and `/more` redirects stay — they answer a TYPED url, and no
rail item reaches them any more. On mobile the More screen is itself
absent, so the provided overflow surfaces take its slot as direct tabs;
a table that renders `/more` gives the verbatim four-tab bar back, and
the adapted verbatim tests prove it under that table.

The active cue is the longest matching href (`activeHref`), because
`/settings/plugins` would otherwise light Settings beside Plugins.

## Red first

The proofs commit precedes the implementation commit. Their tail on the
merge-base plus the proofs alone (`414868b`):

```
Error: Failed to resolve import "@/lib/nav-provided" from "src/components/__tests__/nav-ribbon-provided.test.tsx". Does the file exist?
Error: Failed to resolve import "@/lib/nav-provided" from "src/lib/__tests__/nav-provided.test.ts". Does the file exist?
Error: Failed to resolve import "@/lib/nav-provided" from "src/components/chat/__tests__/mobile-tab-bar-provided.test.tsx". Does the file exist?
 Test Files  3 failed (3)
      Tests  no tests
```

At the implementation (`d1a1b70`):

```
 Test Files  3 passed (3)
      Tests  17 passed (17)
```

The mutant (`renderedPaths` replaced by `new Set(["/settings",
"/settings/plugins"])`):

```
     × provides one more destination when the table renders one more surface 2ms
     × keeps Plugins listed but not provided when the table does not render it 0ms
     × gives the verbatim mobile bar back for a table that renders More 0ms
⎯⎯⎯⎯⎯⎯⎯ Failed Tests 3 ⎯⎯⎯⎯⎯⎯⎯
AssertionError: expected [ '/settings', '/settings/plugins' ] to deeply equal [ '/todos', '/settings', …(1) ]
AssertionError: expected true to be false // Object.is equality
AssertionError: expected [ '/', '/todos', '/workflow', …(2) ] to deeply equal [ '/', '/todos', '/workflow', '/more' ]
 Test Files  1 failed (1)
      Tests  3 failed | 6 passed (9)
```

## Meter

The UI-2 meter (plan §9 preamble) counts production Rust net on its path
list. This packet changes no Rust: **0** on the meter. The TypeScript tree
carries no line ceiling (its acceptance is the diff against the pinned
sha); the card's ESTIMATE was ≤ 120 production net, and the TypeScript
production delta (`git diff --numstat main -- web/src`, test directories
excluded) is **+135 −21 = 114 net**: `lib/nav-provided.ts` +70,
`lib/use-provided-navigation.ts` +10, `components/pill-nav.tsx` +23 −4,
`components/chat/mobile-tab-bar.tsx` +32 −17 — reported against the
estimate, not a ceiling. Every touched file is under 300 lines
(`pill-nav.tsx` 285, `mobile-tab-bar.tsx` 115).

## Out of scope, named

The global search's static pages (`components/global-search/static-pages.tsx`,
verbatim) still offer the absent surfaces from Cmd-K; they land on the
splat as before. Adjacent, reported, not fixed here (Taste §4).

## Browser pass

On the suite's own pinned daemon (`target/composition/pinned-jinnd`,
`f8b285b`) booted by hand from a fresh copy of the suite's `ui` kit on a
free loopback port, driven with `agent-browser` on a throwaway profile;
the browser paired through the pairing screen with the root's own
credential. Ten screenshots are attached to the Todo: `desktop-1440-{light,dark}-{settings,plugins,hover-todos}`
and `mobile-390-{light,dark}-{settings,plugins}`.

Measured on the live DOM, `/settings/plugins`, light, 1440 × 900 (the
rail, in order):

| item | href | aria-disabled | title | current | box | opacity |
|---|---|---|---|---|---|---|
| Chat … Skills (nine) | none | `true` | `not in this profile` | none | 44 × 44 | 0.4 |
| Settings | `/settings` | none | none | none | 44 × 44 | 1 |
| Plugins | `/settings/plugins` | none | none | `page` | 44 × 44 | 1 |

Hovering a disabled row shows its pill reading `Todos · not in this
profile` (the `hover-todos` screenshots). A click on the disabled Todos
row left the URL at `/settings`; a click on Plugins landed on
`/settings/plugins`; on it Plugins alone carries `aria-current`.

Mobile, 390 × 844, the fixed bar: Chat, Todos, Workflows `aria-disabled`
with the title, no href, 78 × 49, opacity 0.4; Settings and Plugins live
links, 78 × 49; a tap on Todos left `/settings`, a tap on Plugins landed
on `/settings/plugins` with Plugins alone current. No More tab.

One transcript note for the next verifier: a selector on
`nav[aria-label="Primary"]` matches the hidden desktop rail first at 390
px (a 0 × 0 anchor agent-browser reports as "covered"); the mobile bar is
`nav[aria-label="Primary"].fixed`.
