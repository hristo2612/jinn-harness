# Navigation is stored source

PLA-376 adds one harness moment and one shell consumer. The optional Tools first
entry is the same admitted Boa artifact already mounted by the UI kit. Its source
is installed as data through profile-admin, and an agent's ordinary config PATCH
replaces the program. The shell has no preset branch and evaluates no JavaScript.
The payload contains only offered destination IDs, labels and availability. The
consumer reconstructs routing and availability from its own descriptors; extension
output cannot grant a route, introduce a URL, or remove Settings/Plugins recovery.

The focused Plugins row separates three observations: the accepted write's ledger
sequence, the catalog's lifecycle/incarnation, and the latest valid navigation
result. Source changes await a fresh observed Active incarnation; a new document
or digest alone is insufficient. An Active reading never means the listener's
last delivery succeeded. Throwing listeners can continue a walk with unchanged
output, and the per-listener result is unavailable at this pin (FINDINGS #51).
Invalid output and whole-walk refusal show standard navigation with a reason.

A completed administration cancels stale navigation queries and recomputes both
surfaces. Runtime confirmation polls every 500 ms for at most ten seconds.
Disposal/removal needs positive witnessed evidence after the request; removal
also needs document absence. Missing or evicted evidence stays unconfirmed.
History stays reachable after removal. Audit records (including previous source
and config) and the admitted shared artifact remain. The extension has no
application writes or external effects, and removal does not undo unrelated work.
Actual grant values, scopes, source qualifier and declared origin are shown.

## Red evidence

Base: `80012a2f05dcc04713f5fd96135e9eb12da2d668`.
Kernel: `f8b285b5aaffddeeb4939a0035d6c18a03487999`, built from git archive by
`composition::daemon::pinned_daemon`; no pin or vendored bytes changed.

Before implementation:

```text
cargo test -p jinn-ui navigation_is_a_typed_moment -- --nocapture
assertion `left == right` failed
  left: None
 right: Some("jinn:ui/after-build-navigation")
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out

pnpm exec vitest run src/lib/__tests__/navigation-extension.test.tsx
Expected ["My tools", "Settings"]; received the original destination list.
Test Files  1 failed (1)
Tests       1 failed (1)
```

Real-loader red-by-reversion: restore only the moment schema/export and kit topic
inventory to the base, keep the new composition test, and run:

```text
JINND_DIR=<checkout> cargo test -p composition --test moments \
  navigation_source_is_installed_replaced_disabled_and_removed_without_a_shell_rebuild \
  -- --nocapture

assertion `left == right` failed: typed navigation path: HTTP/1.1 404 Not Found
Content-Type: application/json
Content-Length: 77
Connection: close

{"api-version":"0.4.0","error":{"code":"not-found","detail":"no such route"}}
  left: 404
 right: 200
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 12 filtered out;
finished in 249.00s
```

The implementation was restored afterward. The expanded composition proofs use
actual profile writes, an alternative source, unchanged served shell bytes,
positive disposal witnesses, retained history, malformed/throwing/exhausted
sources, denied topic/admin grants and typed restarting refusal. Web behavior
tests cover validation, stale responses, bounded confirmation and source changes.
Final gate/live receipts belong to the submitted head and accompany the PR.

## Long labels remain readable on a phone

Live boundary evidence at the first candidate accepted a 40-character unbroken
label but rendered the two provided phone labels over their neighbours. The
result paragraph also painted outside its card. The correction constrains phone
labels to their tab width (ellipsis, full accessible name retained) and permits
wrapping anywhere in the inspection/result card. Targets and navigation paths
stay unchanged. The incomplete first-candidate Rust run was stopped when this
live defect was observed; it is not counted as passing final-head evidence.

Actual failing browser assertion against the first candidate's served artifact
(the label's rectangle must stay within its own tab):

```text
Error: Phone label exceeds its tab: XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```


The final removal exercise also caught stale source-activation prose after disposal.
A source observation now stops waiting when the document is disabled or absent;
this does not claim runtime disposal (the separate witness still owns that claim).
The behavioral red before the fix was:

```text
stops awaiting source activation when the entry is disabled or removed
AssertionError: expected 'Source changed; waiting to observe a fresh Active incarnation.' to contain 'disabled'
Test Files  1 failed (1)
Tests  1 failed | 5 passed (6)
```


## Interim review corrections

COO review required a real elapsed bound for stalled I/O and truthful copy after
external authority/provider changes. Three behavioral tests initially failed
(controls stayed busy after stalled runtime reads, final refresh and writes).
Two inspection cases initially failed for changed topics and an occupied provider.

```text
navigation-admin.test.tsx: Tests 3 failed | 2 passed (5)
navigation-inspection.test.tsx: Tests 2 failed (2)
```

Reads/writes now race an elapsed deadline; only timely observations publish a
confirmation. Refresh cannot hold administration controls busy. A timed-out write
remains uncertain, and a late read cannot retroactively claim witnessed success.
Inspection displays configured provider/topics and explicitly scopes access and
side-effect claims to the unchanged preset. Changed providers/topics may expose
other data and effects. No transport-wide behavior or kernel contracts changed.
