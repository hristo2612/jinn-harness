//! WITNESSED TRANSITIONS: what this seam SAW the kernel do.
//!
//! # Why this module exists at all
//!
//! Every other answer in this seam is a READING OF NOW, joined from a
//! snapshot pull. A snapshot is taken at rest, so three of the eleven
//! readings — `mounted`, `activating`, `interrupted` — name a fiber
//! between two rests and no pull can ever reach one. That was measured
//! rather than assumed (`FINDINGS.md` #41: 189 consecutive catalog reads
//! across a real restart, every one `active`, while the kernel's own
//! ledger recorded the whole path), and until the kernel gained a
//! publish path the honest response was to mark the three words
//! unreachable and guard the marking with a canary.
//!
//! Kernel pin `901d207` gains the publish path (`jinn:introspect@0.4.0`,
//! plugin world 0.8.0). A holder of `jinn:introspect` SUBSCRIBES to the
//! reserved topic [`TRANSITIONS_TOPIC`] and is handed every fiber
//! transition the kernel commits. So the three readings stop being
//! unreachable — and they become reachable HERE, on this surface, and
//! nowhere else. A reading derived from a snapshot still cannot carry
//! one; that limit is narrower than a pin-wide unreachability, and it is
//! what `checks::no_transient_reading_from_a_snapshot` now guards.
//!
//! # This is a HISTORY, never a reading of now
//!
//! A sighting says *the kernel committed this fiber from `from` into
//! `to` at ordinal N*. It does not say what the fiber is doing now, and
//! it must never be read as the entry's live lifecycle. The two answers
//! are deliberately different surfaces for that reason.
//!
//! # Loss is counted, and whose loss it was is never blurred
//!
//! The contract's back-pressure is bounded and counted: a listener slow
//! enough to fill its lane loses transitions, and every loss shows as a
//! gap in the `ordinal` it receives. This log is bounded too, so it can
//! evict what it already witnessed. Those are two different losses with
//! two different owners and they are counted separately ([`Stream`]):
//! `missed` is what the kernel published and this catalog never saw,
//! `evicted` is what this catalog saw and then dropped. A single
//! "incomplete" flag would have let either pass for the other.

use serde::{Deserialize, Serialize};

use crate::lifecycle::{Lifecycle, Reason, Snapshot};
use crate::{ErrorCode, PluginsError};

/// The kernel-reserved topic every committed fiber transition is
/// published on (`jinn:introspect` 0.4.0). Its one home: a subscription
/// and the contract it is granted under can never drift apart.
pub const TRANSITIONS_TOPIC: &str = "jinn:introspect/transitions";

/// How many sightings one catalog keeps. Small on purpose, exactly as
/// the ledger window is: the bound travels in every answer, so a bound
/// that is easy to reason about is worth more than one that is merely
/// large.
pub const WITNESS_CAPACITY: usize = 256;

/// What a sighting IS, travelling in the answer rather than only in this
/// file. Its one home.
pub const WITNESS_QUALIFIER: &str =
    "a witnessed history, never a reading of now: each sighting is one transition the kernel \
     COMMITTED and published on `jinn:introspect/transitions`, in the order it committed them. \
     `missed` counts transitions the kernel published that this catalog never received — \
     ordinal gaps, plus everything before the first ordinal, which is a late join and not a \
     loss; `evicted` counts sightings this catalog witnessed and then dropped from its own \
     bounded log. The kernel's `cause` is deliberately not published on this contract; read \
     `jinn:ledger` for it";

/// One transition the kernel committed, exactly as it publishes it. The
/// field names are the contract's (`jinn:introspect@0.4.0`), and there
/// is no `cause`: the kernel withholds it because nothing in that
/// contract's pull answers WHY, so delivering it would widen the grant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Transition {
    /// The profile entry, exactly as `entry.id` names it.
    pub entry: String,
    /// The fiber, exactly as `entry.fiber` names it.
    pub fiber: u64,
    /// The live incarnation when the transition was committed; absent
    /// when none was installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incarnation: Option<u64>,
    /// The states, from `entry.state`'s vocabulary.
    pub from: String,
    pub to: String,
    /// This kernel process's publish count, from 1. Gaps are losses; a
    /// first value above 1 is a late join.
    pub ordinal: u64,
    /// A ledger sequence already committed when the delivery began; the
    /// transition's own row is at or before it.
    pub committed_by: u64,
}

