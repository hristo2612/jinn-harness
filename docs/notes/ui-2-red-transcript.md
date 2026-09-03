# UI-2: the proofs red, by reversion

*Harness packet UI-2 (PLA-353), round 2 — the red-first evidence the
card's §9.3 requires, supplied the way §9.7 amendment 8(e) names: the
proofs ALONE on the merge-base, run against the pinned daemon, the
failing tail of each pasted here. Round 1 wrote the proofs after the
implementation and had no failing transcript; this is the remedy, and
from this round on the proofs commit precedes the implementation commit
in the branch's history (`8abc816` before `2bda3e6` and `48bb947`).*

## What was run, exactly

A throwaway detached checkout at the merge-base — harness `main`
`101a657`, the tree UI-2 branched from — holding ONE proof-only commit,
`47e77d4`: the daemon proof file exactly as the round's tests-first
commit `8abc816` has it. Nothing else from the branch: no `jinn-ui`
moment vocabulary, no `jinn-ext`, no Boa guest, no `ext-kit`, no
`/v1/moments` route on the transport, no `attestation` on the catalog,
no client adaptation. `8abc816` itself is a delta on the round-1 branch,
so it is not cherry-picked onto a tree where several of its modified
paths do not exist; its proof artifact is extracted and committed alone,
making the reversion literal and inspectable.

```text
git worktree add --detach <throwaway> 101a657
git -C <throwaway> checkout 8abc816 -- tests/composition/tests/moments.rs
git -C <throwaway> add tests/composition/tests/moments.rs
git -C <throwaway> commit -m "test(ui-2): proof snapshot on merge-base"
git -C <throwaway> show --stat --oneline # 47e77d4, one file created
JINND_DIR=<a jinnd checkout holding a53a352> \
  cargo test -p composition --test moments -- --test-threads=1 --nocapture
```

The daemon is the pinned one, `a53a352`, built by the kit from `git
archive` of the pin (`tests/composition/src/daemon.rs`); its build cache
(`target/composition/pinned-jinnd`, marker `.commit` = `a53a352…`) was
cloned from the packet worktree's own cache into the throwaway rather
than rebuilt — the same bytes for the same pin, one heavy lane at a
time. The `ui` profile the proofs boot is what the merge-base's kit
writes: transport, bundle, settings, plugins, cron — and NO extension
entry, because the merge-base has no extension tier to mount.

The proof file compiles on the merge-base because it SPELLS the
vocabulary it checks (the topics, the package, the breadcrumbs, the §6
entry shape, the fixture sources, the digest) instead of importing it
from crates that do not exist there; that is what makes ten failing
proofs possible instead of one compile error, and it is the shape the
file keeps at the head (`docs/notes/2026-09-03-a-moment-is-one-walk.md`,
"Round 2").

The throwaway was `cargo clean`ed and its worktree removed after the
run. Paths in the tails below are relative to the throwaway's root.

## The summary line

```text
running 10 tests
test result: FAILED. 0 passed; 10 failed; 0 ignored; 0 measured; 0 filtered out; finished in 193.19s
```

Zero passed. No proof passes without the implementation, so none had
to be rewritten under the ruling's second sentence.

## The failing tail, per proof

Four distinct reasons, each the absence of one part of the packet.

**Proof 1** — `a_moment_with_no_listener_answers_its_own_payload`. The
proof REMOVES `ext-green` from the kit-written profile before boot; the
merge-base's kit never mounted one.

```text
thread 'a_moment_with_no_listener_answers_its_own_payload' panicked at tests/composition/tests/moments.rs:174:5:
assertion `left == right` failed: ext-green was mounted
  left: 12
 right: 11
```

**Proof 2** — `one_js_extension_folds_the_payload_and_the_ledger_says_so`.
The daemon boots; the catalog has no `ext-green` to read.

```text
thread 'one_js_extension_folds_the_payload_and_the_ledger_says_so' panicked at tests/composition/src/plugins.rs:120:28:
a lifecycle: {"api-version":"0.4.0","error":{"catalog-code":"not-found","code":"not-found","detail":"\"ext-green\" is not in catalog \"main\""}}
```

