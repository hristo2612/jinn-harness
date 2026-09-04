# A shape is a write — and the swap window is a stated limit

*Harness pin-bump 10 (PLA-368) — adopting jinnd `f8b285b` (M2-K23
`jinn:profile-admin` 0.1.0 at `598a03c` plus its verifier lane, PR #27;
`jinn:profile` 0.2.0 → 0.3.0, one refusal; `wit/` byte-identical to
`cb08683`, so `jinn:plugin` stays 0.12.0 and the world does not move).
Kernel changes never here; this note is about what the harness could
finally do through the surface a person or an agent actually uses, and
what it still cannot.*

## What FINDINGS #37 said, and why the pin is the fix

Every seam from 2.3 onward proved its malleability the same way: a
provider is swapped by changing one entry's `package` and `hash` in the
profile FILE behind the daemon. The plugins seam (2.7) tried the same
swap through the operator API and found `jinn:profile.patch-entry`
writes one subtree — `config` — and nothing else, so through the surface
an operator actually has, a plugin's SHAPE (what it is, what it may
reach, whether it runs) was out of reach. The distribution's headline
claim was true only of an operator with filesystem access to the
document, and the plugins page said so on five disabled pills.

M2-K23 answers it from the kernel's side
(`kernel-pin/contracts/jinn-profile-admin/README.md`): a separate
`jinn:profile-admin` grant on the CALLING entry — whoever holds it is the
operator's delegate by the operator's own document — and five writes,
each applied BY THE LOADER as the plan step the document-led diff would
have produced for that entry, written back atomically, recorded as a
`ProfileAdministered { entry, by, write, before, after, prior }` row
under the caller BEFORE the commit (Law 2), and reversible by the
inverse write the row's `prior` carries. What the harness owes is the
grant on the transport, one route per write, the proofs, the page, and
the closure.

## The bump, in the order it happened

1. **Proofs first** (`b1c97b5`): five composition proofs, one per write,
   in `tests/composition/tests/profile_admin.rs`; the #37 proof in
   `plugins.rs` flipped and renamed; the api-kit grant test; the wire
   test; the web tests. Every one red at `cb08683` and against the
   transport as it stood — the failing tails are below.
