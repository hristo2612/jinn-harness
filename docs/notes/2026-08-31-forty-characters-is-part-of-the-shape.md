# Forty characters is part of the shape

*PLA-297, packet 1.11, round 2. Both Majors from the round-1 verify.*

## The defect this packet fixed, and the one it repeated

Round 1's finding was that the soak's account of its own kernel was a file
sitting beside a binary: two files in a directory make no claim about each
other, so the record went two bumps stale and nothing could have noticed.
The fix bound the pin to the binary's digest and called that a derivation.

It was not one yet. Every hex reading in both scripts went through a single
predicate — *a nonempty run of lowercase hex* — under which `a` is a commit,
`a` is a sha256, and a record reading `running-pin=a harness-pin=b` is a
licensed account of a 64-megabyte daemon. The verifier produced exactly that
at rc 0.

That is the packet's own defect one level in. A record asserted a pin that
nothing checked; the fix derived the pin from a string that nothing checked.
Derivation from an unvalidated input is transcription with extra steps, and
the join around it — digest equals recorded digest — does not save it,
because two malformed strings compare equal just as happily as two good ones.

So length is part of the shape: 40 for a commit, 64 for a digest, and
anything else is `unknown` with the attempted reading named.

## Two readings, two fields

The stronger correction is the one that generalises. *Well-formed* and *is a
real commit* are two different questions, and a single `running_pin=` field
answers whichever one the reader assumes. So the check that was actually
performed is reported beside the value:

- `well-formed` — 40 lowercase hex characters, and nothing more was asked.
  This is what every launchd start reads, because no kernel checkout is in
  sight from the runtime root.
- `resolves-in-kernel-repo` — a checkout was reachable (`JINND_DIR`) and it
  holds that object.
- `absent-from-kernel-repo` — a checkout was reachable and it does not. The
  value falls to `unknown`; a sha nothing holds is not a weak pin, it is not
  a commit.

This is the same rule that already keeps `running_pin` and `harness_pin`
apart. A field that can hold either of two readings cannot say which one it
is holding, and the distance between the readings is exactly what an audit
exists to measure.

`record-build.sh` takes the same two readings at install time and refuses
rather than writing a record with a hole in it. The wrapper reports; the
installer gates.

## The gate, because correction has been tried

Four ASCII section dividers were deleted from `soak_supervisor.rs` in the
previous soak packet's round 6. Four more arrived in these two scripts one
packet later. A second instance of a class is evidence that the class
recurs, so it is now a test: `no_soak_shell_asset_carries_a_section_divider_comment`
scans every `tools/soak/*.sh`.

The check found eleven, not four — the seven that predate this branch
included — and all eleven are gone. A gate that permits what is already
there permits the next one too, and the prose those lines were heading did
not need the decoration to be readable.

## What did not change

The live soak was not touched: pid 21597 has been running since
2026-08-31T16:05:32Z on pin `3a8e5c03` throughout this round. Its installed
`bin/soak-run.sh` is round 1's wrapper and stays that way until an operator
re-runs `install-launchd.sh`; the record that wrapper produced is correct
(the bump used real 40-character pins, and the installed binary's sha256
matches `bin/jinnd.build`). What was broken is what the validator would have
accepted at the NEXT install, which is the kind of defect that only surfaces
when it matters.