impl Transition {
    /// One delivered payload, or the typed reason it is not one. A
    /// malformed delivery is never silently dropped and never a fault:
    /// the kernel is the only publisher here, so a payload this seam
    /// cannot read is a fact about the pin worth counting.
    ///
    /// # Errors
    ///
    /// [`PluginsError`] when the payload is not this contract's record.
    pub fn parse(payload: &[u8]) -> Result<Self, PluginsError> {
        serde_json::from_slice(payload).map_err(|error| {
            PluginsError::new(
                ErrorCode::Invalid,
                format!("a {TRANSITIONS_TOPIC} delivery this seam cannot read: {error}"),
            )
        })
    }

    /// The reading this transition LANDED IN, through the seam's one
    /// reading law — never a second, parallel table.
    ///
    /// The transition record carries no `unserved`, so this reading is
    /// bounded by exactly that: it is what the kernel committed the
    /// fiber into, not a claim about what the live incarnation owes. A
    /// reason it cannot know is stated as [`Reason::CauseNotDelivered`],
    /// which is a positive fact about the contract rather than a
    /// sentinel.
    #[must_use]
    pub fn reading(&self) -> Lifecycle {
        Lifecycle::read(
            Some(&Snapshot {
                state: Some(self.to.clone()),
                incarnation: self.incarnation,
                unserved: None,
                provisions: Vec::new(),
            }),
            Reason::cause_not_delivered(),
            Reason::cause_not_delivered(),
        )
    }
}

/// What this catalog's subscription has and has not seen. Every field is
/// counted, because the contract's loss is bounded and counted and a
/// consumer of a lossy stream that cannot see the loss is worse off than
/// one with no stream at all.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Stream {
    /// Deliveries this catalog received and read.
    pub delivered: u64,
    /// Transitions the kernel published that this catalog never
    /// received: ordinal gaps, plus everything before the first ordinal.
    pub missed: u64,
    /// Deliveries received on the topic that this seam could not read.
    pub malformed: u64,
    /// Sightings this catalog witnessed and then dropped from its own
    /// bounded log. ITS loss, never the kernel's.
    pub evicted: u64,
    /// How many sightings the log holds at most.
    pub capacity: u64,
    /// The first ordinal received; `0` before anything arrives. A value
    /// above 1 is a late join, stated rather than hidden.
    pub first_ordinal: u64,
    /// The highest ordinal received; `0` before anything arrives.
    pub last_ordinal: u64,
}

/// One sighting on the wire: the kernel's own record, and the reading it
/// landed in.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Sighting {
    #[serde(flatten)]
    pub transition: Transition,
    /// The reading `to` lands in, through the seam's one reading law.
    pub lifecycle: Lifecycle,
}

/// A catalog's bounded log of what it witnessed.
pub struct Witness {
    kept: Vec<Transition>,
    stream: Stream,
}

impl Witness {
    /// An empty log with a stated bound.
    #[must_use]
    pub const fn new(capacity: usize) -> Self {
        Self {
            kept: Vec::new(),
            stream: Stream {
                delivered: 0,
                missed: 0,
                malformed: 0,
                evicted: 0,
                capacity: capacity as u64,
                first_ordinal: 0,
                last_ordinal: 0,
            },
        }
    }

    /// One delivery from the kernel. A payload this seam cannot read is
    /// COUNTED and the log keeps serving: a catalog that failed its
    /// fiber over a delivery it could not parse would take a working
    /// read surface down with it (R11 — failure is local, and this is
    /// not even a failure).
    pub fn deliver(&mut self, payload: &[u8]) {
        match Transition::parse(payload) {
            Ok(transition) => self.record(transition),
            Err(_) => self.stream.malformed += 1,
        }
    }

    /// One parsed transition, with the ordinal accounting the contract
    /// requires.
    pub fn record(&mut self, transition: Transition) {
        let ordinal = transition.ordinal;
        if self.stream.first_ordinal == 0 {
            self.stream.first_ordinal = ordinal;
            self.stream.missed += ordinal.saturating_sub(1);
        } else if ordinal > self.stream.last_ordinal.saturating_add(1) {
            self.stream.missed += ordinal - self.stream.last_ordinal - 1;
        }
        if ordinal > self.stream.last_ordinal {
            self.stream.last_ordinal = ordinal;
        }
        self.stream.delivered += 1;
        let capacity = self.stream.capacity as usize;
        self.kept.push(transition);
        while self.kept.len() > capacity {
            self.kept.remove(0);
            self.stream.evicted += 1;
        }
    }