2. **The pin** (`e885edb`): one commit, KERNEL-PIN.md + both hashes + the
   vendored surface from `git archive f8b285b`. `wit-hash` unchanged
   (the diff of `wit/` between `cb08683` and `f8b285b` is empty — checked
   by `git diff --stat`, not by the card's word); `contracts-hash` moves
   for the new bundle and `jinn:profile` 0.3.0. `cargo test -p
   harness-pin`: Gate 1 and Gate 2 both pass.
3. **The transport and the kits** (`7cb6b51`): `jinn_api::profile_admin`
   (the pure half: which write a body names, the wire, the answer with
   its class), `jinn-api-http`'s dispatch of the entries path before the
   static table, and the api kit's grant on the transport entry —
   `scope` WRITTEN, because a bare grant administers nothing.
4. **The page** (`36aefbb`): four pills open one inline form each, the
   switch is live, a refusal is rendered in the kernel's words.
5. **The closures** (this commit): FINDINGS #37 CLOSED at the pin with
   the transcript; #52 re-measured; README, plan amendment 9, this note.

## What the routes are, and the one decision they carry

`PATCH /v1/profile/entries/{id}` now carries EITHER a config patch
(`{config}`, `jinn:api-profile` as before) OR exactly one admin write
(`{disabled}`, `{grants}`, `{package, hash}`). A body naming two — or an
admin key beside `config` — is `invalid` before any kernel call: two
writes in one request would be two rows or none, and the answer could
name neither honestly. `POST /v1/profile/entries` takes the 0.2.0
`entry` record (`grants` beside `config`; the kernel mirrors them into
`config.grants`); `DELETE /v1/profile/entries/{id}` removes a leaf.

An accepted write answers `{id, write, administered-seq}` — the row's
sequence, the INTENT landed before the commit. The restart, spawn or
disposal it schedules is never awaited inside the call (R1): the proofs
follow it on the ledger. A refusal is `refused` with the kernel's class
verbatim (`unauthorized` | `malformed` | `conflict` | `irreversible`)
and `retryable` true only for `conflict`; an unresolvable
`jinn:profile-admin` is `refused` too (a grant the entry does not hold —
the profile's to widen), never `unavailable`.

The config route keeps its own law: `config.data` merges, and a
`grants` sent through it is forwarded as the merge it always was — the
0.3.0 kernel refuses it (`grants are jinn:profile-admin's`), and the
route surfaces that as it surfaces every `refused`. The proof asserts it
with nothing written: the entry's grants and incarnation unchanged, an
`AmendmentRefused` row on the record.

## The swap window — asserted in the shape the kernel pins

At this pin `swap-plugin` applies the loader's `Replace` as the kernel
has it: DISPOSE, then spawn. The old incarnation rests `Disposed` under
`ExplicitDispose` (never `Suspend`; no `FiberSuspended` row), its
journal withdrawn rather than inherited, its listens released outright;
the successor is a new fiber whose activation is not staged. A
reply-expecting walk between the two selects nobody — the #47 shape,
dropped as an honestly empty topic, never refused `restarting`. The
bundle README names this as the 0.1.0 limit, carded as jinnd M2-K27,
and the kernel's own daemon suite pins it
(`a_swap_disposes_the_old_incarnation_a_stated_limit_until_m2_k27`).

The flipped harness proof pins what the plugins fixture can pin: the
disposal's cause, the absence of a suspension, the successor as a
DIFFERENT fiber on the other package, and — between the old rest and
the successor's activation — ZERO `DispatchRefused` rows, every
`DispatchTrace` in the window named with its listener count in the
transcript. What it does NOT pin is the drop itself: this fixture lands
no reply-expecting walk on the swapped entry's topic inside the window
(the catalog provider listens on `jinn:introspect/transitions`, which
the kernel publishes emit-mode), so "selects nobody" is the kernel's
case, cited, and the harness asserts the window's shape rather than
claiming a walk it did not make. When M2-K27 lands the proof's window
assertion flips to `restarting` and the name loses its suffix.

## Red tails, per proof (the proofs commit alone, at the pin)

The composition proofs were run at `e885edb` (proofs + pin, the
implementation stashed), the same daemon build lane every green run
uses (`git archive f8b285b`, marker `target/composition/pinned-jinnd/.commit`):

```
$ JINND_DIR=… cargo test -p composition --test profile_admin -- --nocapture --test-threads=3
test an_entry_without_the_grant_is_refused_and_a_grants_widening_through_patch_entry_is_refused ... FAILED
  panicked at tests/composition/tests/profile_admin.rs:351:9: the admin grant was there to strip
  left: 7  right: 6        (no grant on the transport at the old kit — nothing to strip)
test set_disabled_through_the_api_disposes_then_spawns_and_self_administration_is_refused ... FAILED
  panicked at tests/composition/tests/profile_admin.rs:214:5
  left: 200  right: 502     (PATCH {disabled} on the transport's own entry: the config route answered 200 changed:false)
test add_entry_through_the_api_lands_the_row_and_the_entry_live ... FAILED
  panicked at tests/composition/tests/profile_admin.rs:90:5: HTTP/1.1 404 Not Found
  left: 404  right: 200     (POST /v1/profile/entries: a route miss)
test remove_entry_through_the_api_withdraws_it_on_the_record ... FAILED
  panicked at tests/composition/tests/profile_admin.rs:90:5: HTTP/1.1 405 Method Not Allowed
  left: 405  right: 200     (DELETE on a path shaped for PATCH only)
test set_grants_through_the_api_lands_only_via_the_restart ... FAILED
  panicked at tests/composition/tests/profile_admin.rs:91:5
  left: Null  right: "set-grants"   (PATCH {grants}: the config route's answer carries no `write`)
test result: FAILED. 0 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 77.64s
```

```
$ JINND_DIR=… cargo test -p composition --test plugins -- --nocapture --test-threads=2
FINDINGS #37 transcript — PATCH /v1/profile/entries/jinn-plugins-appliance {"package": "plugins/jinn-plugins-profile", "hash": …}
HTTP/1.1 200 OK
{"api-version":"0.4.0","changed":false,"entry":{…,"package":"plugins/jinn-plugins-static",…}}
thread 'the_operator_api_swaps_what_a_plugin_is_and_the_old_incarnation_is_disposed_until_m2_k27' panicked at tests/composition/tests/plugins.rs:525:5:
assertion `left == right` failed: … left: Null  right: "swap-plugin"
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 282.64s
```

The #37 shape exactly: the route works, the entry is patchable,
`changed: false`, and the package is what it was.

```
$ cargo test -p api-kit
failures: tests::the_transport_is_granted_profile_admin_over_every_entry
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo test -p jinn-api --test profile_admin_wire
error[E0432]: unresolved import `jinn_api::profile_admin`

$ pnpm vitest run src/routes/settings/plugins
 ❯ inventory.test.ts (4 tests | 3 failed)     "the operator API writes config only (FINDINGS #37 / KG-1, PLA-348)"
 ❯ plugin-row.test.tsx (7 tests | 6 failed)   Unable to find an element by: [data-testid="plugin-actions-ext-green"]
```

## Green, at the pin

The plugins suite and the admin suite at the pin, the same lane
(`JINND_DIR=… cargo test -p composition --test plugins --test
profile_admin --no-fail-fast -- --nocapture --test-threads=2`):

```
test the_operator_api_swaps_what_a_plugin_is_and_the_old_incarnation_is_disposed_until_m2_k27 ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 200.73s
     Running tests/profile_admin.rs
test add_entry_through_the_api_lands_the_row_and_the_entry_live ... ok
test an_entry_without_the_grant_is_refused_and_a_grants_widening_through_patch_entry_is_refused ... ok
test remove_entry_through_the_api_withdraws_it_on_the_record ... ok
test set_disabled_through_the_api_disposes_then_spawns_and_self_administration_is_refused ... ok
test set_grants_through_the_api_lands_only_via_the_restart ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 108.49s
```

The per-write transcripts — the answer, the row, the live effect — are
in FINDINGS #37's closure at this pin (one home). The swap's window line
as the proof printed it:

```
swap window rows 223..229: 5 rows, 0 DispatchRefused, traces ["\"jinn:introspect/transitions\" listeners 1"]
```

The one trace in the window is the kernel's emit-mode transitions
publication reaching the LIVE catalog's listener (the other entry), not
a walk on the swapped one — named, not asserted against.

Beside them: `cargo test -p harness-pin` (Gate 1 + Gate 2, 2/2),
`cargo test -p api-kit` (4/4 incl. the grant test), `cargo test -p
jinn-api --test profile_admin_wire` (4/4), the ui-kit verbatim gate
(3/3), `pnpm test` 845/845, typecheck, lint, build, ratchet — the
verbatim tails are on the Todo.

## Meter

The UI-2 meter (`docs/plans/ui-malleability-arc.md` §9: UI-1's paths
plus `plugins/ext/**`, `tools/ext-kit/**`,
`plugins/plugins/jinn-plugins/src`; `cfg(test)` a declared category; the
composition suite and every `tests/` directory excluded), `git diff
--numstat main` on a clean tree at the final head.

On the meter's paths (`'plugins/api/jinn-api-http/src/*.rs'` is the one
touched):

| file | + | − | `cfg(test)` | production net |
|---|---|---|---|---|
| `plugins/api/jinn-api-http/src/lib.rs` | 9 | 0 | 0 | +9 |
| `plugins/api/jinn-api-http/src/profile_admin.rs` | 53 | 0 | 0 | +53 |

**Production net on the meter's paths: +62**, so the UI-2 meter reads
**835** from 773.

Declared beside the meter (amendment 5's shape) — production Rust
OUTSIDE the listed paths, the reason being that the definition owns the
schema and the kits own the grants:

| file | + | − | `cfg(test)` | production net |
|---|---|---|---|---|
| `plugins/api/jinn-api/src/profile_admin.rs` | 283 | 0 | 0 | +283 |
| `plugins/api/jinn-api/src/lib.rs` | 1 | 0 | 0 | +1 |
| `tools/api-kit/src/lib.rs` | 34 | 1 | +28 (`mod tests`, one case) | +5 |

**Production net outside the paths: +289.** Excluded by the meter's
rule: `plugins/api/jinn-api/tests/profile_admin_wire.rs` (+168),
`tests/composition/**` (+623 −28).

**Total production net: 351 against the card's ESTIMATE of ≤ 300 — a
MISS of 51, named here and on the Todo.** The Todo's number is a
pre-design estimate ("five routes + page wiring"), re-priced once on
this first clean reading, which is the COO's. Where the 51 went: the
definition module carries the typed refusal classes and `retryable`,
the one-write-per-call classifier with its `invalid` reasons, the
five-shape wire encoder, the answer decoder and the answer schema —
each a card requirement (a refusal "surfaced typed as every `refused`
is"), none an addition. The TypeScript tree carries no line ceiling
(its acceptance is the diff): `profile-admin.ts` +79, `actions.tsx`
+254, the row/page/inventory/not-yet edits net +61.

## What did not move

- **The swap window** — dispose-then-spawn, the bundle's stated 0.1.0
  limit (jinnd M2-K27). Asserted in the kernel's shape above.
- **#52's reading** — re-measured at the pin (FINDINGS #52), open;
  M2-K23 touches no `DispatchTrace` field.
- **Reveal and rescan** — no counterpart: a catalog entry is not a
  folder, and the catalog is the document of record. They refuse
  client-side with that reason; the old "config only" reason is gone.
- **`version` on a swap** — the route accepts it and defaults it empty;
  no first-party entry declares one, so no proof exercises it.
- **Cascading removal, batch writes, kernel `revert` over admin
  intents** — the bundle's own OUT list, unchanged.
- **The soak and Hristo's instance** — untouched by this build; the
  refresh and the soak bump are the COO's separate steps after the land.

## Round 2 — the two Taste fixes, and nothing else

The verifier's round-1 verdict (REWORK, 0 Blockers, 2 Majors) named two
Jinn Taste breaches on a functionally green head, and the COO ruled the
round a DELTA of exactly those two. Neither touches the pin, a route, a
proof, or a finding.

**Taste §2 — `web/src/routes/settings/plugins/actions.tsx`.** The four
action pills were a 22 px caption pill and the form's inputs a 30 px
well with a 1 px inset shadow at rest. Both now wear the settings
page's OWN tokens, nothing new: the 34 px pill button the page already
uses for its actions (`config-conflict-notice.tsx`, `engines/legacy-row.tsx`)
and `CONTROL_CLASS` from `routes/settings/shared.tsx` at the
`h-[34px]` the model-map editor sets on it — no border, no shadow at
rest, the accent focus ring its only ring. The Apply pill keeps its
accent wash. Red-first, in `plugin-row.test.tsx` (the proof commit
precedes the fix in git):

```
 FAIL  src/routes/settings/plugins/__tests__/plugin-row.test.tsx > an extension row > stands every action and form control at the settings page's 34 px control height, no hairline at rest
AssertionError: expected 'inline-flex h-[22px] items-center rou…' to contain 'h-[34px]'
 Test Files  1 failed (1)
      Tests  1 failed | 7 passed (8)
```

Green after the fix: `Tests  8 passed (8)`. Measured in the live DOM on
the suite's own kit served by the pinned archive-built daemon
(`cargo run -p ui-kit -- kit`, a throwaway root, loopback port, torn
down after), the ext-green row with its Install form open — every
control `h=34.0`, `box-shadow: none`, `border: 0px`: the four action
pills, the five inputs, Apply, Cancel; at 1440 × 900 and at 390 × 844,
where `scrollWidth` is 390. The two screenshots are on the Todo
(`round2-plugin-row-desktop-1440.png`, `round2-plugin-row-mobile-390.png`).

**Taste §1 — `tests/composition/tests/profile_admin.rs`.** The three
`// -----` section dividers (lines 23, 111, 335) are deleted, each with
the blank line it left behind so rustfmt's one-blank-line bound holds;
no other byte in the file moves. The suite re-run against the pinned
daemon (`JINND_DIR` at a checkout holding `f8b285b`, the daemon
materialised from `git archive` as always):

```
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 68.33s
```

A first re-run with a RELATIVE `JINND_DIR` answered `5 passed` in
0.01 s — the gate's loud SKIP, every proof vacuous — and is not
evidence; it is named here so the next round does not mistake it for
a pass.

**Meter.** The UI-2 meter's paths are Rust production paths; the
TypeScript tree and `tests/composition/**` are outside it by its rule.
**Round-2 production delta on the UI-2 meter: 0** (360 binding, unchanged
at 351 read). Beside the meter: `actions.tsx` +6 −3 (an import line, a
two-line comment, the two token strings), `plugin-row.test.tsx` +14
(the proof), `profile_admin.rs` −6.
