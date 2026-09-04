# A delivery spends its listener's clock, and a dead instance is a failed fiber

*Harness pin-bump 8 (PLA-361) — adopting jinnd `b1dbe8f` (M2-K25, the
per-delivery budget; `jinn:plugin` 0.10.0 → 0.11.0, additive). Kernel
changes never here; this note is about what the harness could finally
assert, and the one field it could finally declare.*

## What UI-2 had to leave open, and why the pin is the fix

UI-2 proof 7 mounted a `while (true) {}` extension on
`jinn:ui/before-send` and RECORDED what happened to the transport, which
emits inside its own `handle-event` (`2026-09-03-a-moment-is-one-walk.md`,
"What proof 7 records"). At `a53a352` the record was FINDINGS #48: every
guest call was one `settle(deadline)`, `emit` awaited every delivery on
the emitter's clock, so the transport's instance died at the 5 s
deadline, the operator API was gone until a daemon restart, the port
kept accepting for an instance that could not answer, and the
transport's fiber showed no transition. Three defects, one transcript;
the UI-2 card withheld the extension entry's `budget` field because a
declared field the guest cannot enforce is a lie on the record.

M2-K25 answers all three from the kernel's side (`docs/packets/M2-K25.md`
in the kernel repo): the emitter's guest deadline does not run while it
is parked on a walk, every delivery is bounded on the LISTENER's side —
its declared fuel budget (`events.listen-within`, `types.delivery-budget`)
or its guest deadline — and an instance the kernel ends after activation
fails its OWN fiber on the record under the additive
`TransitionCause::BodyFaulted`, releasing its kernel registrations. What
the harness owes is the flipped assertion, the field, and the closure.

## The bump, in the order it happened

1. **The proofs, red first, before the pin** (`53f634f`). Proof 7 flipped
   to its intended assertion and proof 7b written beside it; the
   `jinn-ext` unit test names the `budget` field. Run on the merge-base
   against the `a53a352` daemon:

   ```text
   thread 'a_looping_extension_costs_its_own_slot_and_not_the_transport' panicked at tests/composition/tests/moments.rs:922:5:
   the walk costs the listener's guest deadline and no more: 60.00220725s

   thread 'an_extension_s_budget_is_a_listen_within_and_a_looping_delivery_ends_at_its_fuel' panicked at tests/composition/tests/moments.rs:1067:9:
   assertion `left == right` failed: a budget is accepted at activation: HTTP/1.1 200 OK
   …
     left: "failed"
    right: "active"

   test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 9 filtered out; finished in 223.77s
   ```

   and `cargo test -p jinn-ext`:

   ```text
   error[E0422]: cannot find struct, variant or union type `Budget` in this scope
   error[E0609]: no field `budget` on type `ExtConfig`
   error: could not compile `jinn-ext` (lib test) due to 5 previous errors
   ```

   Proof 7's red is the transport dying (the 60 s is the client's own
   bound on a socket nobody answered); proof 7b's red is the closed
   schema refusing the field (`ext-green` and `ext-looping` both `failed`
   at activation, so no listener, so no fold).

2. **The pin, one commit** (`786ee7c`). `KERNEL-PIN.md`'s procedure:
   commit, both hashes and the vendored `kernel-pin/` trees together;
   `cargo test -p harness-pin` green on both gates. The plugin world
   moved 0.10.0 → 0.11.0, so every guest's generated bindings now
   import `@0.11.0`, and `tools/ext-kit/tests/imports.rs` went red on
   its own exactly as built:

   ```text
   thread 'the_boa_provider_imports_exactly_the_four_plugin_world_interfaces' panicked at tools/ext-kit/tests/imports.rs:18:5:
   assertion `left == right` failed: the component's imports, in declaration order: ["jinn:plugin/types@0.11.0", "jinn:plugin/effects@0.11.0", "jinn:plugin/services@0.11.0", "jinn:plugin/events@0.11.0"]
     left: ["jinn:plugin/effects@0.11.0", "jinn:plugin/events@0.11.0", "jinn:plugin/services@0.11.0", "jinn:plugin/types@0.11.0"]
    right: ["jinn:plugin/effects@0.10.0", "jinn:plugin/events@0.10.0", "jinn:plugin/services@0.10.0", "jinn:plugin/types@0.10.0"]
   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.16s
   ```

   At this commit proof 7 is GREEN with no harness change but the pin —
   the flip is the kernel's, which is what a pin-bump proof is for —
   and proof 7b is still red on the same closed schema:

   ```text
   proof 7: the looping walk took 5.001711833s (guest deadline 5s); the moment answered Some(200) unmodified with {"emitter":3,"failures":1,"listeners":1,"mode":"Waterfall","topic":"jinn:ui/before-send"}
     the transport after the walk: GET /v1/health Some(200) in 2.775416ms, incarnation Number(12) → Number(12) …
     ext-looping after the walk: errors ["guest exceeded its call deadline"], transitions [("Active", "Unloading", "BodyFaulted"), ("Unloading", "Failed", "BodyFaulted")]
     deadline rows: [(303, Some("ext-looping"), "{\"ErrorRecorded\":{\"error\":{\"code\":\"PluginFailed\",\"message\":\"guest exceeded its call deadline\",\"fiber\":12}}}")]
   proof 7: the transport survived the walk — a bad extension costs its own slot (FINDINGS #48 closed at this pin)
   ok
   test an_extension_s_budget_is_a_listen_within_and_a_looping_delivery_ends_at_its_fuel ...
   thread 'an_extension_s_budget_is_a_listen_within_and_a_looping_delivery_ends_at_its_fuel' panicked at tests/composition/tests/moments.rs:1067:9:
   assertion `left == right` failed: a budget is accepted at activation: HTTP/1.1 200 OK
   …
     left: "failed"
    right: "active"
   test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 207.75s
   ```