    /// Every sighting of one entry, oldest first — the order the kernel
    /// committed them in, which is the order they were delivered in.
    #[must_use]
    pub fn sightings(&self, entry: &str) -> Vec<Sighting> {
        self.kept
            .iter()
            .filter(|transition| transition.entry == entry)
            .map(|transition| Sighting {
                transition: transition.clone(),
                lifecycle: transition.reading(),
            })
            .collect()
    }

    /// What this subscription has and has not seen.
    #[must_use]
    pub const fn stream(&self) -> Stream {
        self.stream
    }

    /// Withdrawal: the subscription is gone, so what it witnessed is no
    /// longer being fed and is not kept as a second source of truth.
    pub fn clear(&mut self) {
        self.kept.clear();
        self.stream = Stream {
            capacity: self.stream.capacity,
            ..Stream::default()
        };
    }
}

/// One entry's witnessed history, as the operator surface answers it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Witnessed {
    pub api_version: String,
    pub catalog: String,
    /// Which package witnessed it — a subscription belongs to the
    /// incarnation that made it, so a swap resets the stream and the
    /// answer says who is answering.
    pub served_by: String,
    pub plugin: String,
    pub witnessed: Vec<Sighting>,
    pub stream: Stream,
    pub qualifier: String,
}

impl Witnessed {
    /// One entry's answer, from a live log.
    #[must_use]
    pub fn of(catalog: &str, served_by: &str, plugin: &str, witness: &Witness) -> Self {
        Self {
            api_version: crate::API_VERSION.to_owned(),
            catalog: catalog.to_owned(),
            served_by: served_by.to_owned(),
            plugin: plugin.to_owned(),
            witnessed: witness.sightings(plugin),
            stream: witness.stream(),
            qualifier: WITNESS_QUALIFIER.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published(entry: &str, from: &str, to: &str, ordinal: u64) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "entry": entry, "fiber": 4, "incarnation": 7,
            "from": from, "to": to, "ordinal": ordinal, "committed-by": 100 + ordinal,
        }))
        .expect("encodes")
    }

    #[test]
    fn a_witnessed_transition_reads_in_the_seams_one_reading_law() {
        // The whole point of the subscription: the three readings a
        // snapshot cannot produce arrive here, and they arrive through
        // the SAME reading law the snapshot answers use — never a second
        // table that could drift from it.
        let mut witness = Witness::new(8);
        for (index, (from, to)) in [
            ("active", "unloading"),
            ("unloading", "pending"),
            ("pending", "loading"),
            ("loading", "active"),
        ]
        .into_iter()
        .enumerate()
        {
            witness.deliver(&published("a", from, to, index as u64 + 1));
        }
        let read: Vec<&str> = witness
            .sightings("a")
            .iter()
            .map(|sighting| sighting.lifecycle.name())
            .collect();
        assert_eq!(read, ["interrupted", "mounted", "activating", "active"]);
        // And every one of the three is a reading NO snapshot answer may
        // carry — which is exactly why this surface exists.
        for transient in crate::snapshot::NOT_FROM_A_SNAPSHOT {
            assert!(
                read.contains(&transient),
                "`{transient}` is unreachable from a snapshot and was not witnessed either"
            );
        }
    }

    #[test]
    fn a_reading_from_a_transition_carries_a_reason_that_is_not_correlated() {
        // The kernel deliberately withholds `cause`. The reading says
        // so positively and points at the contract that does carry it,
        // rather than naming a neighbouring ledger line or an `unknown`.
        let mut witness = Witness::new(4);
        witness.deliver(&published("a", "loading", "failed", 1));
        let sighting = witness.sightings("a").pop().expect("a sighting");
        let wire = serde_json::to_value(&sighting).expect("encodes");
        assert_eq!(wire["lifecycle"]["state"], "failed");
        assert_eq!(wire["lifecycle"]["reason"]["from"], "cause-not-delivered");
        assert!(wire["lifecycle"]["reason"]["qualifier"]
            .as_str()
            .expect("a qualifier")
            .contains("jinn:ledger"));
        for correlated in ["seq", "detail", "kind"] {
            assert!(wire["lifecycle"]["reason"][correlated].is_null());
        }
    }

    #[test]
    fn a_gap_in_the_ordinals_is_counted_as_the_kernels_loss() {
        // The contract's back-pressure is bounded and COUNTED: a gap is
        // the listener's only in-band evidence that the kernel dropped
        // something. A consumer that reported the stream as whole would
        // be worse off than one with no stream at all.
        let mut witness = Witness::new(8);
        witness.deliver(&published("a", "pending", "loading", 1));
        witness.deliver(&published("a", "loading", "active", 4));
        let stream = witness.stream();
        assert_eq!(stream.delivered, 2);
        assert_eq!(stream.missed, 2, "ordinals 2 and 3 never arrived");
        assert_eq!(stream.evicted, 0, "this log dropped nothing of its own");
        assert_eq!(stream.first_ordinal, 1);
        assert_eq!(stream.last_ordinal, 4);
    }

    #[test]
    fn a_late_join_is_stated_and_is_not_dressed_up_as_a_complete_history() {
        // There is no replay. A first ordinal above 1 says exactly how
        // many transitions preceded the subscription, so a consumer can
        // tell "nothing happened" from "I was not there".
        let mut witness = Witness::new(8);
        witness.deliver(&published("a", "active", "unloading", 12));
        assert_eq!(witness.stream().first_ordinal, 12);
        assert_eq!(witness.stream().missed, 11);
        assert_eq!(witness.stream().delivered, 1);
    }

    #[test]
    fn this_logs_own_eviction_is_never_counted_as_the_kernels_loss() {
        // Two different losses with two different owners. Folding them
        // into one "incomplete" flag would let either pass for the
        // other, and only one of them is a kernel defect.
        let mut witness = Witness::new(2);
        for ordinal in 1..=5 {
            witness.deliver(&published("a", "active", "unloading", ordinal));
        }
        let stream = witness.stream();
        assert_eq!(stream.missed, 0, "the kernel lost nothing");
        assert_eq!(stream.evicted, 3, "this log dropped three of its own");
        assert_eq!(witness.sightings("a").len(), 2);
        assert_eq!(stream.capacity, 2);
    }

    #[test]
    fn an_unreadable_delivery_is_counted_and_the_catalog_keeps_serving() {
        // R11: a delivery this seam cannot read is a fact about the pin,
        // not a reason to fail a fiber and take a working read surface
        // down with it.
        let mut witness = Witness::new(4);
        witness.deliver(b"not a transition");
        witness.deliver(&published("a", "pending", "loading", 1));
        assert_eq!(witness.stream().malformed, 1);
        assert_eq!(witness.stream().delivered, 1);
        assert_eq!(witness.sightings("a").len(), 1);
    }

    #[test]
    fn sightings_are_one_entrys_own_and_stay_in_the_order_the_kernel_committed_them() {
        let mut witness = Witness::new(8);
        witness.deliver(&published("a", "pending", "loading", 1));
        witness.deliver(&published("b", "pending", "loading", 2));
        witness.deliver(&published("a", "loading", "active", 3));
        let ordinals: Vec<u64> = witness
            .sightings("a")
            .iter()
            .map(|sighting| sighting.transition.ordinal)
            .collect();
        assert_eq!(ordinals, [1, 3]);
        assert!(witness.sightings("c").is_empty());
    }

    #[test]
    fn withdrawing_the_subscription_withdraws_what_it_witnessed() {
        // A log that outlived its subscription would be a second source
        // of truth about a machine nobody is watching any more.
        let mut witness = Witness::new(4);
        witness.deliver(&published("a", "pending", "loading", 1));
        witness.clear();
        assert!(witness.sightings("a").is_empty());
        assert_eq!(
            witness.stream(),
            Stream {
                capacity: 4,
                ..Stream::default()
            }
        );
    }

    #[test]
    fn the_answer_carries_the_bound_it_was_read_under() {
        // M2-K12's shape: a limit travels with the evidence it
        // qualifies, in the answer the consumer reads.
        let mut witness = Witness::new(2);
        witness.deliver(&published("a", "pending", "loading", 3));
        let answer = Witnessed::of("main", "plugins/jinn-plugins-profile", "a", &witness);
        let wire = serde_json::to_value(&answer).expect("encodes");
        assert_eq!(wire["catalog"], "main");
        assert_eq!(wire["served-by"], "plugins/jinn-plugins-profile");
        assert_eq!(wire["stream"]["capacity"], 2);
        assert_eq!(wire["stream"]["missed"], 2);
        assert!(wire["qualifier"]
            .as_str()
            .expect("a qualifier")
            .contains("witnessed history"));
    }
}
