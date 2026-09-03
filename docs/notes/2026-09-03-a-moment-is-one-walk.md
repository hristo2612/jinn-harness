# A moment is one walk, and the kernel's window is fail-open

*Harness packet UI-2 (PLA-353) — moments and the JS-in-WASM extension
tier on jinnd `a53a352`. Kernel changes never here; this note is about
what the harness built, what it measured, and the one place the pinned
kernel breaks the packet's decision.*

## The decision, and where each half lives

The card's one decision (`docs/plans/ui-malleability-arc.md` §9.1): a
moment is a `waterfall` walk on `jinn:ui/<topic>` that the transport
dispatches for an authenticated `POST /v1/moments/<domain>/<topic>` and
answers with the folded payload; a refused walk is a typed `503` naming
the refusal, never the unmodified payload. Three homes:

- **The vocabulary is `jinn-ui`'s** (`src/moments.rs`): the three topics,
  each payload's schema, the path law. Closed, not forwarded — a path the
  crate does not name is a 404 with no dispatch. The transport asks the
  definition "which topic?" and never derives one from the path itself,
  so `/v1/moments/introspect/transitions` cannot reach `emit` even though
  the kernel would refuse it there; a route that relies on the kernel's
  refusal is a route that dispatched.
- **The walk is the transport's** (`jinn-api-http/src/moments.rs`, 74
  lines): the door, the schema, one `events.emit(topic, waterfall, all,
  body)`, the fold or the refusal. Every `kernel-error` case is mapped by
  identity, never by parsing prose (R3): the five whole-walk refusals are
  `unavailable` with the case's name first in `detail` and again as the
  typed `refusal` field; `grant-refused` is `refused`; `invalid` is
  `invalid`.
- **The listener is the extension tier** (`plugins/ext/`): a definition
  crate holding the closed config schema and the two JS programs, and a
  Boa guest that evaluates them.

## Why the JS programs are the definition's and not the guest's

`jinn-ext` owns `self_test(source)` and `delivery(source, payload)` — the
exact JS text an engine evaluates at activation and per delivery — even
though only the guest runs them. Two reasons. The payload rides into JS
as `JSON.parse(<a JSON string literal>)`, never spliced as code, and
that escaping rule is a contract every engine must keep identically;
one home for it means a second engine (`jinn-ext-js-quickjs`) cannot
drift on it. And the three answers — an object folds, `undefined`
passes through as EMPTY bytes, anything else throws — are the tier's
law, tested natively in the definition's `tests.rs` without a wasm
build. The guest is then thin: a clock, a context, an `eval`, and the
string it gets back.

## The toolchain facts, carried

`getrandom` refuses `wasm32-unknown-unknown` without a backend cfg; the
guest's own `.cargo/config.toml` carries `--cfg getrandom_backend="custom"`
so the flag travels with the crate — the kit strips `RUSTFLAGS` from
guest builds on purpose, which is exactly why a shell-side flag would
not survive. The backend symbol is `__getrandom_v03_custom` in 0.4 too,
and it is a deterministic xorshift, stated as such: the plugin world
imports no entropy and Boa's hashers are its only consumer. A Boa
context needs a `Clock`; `Context::default()` reaches for
`Instant::now()`, which aborts in the plugin world before `activate` can
say a word — so every context is built on `FixedClock::from_millis(now)`
with `now` read ONCE from `jinn:clock`, the guest's one host call per
delivery. The component's imports are exactly `types`, `effects`,
`events`, `services` of `jinn:plugin@0.10.0`, and `tools/ext-kit/tests/imports.rs`
reads them off the encoded component at depth 0 (the encoder nests a
shim component whose imports are the export glue; those are not the
host surface).

## What proof 2 measured

Twenty walks of the §6 payload through `ext-green`, a fresh Boa context
each, on a debug-built pinned daemon under fuel metering: **3.27 ms
average per walk, 9.7 ms worst**, and ~1 ms between the walk's trace row
and the door's decision on the ledger's own clock. §5.5's "correct and
slow" is correct and 3 ms. No context reuse is designed (§9.5), and the
guest's memory high-water mark is not a reading the kernel exposes
(`FINDINGS.md` #50).

## What proof 5 found instead of what the card expected

The card expected a moment posted inside an extension's restart window
to be refused `restarting` (M2-K9). The pinned kernel does something
else on a `ConfigChanged` restart of a listener-only fiber: it suspends
the old incarnation and WITHDRAWS its `listen` at the start of the
replacement, activates the new instance in staging, and lands the new
`listen` at the commit. For the whole staging window — 1.5 s with the
proof's deliberately slow source, 53 walks — the topic has no registered
listener, so `emit` selects nobody, the walk lands with `listeners: 0`,
and the transport answers the payload UNMODIFIED. Not one `503`. That is
the fail-open the decision exists to prevent, one layer below the
transport, and it is `FINDINGS.md` #47 (Blocker-class). Proof 5 keeps
the transport's half as an assertion (a refusal, when one comes, is
typed) and records the kernel's half: it counts the unmodified answers,
prints the window, and lands NOT-YET.

The vehicle matters too. The card said `PATCH /v1/profile/entries/ext-green`;
the proof edits the profile DOCUMENT. Through the API the transport
itself awaits the restart inside the patch's own request (`FINDINGS.md`
#26) and, being one instance, cannot take a moment until the patch
answers — so the window is unobservable from the API by construction.
The document lane is the watcher's, and the transport stays free.

