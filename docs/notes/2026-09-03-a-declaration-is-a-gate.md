# A declaration is a gate, and a gate is not a subscription

*Harness pin-bump 7 (PLA-352) — adopting jinnd `a53a352` (M2-K24,
string-lane `injects`; `jinn:introspect` 0.5.0 → 0.6.0). Kernel changes
never here; this note is about what the harness stopped doing.*

## What UI-1 had to build, and why it was always going to be removed

UI-1's transport reads its bundle ONCE, at `activate`, as an injected
dependency — the only shape under which a byte served to a browser is
never a crossing on an unauthenticated connection's behalf
(`2026-09-02-a-byte-is-not-a-dispatch.md`). At pin `85d36b4` the kernel
gave that read no guarantee that the provider was there: `resolve`
answers from the grant alone, the first CALL meets the provider, and
sibling activation order was unspecified (#7). The harness closed the
window from its side, within Law and three times over (#45): a
subscription to `jinn:introspect/transitions` under a grant the
transport had no other use for, a second probe before the listen, ONE
post-commit probe on a one-shot clock alarm (because a listen made
inside `activate` is not live until the activation commits — the coin
toss's third face), and a classification of a provider's contained
failure as "not yet". And a swap was a witnessed re-read, not a restart
(#46): epoch gating stopped at the string lane.

Every one of those was a harness stand-in for a sentence SOURCE-OF-TRUTH
§3 already made about the typed lane: a fiber activates only when every
injected service's provider is Active, and any provider change forces
consumers through a full unload → reload. The packet card that closes
them (jinnd M2-K24) carries that sentence to the string lane as a
DECLARATION on the entry: `config.injects`, beside `config.grants`.

## The bump, in the order it had to happen

1. **The pin, one commit.** `KERNEL-PIN.md`'s procedure: hashes, commit
   and the vendored `kernel-pin/` trees together; `cargo test -p
   harness-pin` green on both gates. No `.wit` signature in the plugin
   world changed, so no guest needed a source change for the world.
2. **The mirrors went red on their own.** `entry` gained `injects` and
   `unmet`; both introspect mirror gates refused the new file exactly as
   they were built to (`plugins/api/jinn-api/tests/introspect_mirror.rs`:
   `left: ["injects", "unmet", "unserved"] right: ["unserved"]`;
   `plugins/plugins/jinn-plugins/tests/introspect_mirror.rs`: `the entry
   record grew a field this seam has not read: injects`). Each is
   widened by NAMING the two fields — read by nothing here yet; the
   operator surface that shows why an entry is `pending` is a later
   card's, not a side effect of a pin bump.
3. **The proofs, restated and red first.** Proofs 3, 4 and 5 restated to
   the new pin were run against the OLD harness code on the NEW kernel —
   an entry that declares nothing is unchanged by K24 (the kernel's own
   invariant says so), so this is the old behaviour exactly, at the cost
   of one daemon build instead of two. Proof 3 (`manifest` crossings
   exactly 1) and proof 4 (incarnation +1 exactly) went red; proof 5 and
   5b are recorded below as they fell.
4. **The declaration, and the removal.** `tools/ui-kit` writes
   `injects: ["jinn:ui-bundle"]` on the transport entry and no longer
   grants `jinn:introspect` or `jinn:clock` to it. `jinn-api-http` loses
   the subscription, both probes, the alarm, the transition matcher and
   the "not yet" classification: `ui::read()` is the one read, every
   refusal is the entry's own fault (R11), and `activate` calls it when
   the profile mounts a bundle. Removed, not flagged: a workaround kept
   behind a flag is a second implementation of the kernel's promise, and
   the drift between them is the next finding.

## What the proofs say now

All six green at pin `a53a352` (`tests/composition/tests/ui.rs`, the
pinned daemon from `git archive`), the three restated ones red first
against the old harness code on the same kernel:

| proof | red first (old harness code, new kernel) | green (declaration) |
|---|---|---|
| 3 one crossing | `manifest` crossings 4 (two probes, the alarm, the witnessed read) | `bundle 1375153 bytes crossed once (1 manifest crossings); 31 files; ledger 168 rows in total, 30 on the transport` |
| 4 swap is a restart | incarnation unchanged (`left: 3 right: 4`) | `swap served 1.33 s after the edit; blip: 3 refused connects; transport loads 1 -> 2 (incarnation identity 11 -> 13); bundle crossings 1 -> 2` |
| 5 one order | timed out: the transport rested Active with a 503 (the late order) | `refused at activation — the transport's fiber failed, the port never opened; reason on the record: true (#38)` |
| 5b ten boots | 10/10 (the workaround holding) | 10/10, boot-to-served ≈ 27 s each on the shared kit's boot (the first boot of a run pays the kit copy) |

One word of the card needed the kernel's own vocabulary. "Incarnation +1
EXACTLY" reads as a per-fiber counter; the introspect `incarnation`
"identifies the CURRENT activation — never reused within a kernel
process" — an identity (the roster slot id), and the swapped-in bundle
fiber takes one between the transport's two, so it read 11 → 13. The
kernel's invariants spell the same promise as one more LOAD of the fiber
(`assert_eq!(loads(..), 2, "incarnation +1 exactly")`), and proof 4
asserts exactly that on the transport's own rows: one more `Loading`, the
one `Unloading` before it caused by `DependencyChanged`, the identity
moved and printed. Rows in `FINDINGS.md` #46.

## What the swap costs now, and why that is the right shape

At `85d36b4` a swap was 0 refused connects: the transport never stopped
listening, because it never restarted. At `a53a352` the port closes
between the two incarnations — the "blip" the UI-1 card predicted from
#27's reconcile — and proof 4 measures it instead of asserting it away.
The trade is the whole point of R9: a transport that keeps serving across
a provider change is a transport whose running state the kernel cannot
vouch for. A restart is a fact on the ledger (`Unloading`, cause
`DependencyChanged`, on the transport's own row); a refresh in place was
a fact only in the transport's memory.

## What did not move

- **#38 (KG-5) stays open.** The transport still writes its own
  activation fault onto the ledger before failing, because the kernel
  still records a state and never a reason. Proof 5 prints whether the
  reason was on the record; it does not assert around it.
- **The soak.** Pin `3a8e5c03`, pid untouched; the 2026-09-04 §7(b)
  audit decides its bump. This bump is for the harness tree and the
  operator's test instance.
- **The late-provider order is gone, and so is its proof.** Order (ii)
  of the old proof 5 forced a bundle entry to land AFTER the transport
  was `active`. A declared consumer whose provider is absent now rests
  `pending` — it never opens its port without its bundle — so that
  order is not reachable and the proof would be proving a state the
  kernel forbids. Named here so nobody hunts for it.
- **`ui-bundle-entry` in the transport's config stays** as the profile's
  own statement that a bundle is mounted; the kernel's declaration gates,
  the grant authorizes, the data field tells the plugin which seam to
  serve. Three facts, three homes.

## Meter

The UI-1 meter (`docs/plans/ui-malleability-arc.md` §4 as amended:
`plugins/ui/`, `plugins/api/jinn-api-http/src`,
`plugins/api/jinn-api-http-wire/src`, `tools/ui-kit`; `cfg(test)` a
declared category; the composition suite excluded), `git diff --numstat
main` on this branch:

| file | + | − |
|---|---|---|
| `plugins/api/jinn-api-http/src/lib.rs` | 11 | 32 |
| `plugins/api/jinn-api-http/src/ui.rs` | 19 | 78 |
| `tools/ui-kit/src/lib.rs` | 47 | 6 |

Raw net −39; the kit's new `#[cfg(test)]` module is 39 lines, all added,
subtracted as the category: **production net −78**, so the UI-1 meter
reads **765** from 843. Production Rust outside the four paths: none (the
two mirror gates widened are test files).
