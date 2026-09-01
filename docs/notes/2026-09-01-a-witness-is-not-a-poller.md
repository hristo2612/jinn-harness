# A witness is not a poller, and a marking is not a law

*Harness pin-bump 5 — adopting M2-K13 (`jinnd` `901d207`), plugin world
0.6.0 → 0.8.0.*

## The bump was a migration, and planning it as one was the whole trick

`KERNEL-PIN.md`'s procedure is four mechanical steps and it reads like a
version edit. This one was not. The world moved 0.6.0 → 0.8.0, and the
kernel refuses an artifact built on a different world with `artifact is
not a loadable component of the plugin world` — so at the moment the
vendored `wit/` changed, every guest in this repo became unloadable and
the daemon would have come up with nothing at all. The rebuild is the
first task; the pin edit is the last. Doing it the other way round
produces a composition suite failing for a reason that looks like the
seam and is not.

Nothing in the repo needed a source change for the world move itself. The
guests generate their bindings from `kernel-pin/wit`, the additive
`cycle` case on `kernel-error` is matched nowhere exhaustively, and the
kits rebuild on the changed input. That is the pin discipline paying off:
the blast radius of a world bump is a rebuild, because no crate reaches
past the vendored surface.

## What the kernel now offers, and why it changed a claim rather than an API

`jinn:introspect@0.4.0` publishes every `FiberTransition` the kernel
commits on the reserved topic `jinn:introspect/transitions`, to listeners
holding that contract's grant. Additive: no operation changed shape. The
interesting part is not the topic, it is the three properties around it —
a delivery never precedes its own ledger row, loss is bounded and counted
twice (a `PublishDropped` row and a gap in the listener's `ordinal`), and
there is no replay, so a late joiner is told how much it missed rather
than left to assume it missed nothing.

Those three are what let a consumer be honest. A stream without them is
worse than no stream: it looks complete.

## The seam had a decision, and the decision had an expiry date

`jinn_plugins`'s module doc used to carry a refusal, at the place a
person comes to add an event surface: *do not*. The reasoning was that
`jinn:introspect` was a pull, so the only event this seam could emit was
one a poller synthesised by diffing two snapshots — a transition
announced without being witnessed and without a time, which is the
fabrication class the whole seam exists to kill one layer up.

That reasoning is unchanged and still correct. What changed is its
premise. So the seam now subscribes, and it still emits nothing of its
own: a sighting is the kernel's own record, delivered. The rule was never
"no events" — it was "what this seam does not witness, it does not
report", and the rule outlived the absence it produced.

`witness` keeps a bounded log per incarnation and answers
`GET /v1/plugins/{catalog}/{id}/transitions`. Two design points are worth
naming because both were tempting to get wrong:

- **Two losses, two owners.** `missed` is what the kernel published and
  this catalog never received; `evicted` is what this catalog witnessed
  and then dropped from its own bound. One "incomplete" flag would have
  let either pass for the other, and only one of them is a kernel defect.
- **A history, never a reading of now.** The entry's `lifecycle` is a
  join over a snapshot and answers *what is this doing*. A sighting
  answers *what did the kernel do to this fiber, and when in its own
  order*. Merging them would have produced exactly the surface that
  claims more than it saw.

The kernel deliberately withholds `cause` — nothing in `jinn:introspect`
answers WHY, so publishing it would widen the grant. Rather than leave a
reason-shaped hole, a witnessed reading carries
`Reason::CauseNotDelivered`, which names `jinn:ledger` as where the cause
lives. That is a positive fact about the contract. It is not an
`unknown`, and it is not a neighbouring line pressed into service.

## The canary went red, which is the point of a canary

Harness 2.7 shipped `UNREACHABLE_AT_PIN`, marking `mounted`,
`activating` and `interrupted` unreachable, guarded by
`no-transient-reading-at-this-pin`. It was built to fail on this exact
day. It did, and the transcript is in the composition test's own output:
fed the readings this daemon actually delivered, the predicate refuses
every one of the three.

A red canary here is the designed outcome. It is worth saying plainly,
because the same red filed as a regression would have been "fixed" by
deleting the guard.

The retirement that followed is the part worth being careful about.
`UNREACHABLE_AT_PIN` said *no consumer at pin `3a8e5c0` can ever be
handed one of these*. That was two claims wearing one name: a
MEASUREMENT (a pull answered at rest cannot reach a state between two
rests — 190 reads, all `active`) and a GENERALISATION from it (therefore
nothing can). The measurement is still true and was reproduced at the new
pin. The generalisation was never about the readings; it was about the
kernel's read surface, and the kernel moved.

So the marking was retired and its true half kept:
`NOT_FROM_A_SNAPSHOT` and `no-transient-reading-from-a-snapshot`. Not a
rename — a narrowing, with the wider claim struck and its correction in
`FINDINGS.md` #41 beside the original rather than in place of it.

Deleting the guard outright was the other option and it was wrong twice
over. An entry's lifecycle is still snapshot-derived, so one carrying a
transient reading is still reporting what it cannot have seen. And the
mutation harness fails on a check no mutant reaches AND on a mutant no
check catches: the `eternally activating` mutant is caught by this guard
alone, so removing it would have left a named defect uncaught while every
test stayed green.

## What this did not touch

The M2 duty soak. It is acceptance evidence with an audit due, running
`3a8e5c03`, and moving it onto this kernel is a separate supervised
decision. Worth recording why the bump could not disturb it even by
accident: `soak-run.sh` reads both `running-pin` and `harness-pin` from
the install record written at build time, not live from `KERNEL-PIN.md`,
so a pin bump landing on `main` changes nothing the soak reports. That is
`FINDINGS.md` #42's workaround earning its keep in a way it was not
designed for.