3. **The field, and the provider's translation** (`d3bfb99`). `jinn-ext`'s
   `ExtConfig` gains `budget: Option<Budget>` where `Budget { fuel: u64 }`
   is the kernel's `delivery-budget` record spelled on the entry —
   typed, closed, optional, absent-stays-absent on the wire. The Boa
   provider's activation calls `events::listen_within(topic, token,
   DeliveryBudget { fuel })` when the entry declares one and
   `events::listen` when it does not; zero is carried as declared, so
   the kernel's `invalid` refusal is the record and the provider never
   clamps (one home for "zero is not a budget": the contract).
   `ext_kit::ext_entry` takes the budget, and the `ui` profile mounts
   `ext-green` under `ext_kit::GREEN_BUDGET` (4 000 000 000 fuel — proof
   2's fold is a fresh Boa context plus the source at 3.3 ms; the number
   bounds a runaway, never a fold). Green:

   ```text
   proof 7b: budgeted walk took 32.148084ms (fuel 50000000 on the looping listener, 4000000000 on the fold); {"emitter":3,"failures":1,"listeners":2,"mode":"Waterfall","topic":"jinn:ui/before-send"}; ext-looping rows [ContractCall jinn:clock now, ErrorRecorded { code: PluginFailed, message: "guest exhausted its delivery fuel budget", fiber: 13 }, FiberTransition { Active → Unloading, BodyFaulted }, FiberTransition { Unloading → Failed, BodyFaulted }, EffectWithdrawn { clean: true, label: "listen jinn:ui/before-send" }, …]; the next walk {"failures":0,"listeners":1,…}
   proof 7b (second half): a zero budget is refused at listen — ext-zero failed with ["delivery fuel budget must be non-zero"]; ext-green folds beside it
   proof 7: the looping walk took 5.000880667s (guest deadline 5s); the moment answered Some(200) unmodified with {"emitter":3,"failures":1,"listeners":1,"mode":"Waterfall","topic":"jinn:ui/before-send"}
   proof 7: the transport survived the walk — a bad extension costs its own slot (FINDINGS #48 closed at this pin)
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 251.75s
   (tools/ext-kit/tests/imports.rs at @0.11.0: test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.55s)
   ```

4. **The page and the record.** The plugins page drops the #48 pill and
   keeps #37 and #47 (`web/src/routes/settings/plugins/not-yet.tsx`; its
   test pins six pills and no `#48`). FINDINGS #48 is graded ANSWERED at
   the pin with proof 7's transcript appended in place; #51's fatal half
   is closed under it, its non-fatal half stays open and proof 4's
   NOT-YET assertion stands as written. The README's limitations name
   the retirement; the plan's §9.3 item 7 is restated to the pin with
   its previous text kept, the pin-bump 7 shape.

## What the proofs say now

| proof | red first | green at `b1dbe8f` |
|---|---|---|
| 7 the transport survives | the transport died; 60 s client bound (`a53a352`) | `the looping walk took 5.00 s (guest deadline 5 s); the moment answered 200 unmodified, failures: 1; GET /v1/health 200 in 2.8 ms; incarnation 12 → 12; ext-looping: "guest exceeded its call deadline", Active → Unloading → Failed under BodyFaulted` |
| 7b the budget is a `listen-within` | `ext-green` `failed` at activation: the schema refused `budget` (`a53a352`, and `b1dbe8f` before the field) | `budgeted walk took 32 ms (fuel 50 000 000 on the looping listener); failures: 1 of 2; ext-looping: "guest exhausted its delivery fuel budget", Failed under BodyFaulted, its listen withdrawn clean; the next walk 1 listener, 0 failures; a zero budget: "delivery fuel budget must be non-zero", ext-green folds beside it` |

## What did not move

- **#47 (M2-K26) stays open** — proof 5 stays NOT-YET, the page keeps its
  pill.
- **#51's non-fatal half stays open** — a throwing extension's contained
  failure is still a count on the emitter's trace and no row of its own.
- **No budget for `services.call`** — the card's §Out; a provider's slow
  answer still spends the caller's clock (#4/#32's class).
- **The soak** — pin `3a8e5c03`, pid untouched; the 2026-09-04 audit
  decides its bump.
- **The README already has a paragraph titled "Pin-bump 8"** from phase
  2.4 (`3fd7b05` → `3a8e5c0`, the eighth bump of that era's count). The
  COO's packet numbering restarted with the UI arc; this bump is
  "pin-bump 8" by that count, and the README paragraph names its pin
  (`b1dbe8f`) so the two are never confused.

## Meter

The UI-2 meter (`docs/plans/ui-malleability-arc.md` §9: UI-1's paths
plus `plugins/ext/**`, `tools/ext-kit/**`,
`plugins/plugins/jinn-plugins/src`; `cfg(test)` a declared category;
the composition suite excluded), `git diff --numstat main` on a clean
tree. Estimate before the first edit: about 60 production Rust net,
under the card's ≤ 150; re-priced once on the first reading: 46, so the estimate stands and no re-pricing is needed.

| file | + | − |
|---|---|---|
| `plugins/ext/jinn-ext/src/lib.rs` | 20 | 2 |
| `plugins/ext/jinn-ext-js-boa/src/lib.rs` | 17 | 4 |
| `tools/ext-kit/src/lib.rs` | 22 | 9 |
| `tools/ui-kit/src/main.rs` | 4 | 2 |

Raw net +63 −17 = 46; `cfg(test)` modules touched: none (the `jinn-ext` tests
live in `src/tests.rs`, a test file by name). **Production net
+46**, so the UI-2 meter reads **771** from 725. Production
Rust outside the listed paths: none.
