# A saved patch can still owe its notification

The scheduler declares on every wake to reconcile direct overlay edits and
settings-provider replacement without restarting the scheduler. During a hot
patch, a declaration can queue on the provider before that provider emits back
to the scheduler. At the pinned kernel, the second crossing correctly receives
`cycle`: no listener ran. Discarding that result lost the notice, while the queued
declaration could still make the new schedule appear to work.

The profile provider now retains that exact hot notice only for the direct
owner-to-provider `jinn:settings.declare` wait. It returns the applied patch with
`notification.state: pending`, answers the opposing declaration, and arms a
one-shot clock callback from that declaration's return path. The callback enters
only after the current guest call returns. A renewed direct declaration cycle
waits for another matching declaration to return; it never spins or sleeps inside
the blocked call. Ordinary successful hot delivery remains synchronous.

There is one outstanding slot per provider incarnation. While occupied, every
new patch is refused before persistence. A newer accepted revision therefore
cannot overtake the retained payload. Only the kernel's whole-walk pre-delivery
cycle refusal is replayable. Other dispatch failures and empty replies become
`unconfirmed`, with no replay. A failure to arm the callback stays observable on
GET and keeps the slot occupied; subsequent declarations do not rearm a failed
clock request. Recovery requires an explicit provider lifecycle action.

The slot is not durable. Restart or removal discards it; settings already saved
in the profile's overlay store remain, and the scheduler's declarations reconcile
that state. An absent notification record in a new incarnation is not evidence
of delivery. There is no exactly-once claim across lifecycle boundaries, and a
successful collecting walk does not attribute individual replies to listeners.
Direct external edits still reconcile on the next declaration; the retained
notice describes its original revision, not a later external edit.

The Settings API adapter carries pending/unconfirmed results into the existing
save status and reload path, without resubmitting the patch. If a provider loses
a previously observed record, reload says unconfirmed. If a save spans namespaces
and one goes pending, remaining namespace edits are not sent and the page shows
the actual saved document. The owning wire behavior is in the
[settings contract](../../plugins/settings/jinn-settings/README.md).

The controlled loader experiment holds the provider immediately before its emit,
then releases it only after the real broker records the opposing declaration.
The prior implementation saved the patch and updated scheduler state with no
successful notification. With the repair, two forced successive cycles produce
one successful serial walk with one listener and zero failures. Separate probes
cover a denied clock, backpressure, and pending restart/removal. These diagnostic
barriers are test fixtures, not production code. The ordinary composition suite
still requires the actual attributed successful walk; state alone is insufficient.
