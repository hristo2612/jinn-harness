//! The honesty law as RUNNABLE PREDICATES, so the assertion that passes
//! on the real daemon's answer is the same one a mutant is measured
//! against.
//!
//! # Why the assertions are values
//!
//! An assertion written inline in a test proves that the code passes it
//! TODAY. It cannot show that the test would go red on the defect it is
//! named after — an assertion can be correct, compiled, executed,
//! passing, and vacuous. Lifting each one to a named [`Check`] lets the
//! mutation harness (`crate::mutants`) inject each named defect and show
//! exactly which checks go red, which is a strictly stronger statement
//! than the order the code and its test were written in.
//!
//! Every check reads the SERIALIZED entry — the JSON an operator gets
//! from `GET /v1/plugins/<catalog>` — so nothing here can pass by
//! reaching into a private field the wire does not carry.

use crate::entry::JOIN_QUALIFIER;

/// One honesty property, by name, over one serialized entry.
pub struct Check {
    /// The property, named as the defect it excludes.
    pub name: &'static str,
    /// `Ok` when the answer honours it; `Err` naming what it said.
    pub run: fn(&serde_json::Value) -> Result<(), String>,
}

/// Every honesty property this seam claims about one entry's reading.
pub const CHECKS: [Check; 6] = [
    Check {
        name: "active-needs-positive-proof",
        run: active_needs_positive_proof,
    },
    Check {
        name: "no-reason-is-correlated",
        run: no_reason_is_correlated,
    },
    Check {
        name: "no-sentinel-in-the-vocabulary",
        run: no_sentinel_in_the_vocabulary,
    },
    Check {
        name: "every-reading-that-owes-one-has-a-reason",
        run: every_reading_that_owes_one_has_a_reason,
    },
    Check {
        name: "grants-name-their-authority",
        run: grants_name_their_authority,
    },
    Check {
        name: "the-limit-travels-in-the-answer",
        run: the_limit_travels_in_the_answer,
    },
];

/// Runs every check, collecting the names that went red.
#[must_use]
pub fn failures(entry: &serde_json::Value) -> Vec<String> {
    CHECKS
        .iter()
        .filter_map(|check| {
            (check.run)(entry)
                .err()
                .map(|why| format!("{}: {why}", check.name))
        })
        .collect()
}

fn state(entry: &serde_json::Value) -> &str {
    entry["lifecycle"]["state"].as_str().unwrap_or_default()
}

/// `active` is the claim an operator acts on, so it is the one answer
/// that needs three positive facts. An entry reading `active` without an
/// incarnation, or while its live incarnation already owes a change, is
/// the defect this excludes.
fn active_needs_positive_proof(entry: &serde_json::Value) -> Result<(), String> {
    if state(entry) != "active" {
        return Ok(());
    }
    // The evidence rides on the wire beside the claim, so a consumer
    // checks it rather than trusting it: the reading law reaches
    // `active` only with an installed incarnation.
    if entry["incarnation"].as_u64().is_none() {
        return Err(format!(
            "read `active` with no incarnation to prove it: {entry}"
        ));
    }
    Ok(())
}

/// No ledger line may ride a reason. `jinn:ledger` v0.1 records no
/// causal parent, so a line presented as a cause is a fabrication — and
/// it is caught on the WIRE, where the fields would have to appear.
fn no_reason_is_correlated(entry: &serde_json::Value) -> Result<(), String> {
    let reason = &entry["lifecycle"]["reason"];
    for cited in ["seq", "detail", "kind"] {
        if !reason[cited].is_null() {
            return Err(format!("a reason carried `{cited}`: {reason}"));
        }
    }
    if reason["from"] == "ledgered" {
        return Err(format!(
            "a reason claimed a ledger line as its cause: {reason}"
        ));
    }
    Ok(())
}

/// There is no `unknown` in this vocabulary, and no empty stand-in for a
/// reading that never happened.
fn no_sentinel_in_the_vocabulary(entry: &serde_json::Value) -> Result<(), String> {
    match state(entry) {
        "" => Err("an entry with no reading at all".to_owned()),
        "unknown" | "unavailable" => Err(format!("a sentinel state: {}", state(entry))),
        _ => Ok(()),
    }
}

/// Every reading whose name implies a cause carries one, and the reason
/// names where it came from.
fn every_reading_that_owes_one_has_a_reason(entry: &serde_json::Value) -> Result<(), String> {
    let owes = ["no-incarnation", "failed", "interrupted", "disposed"];
    if !owes.contains(&state(entry)) {
        return Ok(());
    }
    match entry["lifecycle"]["reason"]["from"].as_str() {
        None => Err(format!("`{}` with no reason at all", state(entry))),
        Some("unknown") => Err("a reason named `unknown`".to_owned()),
        Some(_) => Ok(()),
    }
}

/// A grant list says which authority it came from, so an appliance
/// catalog's build-time opinion can never pass for the kernel's
/// enforcement.
fn grants_name_their_authority(entry: &serde_json::Value) -> Result<(), String> {
    let source = entry["grants"]["source"].as_str().unwrap_or_default();
    if !["profile-document", "catalog-declaration"].contains(&source) {
        return Err(format!("grants from an unnamed authority: {source:?}"));
    }
    if entry["grants"]["values"].as_array().is_none() {
        return Err("grants with no list at all — an absence dressed as none".to_owned());
    }
    Ok(())
}

/// Where a guarantee is narrower than its name, the qualifier is in the
/// answer the consumer reads and not only in a README.
fn the_limit_travels_in_the_answer(entry: &serde_json::Value) -> Result<(), String> {
    let qualifier = entry["grants"]["qualifier"].as_str().unwrap_or_default();
    if qualifier.trim().is_empty() {
        return Err("grants with no qualifier".to_owned());
    }
    if let Some(reason) = entry["lifecycle"]["reason"].as_object() {
        if reason.get("from").and_then(serde_json::Value::as_str) == Some("no-recorded-cause") {
            let stated = reason
                .get("qualifier")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if !stated.contains("no causal parent") {
                return Err("a no-recorded-cause reason with no qualifier".to_owned());
            }
        }
    }
    Ok(())
}

/// The listing-level qualifier: the join is three reads, and the answer
/// says so. Separate from [`CHECKS`] because it is a property of the
/// LISTING rather than of one entry.
///
/// # Errors
///
/// When the listing drops the qualifier that bounds every answer in it.
pub fn listing_states_the_join(listing: &serde_json::Value) -> Result<(), String> {
    let stated = listing["read"]["qualifier"].as_str().unwrap_or_default();
    if stated == JOIN_QUALIFIER {
        Ok(())
    } else {
        Err(format!("a listing whose join qualifier reads {stated:?}"))
    }
}