## What proof 7 records

A `while (true) {}` source on `jinn:ui/before-send`. The TRANSPORT's
own instance died at the 5 s deadline — it emits inside its own
`handle-event`, on the same clock — the moment got no answer, the port
kept accepting for an instance that was gone, and the transport's fiber
was left without a transition (`FINDINGS.md` #48, Blocker-class; the
ruling's NOT-YET clause applies and jinnd M2-K25 is the unblock). Nothing after that walk trusts
the transport to answer: the socket may accept (the kernel holds the
listener) while no incarnation serves it, so every read is bounded and
the state is read off the ledger. The first run of the proof learned
that the hard way — a plain `GET` after the walk hung for the client's
45 s bound, which was itself the evidence.

## Two more things the ledger said

A throwing extension's failure is `failures: 1` on the EMITTER's trace
row and nothing at all on the listener's history — the plugins page
shows it `active` with a clock read (`FINDINGS.md` #51, #38's sibling
for deliveries). And a syntax error fails the fiber with the crumbs
`activate entered`, `config parsed`, `js context built` on the record —
the context builds before the source is read, one crumb further than
the card's "up to `config parsed`" — and the guest names its fault on
the ledger before failing, the transport's #38 workaround copied.

## Meter

The UI-2 meter (`docs/plans/ui-malleability-arc.md` §9 header: UI-1's
paths plus `plugins/ext/**`, `tools/ext-kit/**`,
`plugins/plugins/jinn-plugins/src`; `cfg(test)` a declared category;
`tests/composition` excluded), `git diff --numstat main` on a clean
tree, reads 750 raw production Rust net. The one `#[cfg(test)]` module
added in `tools/ui-kit/src/lib.rs` is 25 lines, so the billed reading is
**725 / 800**. Round 1's estimate was ~740; round 2 declared an estimated
net-zero delta by ruling item before its first edit, and the actual +7 is
the catalog source attestation mandated by §9.7 amendment 8(d).

## Round 2: what the verifier found, and what changed

Three Blockers and a Major at round 1's head (PLA-353,
`wic_992226223d0a`; rulings `wic_11e20879a220`; plan §9.7 amendment 8).

**The page said "Saved" over a stale draft.** The save went through the
moment, the PATCH carried the folded patch, the daemon held the folded
value — and the page kept the number the operator typed, because the
adapter recorded the daemon's answer only in its own `lastRead` and the
commit hook marked the write saved without handing the page anything.
Now `updateConfig` answers `{ revision, config }`, the document as the
daemon holds it, and the commit hook's `onFolded` replaces the page's
draft with it BEFORE the status reads saved. One guard: when a newer
edit is already queued behind the write in flight, the answer is not
adopted — that edit goes out next and brings its own answer, and
replacing the draft under it would clobber the very edit about to be
written. The Settings page sits exactly at its size ratchet, so its
adoption is one line on the hook's option list.

**The breadcrumb was read off a sliding window.** Proof 11 wants the
`source sha256:` breadcrumb on the plugins page; round 1 showed it only
in the row's history, which is the last 400 ledger rows — and the page's
own traffic pushed the activation rows out of the window before the
verifier looked. The breadcrumb is now the catalog's: `attestation`
carries `source` (the digest of `config.data.source`, `sha256:<hex>`)
beside `origin`, computed by `jinn_ext::source_digest` — the one home,
the same bytes the guest writes on the ledger — and the row renders
`source <digest>` from that stable reading. The history stays what it
is: a window, labelled as one.

**Proof 4 printed what it should have asserted.** The card's "in ITS
history" is not true at this pin (#51), and a print of the absence is
not evidence of anything. The proof asserts `failures: 1` on the
emitter's trace and, as a named NOT-YET assertion, that the throwing
extension's history after the walk is its clock read and no failure
row. When a pin writes the row, that assertion fails with #51 in its
message and the proof is flipped.

**The proofs came after the implementation.** Round 1 opened the PR on
the implementation commit and wrote the proofs against it; there was no
failing transcript. The remedy the ruling names is red-by-reversion:
the proofs alone on the merge-base, run, the failing tail of each
pasted (`docs/notes/ui-2-red-transcript.md`). For that to be possible
at all the proofs had to STAND without the implementation — round 1's
file imported the topics from `jinn-ui`, the breadcrumbs and `Origin`
from `jinn-ext` and the fixture sources from `ext-kit`, none of which
exist at the merge-base, so "alone on the merge-base" would have been
one compile error and not ten failing proofs. The proofs now spell the
vocabulary they check: the topic strings, the package, the breadcrumbs,
the §6 entry shape, the fixture sources, the digest. That is the right
shape for an acceptance test anyway — the card names those strings, and
a crate whose constant drifted from the card should fail the proof, not
redefine it. The production crates stay the one home for production
code; the proof-only sources (a throwing one, a looping one, …) that
`ext-kit` used to export for the suite's benefit now live in the suite,
which is where a fixture belongs.

**Every NOT-YET item is on the page, disabled, with its number.** The
extension row carries three pills — the K23 profile-admin edits
(#37, PLA-348), a moment mid-restart (#47, M2-K26), a bad extension
costing only its own slot (#48, M2-K25) — each `disabled` with the
finding as its title and the number in the label; install joins the
plugins page's read-only sentence. A limit an operator can see is a
limit they can plan around; one that is merely absent is a surprise.