**Proof 3** — `two_extensions_compose_in_registration_order_and_the_order_is_named`.
The proof pins a second extension entry to the root's engine provider;
the merge-base builds no provider, so the kit's sidecar does not exist.

```text
thread 'two_extensions_compose_in_registration_order_and_the_order_is_named' panicked at tests/composition/src/kit.rs:410:33:
target/composition/runs/moments-two-92586/artifacts/jinn-ext-js-boa.wasm.sha256: No such file or directory (os error 2)
```

**Proof 4** — `a_throwing_extension_is_recorded_and_the_walk_continues`.
Same absence: no engine provider to pin the throwing entry to.

```text
thread 'a_throwing_extension_is_recorded_and_the_walk_continues' panicked at tests/composition/src/kit.rs:410:33:
target/composition/runs/moments-throw-92586/artifacts/jinn-ext-js-boa.wasm.sha256: No such file or directory (os error 2)
```

**Proof 5** — `a_restarting_extension_refuses_the_moment_typed_and_nothing_is_sent`.
The proof's first moment, before any edit, meets a transport with no
moment route: `404 no such route`, and no fold.

```text
thread 'a_restarting_extension_refuses_the_moment_typed_and_nothing_is_sent' panicked at tests/composition/tests/moments.rs:688:5:
assertion `left == right` failed: HTTP/1.1 404 Not Found
Content-Type: application/json
Content-Length: 77
Connection: close

{"api-version":"0.4.0","error":{"code":"not-found","detail":"no such route"}}
  left: Null
 right: "hello 🟢"
```

**Proof 6** — `an_extension_is_granted_its_topic_and_nothing_else`.
No engine provider to pin the ungranted entry to.

```text
thread 'an_extension_is_granted_its_topic_and_nothing_else' panicked at tests/composition/src/kit.rs:410:33:
target/composition/runs/moments-grants-92586/artifacts/jinn-ext-js-boa.wasm.sha256: No such file or directory (os error 2)
```

**Proof 7** — `a_looping_extension_costs_the_walk_the_guest_deadline_and_the_transport_s_fate_is_recorded`.
No engine provider to pin the looping entry to.

```text
thread 'a_looping_extension_costs_the_walk_the_guest_deadline_and_the_transport_s_fate_is_recorded' panicked at tests/composition/src/kit.rs:410:33:
target/composition/runs/moments-loop-92586/artifacts/jinn-ext-js-boa.wasm.sha256: No such file or directory (os error 2)
```

**Proof 8** — `an_extension_boots_from_a_profile_and_a_syntax_error_is_a_failed_fiber`.
No engine provider to pin the broken entry to (and no `ext-green` in
the profile behind it).

```text
thread 'an_extension_boots_from_a_profile_and_a_syntax_error_is_a_failed_fiber' panicked at tests/composition/src/kit.rs:410:33:
target/composition/runs/moments-boot-92586/artifacts/jinn-ext-js-boa.wasm.sha256: No such file or directory (os error 2)
```

**Proof 10** — `a_moment_is_the_door_then_one_walk_and_nothing_else`.
The transport has no moment route: `404`, no door, no walk.

```text
thread 'a_moment_is_the_door_then_one_walk_and_nothing_else' panicked at tests/composition/tests/moments.rs:1072:5:
assertion `left == right` failed: HTTP/1.1 404 Not Found
Content-Type: application/json
Content-Length: 77
Connection: close

{"api-version":"0.4.0","error":{"code":"not-found","detail":"no such route"}}
  left: 404
 right: 200
```

**The KG-6 probe** — `an_emit_is_not_gated_by_the_topics_grant_at_this_pin`
(FINDINGS #49). The probe strips the transport's three topic grants
from the profile; the merge-base's kit never granted them.

```text
thread 'an_emit_is_not_gated_by_the_topics_grant_at_this_pin' panicked at tests/composition/tests/moments.rs:1183:9:
assertion `left == right` failed: the three topic grants were there
  left: 8
 right: 5
```

Proof 9 is the repo gate (`tools/ui-kit/tests/verbatim.rs`), not a
daemon proof; proof 11 is the verifier's, over `agent-browser`.

## Green, at the head

The same file, same daemon, with the implementation: the round's gate
output on the PR carries `tests/moments.rs` at `10 passed; 0 failed`.
