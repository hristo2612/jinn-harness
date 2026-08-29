//! The engines seam ON the operator surface: the routes, the engine
//! list's schema, and the pure mapping of the engines seam's typed error
//! onto this seam's. The engines CONTRACT itself is not restated here —
//! its one home is `plugins/engines/jinn-engine/README.md`; this module
//! only says how an operator reaches it over a transport.
//!
//! The definition depends on the engines definition for exactly two
//! facts it must not re-derive: the contract name an engine id is served
//! under, and the vocabulary of its error codes (the same shape as the
//! engines seam's own dependency on the settings seam for secret refs —
//! one home per fact, borrowed rather than copied).

use jinn_engine::{engine_contract, EngineError, ErrorCode as EngineErrorCode};
use serde::{Deserialize, Serialize};

use crate::{ApiError, ErrorCode, Extensions, API_VERSION};

/// The engines surface's path prefix.
pub const ENGINES_PATH: &str = "/v1/engines";

/// The methods the engines surface answers. A path this table shapes
/// under another method is a method refusal, not a route miss.
pub const ENGINE_METHODS: [&str; 3] = ["GET", "POST", "DELETE"];

/// One engines route: which operation the path names, and on which
/// engine. `List` is the only one that is not a call on a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineRoute {
    /// `GET /v1/engines`
    List,
    /// `GET /v1/engines/{engine}`
    Describe { engine: String },
    /// `POST /v1/engines/{engine}/runs`
    Run { engine: String },
    /// `GET /v1/engines/{engine}/runs/{run-id}`
    RunGet { engine: String, run: String },
    /// `DELETE /v1/engines/{engine}/runs/{run-id}`
    Cancel { engine: String, run: String },
}

impl EngineRoute {
    /// The engine the route addresses, if it addresses one.
    #[must_use]
    pub fn engine(&self) -> Option<&str> {
        match self {
            Self::List => None,
            Self::Describe { engine }
            | Self::Run { engine }
            | Self::RunGet { engine, .. }
            | Self::Cancel { engine, .. } => Some(engine),
        }
    }

    /// The engines-seam operation the route calls, if it calls one.
    #[must_use]
    pub fn operation(&self) -> Option<&'static str> {
        match self {
            Self::List => None,
            Self::Describe { .. } => jinn_engine::OP_DESCRIBE.into(),
            Self::Run { .. } => jinn_engine::OP_RUN.into(),
            Self::RunGet { .. } => jinn_engine::OP_RUN_GET.into(),
            Self::Cancel { .. } => jinn_engine::OP_CANCEL.into(),
        }
    }
}

/// Whether a path belongs to the engines surface at all — the provider
/// asks before it consults the static route table, so an engines path is
/// never answered by another route.
#[must_use]
pub fn is_engines_path(path: &str) -> bool {
    path.strip_prefix(ENGINES_PATH)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Matches a method + path (query already stripped) against the engines
/// surface. `None` is a miss — a malformed path, an unknown shape, or a
/// method this shape does not answer — and the caller answers it typed,
/// never by guessing a neighbouring route.
#[must_use]
pub fn engine_route(method: &str, path: &str) -> Option<EngineRoute> {
    let rest = path.strip_prefix(ENGINES_PATH)?;
    if rest.is_empty() {
        return (method == "GET").then_some(EngineRoute::List);
    }
    let mut segments = rest.strip_prefix('/')?.split('/');
    let engine = segments.next().filter(|segment| !segment.is_empty())?;
    let Some(collection) = segments.next() else {
        // `/v1/engines/{engine}` — one engine's own `describe`.
        return (method == "GET").then(|| EngineRoute::Describe {
            engine: engine.to_owned(),
        });
    };
    if collection != "runs" {
        return None;
    }
    let run = segments.next().filter(|segment| !segment.is_empty());
    if segments.next().is_some() {
        return None;
    }
    match (method, run) {
        ("POST", None) => Some(EngineRoute::Run {
            engine: engine.to_owned(),
        }),
        ("GET", Some(run)) => Some(EngineRoute::RunGet {
            engine: engine.to_owned(),
            run: run.to_owned(),
        }),
        ("DELETE", Some(run)) => Some(EngineRoute::Cancel {
            engine: engine.to_owned(),
            run: run.to_owned(),
        }),
        _ => None,
    }
}

/// The typed refusal for an engine id this API may not route to. The
/// GRANT is the authority the kernel enforces; the configured list is the
/// same fact told to the provider, so an id in neither is simply not
/// here — answered without a kernel call.
#[must_use]
pub fn no_such_engine(engine: &str) -> ApiError {
    ApiError::new(
        ErrorCode::NotFound,
        format!("this API routes to no engine {engine:?}"),
    )
}

/// The contract name an engine may be reached under, or the typed
/// refusal when this API may not route to it.
///
/// # Errors
///
/// `not-found` for an engine outside the configured list.
pub fn engine_routable(engines: &[String], engine: &str) -> Result<String, ApiError> {
    if engines.iter().any(|known| known == engine) {
        Ok(engine_contract(engine))
    } else {
        Err(no_such_engine(engine))
    }
}

/// The `run` payload: the request body with the PATH's engine as the
/// route. The path supplies `engine`; a body that names another engine
/// is not a second opinion.
#[must_use]
pub fn run_payload(engine: &str, body: serde_json::Value) -> serde_json::Value {
    let mut payload = match body {
        serde_json::Value::Object(fields) => serde_json::Value::Object(fields),
        _ => serde_json::json!({}),
    };
    payload["engine"] = serde_json::Value::String(engine.to_owned());
    payload
}

/// The `run-get` / `cancel` payload.
#[must_use]
pub fn run_id_payload(run: &str) -> serde_json::Value {
    serde_json::json!({ "run-id": run })
}

/// The engines seam's typed error as this seam's. The mapping is honest
/// in both directions: `unavailable` — the provider is mounted and
/// correct, this host cannot carry the run — stays `unavailable` and so
/// stays distinguishable from every other refusal, and the engines
/// seam's own code rides along verbatim as `engine-code` (additive) so
/// `failed` is never mistaken for `refused` by an operator reading the
/// answer.
#[must_use]
pub fn engine_api_error(error: &EngineError) -> ApiError {
    let code = match error.code {
        EngineErrorCode::Invalid => ErrorCode::Invalid,
        EngineErrorCode::NotFound => ErrorCode::NotFound,
        // The provider tried and the run failed: an upstream failure,
        // the same class of answer as a refusal, told apart by the
        // `engine-code` this carries.
        EngineErrorCode::Refused | EngineErrorCode::Failed => ErrorCode::Refused,
        EngineErrorCode::Unavailable => ErrorCode::Unavailable,
    };
    let mut mapped = ApiError::new(code, error.message.clone());
    if let Ok(engine_code) = serde_json::to_value(error.code) {
        mapped.extra.insert("engine-code".into(), engine_code);
    }
    mapped
}

/// One engine answer decoded into this seam's outcome: the `ok` value,
/// or the typed error its code maps onto. A malformed answer is
/// `refused` — the provider spoke, and not this contract.
///
/// # Errors
///
/// The mapped [`ApiError`].
pub fn decode_engine_answer(bytes: &[u8]) -> Result<serde_json::Value, ApiError> {
    let answer: jinn_engine::Answer = serde_json::from_slice(bytes).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("malformed engine answer: {error}"),
        )
    })?;
    answer
        .into_result()
        .map_err(|error| engine_api_error(&error))
}

/// One engine on the operator surface: what its provider says about
/// itself (`describe`), or the typed reason it could not say.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EngineEntry {
    pub engine: String,
    /// The contract name it is served under — what a profile edit swaps.
    pub contract: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub describe: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `GET /v1/engines` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EngineList {
    pub api_version: String,
    pub engines: Vec<EngineEntry>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Assembles the engine list from each configured engine's `describe`
/// outcome: an unreachable engine is a row with a typed error, never a
/// missing row and never a fault. Sorted by engine id.
#[must_use]
pub fn engine_list<I>(described: I) -> EngineList
where
    I: IntoIterator<Item = (String, Result<serde_json::Value, ApiError>)>,
{
    let mut engines: Vec<EngineEntry> = described
        .into_iter()
        .map(|(engine, described)| {
            let (describe, error) = match described {
                Ok(description) => (Some(description), None),
                Err(error) => (None, Some(error)),
            };
            EngineEntry {
                contract: engine_contract(&engine),
                engine,
                describe,
                error,
                extra: Extensions::new(),
            }
        })
        .collect();
    engines.sort_by(|left, right| left.engine.cmp(&right.engine));
    EngineList {
        api_version: API_VERSION.to_owned(),
        engines,
        extra: Extensions::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_five_routes_parse_with_both_path_parameters() {
        assert_eq!(engine_route("GET", ENGINES_PATH), Some(EngineRoute::List));
        let describe = engine_route("GET", "/v1/engines/claude").expect("describe");
        assert_eq!(
            describe,
            EngineRoute::Describe {
                engine: "claude".into()
            }
        );
        assert_eq!(describe.engine(), Some("claude"));
        assert_eq!(describe.operation(), Some(jinn_engine::OP_DESCRIBE));
        assert_eq!(
            engine_route("POST", "/v1/engines/claude/runs"),
            Some(EngineRoute::Run {
                engine: "claude".into()
            })
        );
        let get = engine_route("GET", "/v1/engines/claude/runs/claude-3").expect("run-get");
        assert_eq!(
            get,
            EngineRoute::RunGet {
                engine: "claude".into(),
                run: "claude-3".into()
            },
            "both parameters, and a run id keeps its dash"
        );
        assert_eq!(get.engine(), Some("claude"));
        assert_eq!(get.operation(), Some(jinn_engine::OP_RUN_GET));
        assert_eq!(
            engine_route("DELETE", "/v1/engines/default/runs/default-12"),
            Some(EngineRoute::Cancel {
                engine: "default".into(),
                run: "default-12".into()
            })
        );
        assert_eq!(EngineRoute::List.engine(), None);
        assert_eq!(EngineRoute::List.operation(), None);
        assert_eq!(
            EngineRoute::Run { engine: "e".into() }.operation(),
            Some(jinn_engine::OP_RUN)
        );
        assert_eq!(
            EngineRoute::Cancel {
                engine: "e".into(),
                run: "r".into()
            }
            .operation(),
            Some(jinn_engine::OP_CANCEL)
        );
    }

    #[test]
    fn a_malformed_engines_path_is_a_miss_not_a_wrong_route() {
        for path in [
            "/v1/engines/",
            "/v1/engines/claude/",
            "/v1/engines/claude/jobs",
            "/v1/engines/claude/runs/a/b",
            "/v1/engines/claude/runs//",
            "/v1/engines//runs/x",
            "/v1/enginesX",
            "/v1/status",
        ] {
            for method in ENGINE_METHODS {
                assert_eq!(engine_route(method, path), None, "{method} {path}");
            }
        }
        // The right shape under the wrong method is a miss too — the
        // provider turns it into a method refusal, never a run.
        assert_eq!(engine_route("GET", "/v1/engines/claude/runs"), None);
        assert_eq!(engine_route("POST", ENGINES_PATH), None);
        assert_eq!(engine_route("DELETE", "/v1/engines/claude"), None);
        assert_eq!(engine_route("POST", "/v1/engines/claude/runs/r"), None);
        assert_eq!(engine_route("DELETE", ENGINES_PATH), None);
        // Only the engines surface claims an engines path.
        assert!(is_engines_path(ENGINES_PATH));
        assert!(is_engines_path("/v1/engines/claude/runs"));
        assert!(!is_engines_path("/v1/enginesX"));
        assert!(!is_engines_path("/v1/status"));
    }

    #[test]
    fn an_engine_outside_the_configured_list_is_not_found_without_a_call() {
        let engines = vec!["default".to_owned(), "claude".to_owned()];
        assert_eq!(
            engine_routable(&engines, "claude").expect("routable"),
            "jinn:engine.claude"
        );
        let missing = engine_routable(&engines, "codex").expect_err("not routable");
        assert_eq!(missing.code, ErrorCode::NotFound);
        assert!(missing.detail.contains("codex"), "{missing:?}");
        assert_eq!(no_such_engine("codex").code, ErrorCode::NotFound);
        assert!(
            engine_routable(&[], "default").is_err(),
            "an API granted no engine routes to none"
        );
    }

    #[test]
    fn the_path_supplies_the_engine_and_the_run_id() {
        assert_eq!(
            run_payload("claude", json!({ "prompt": "hi", "engine": "codex" })),
            json!({ "prompt": "hi", "engine": "claude" }),
            "the path is the route, not the body"
        );
        assert_eq!(
            run_payload("default", json!({})),
            json!({ "engine": "default" })
        );
        assert_eq!(run_id_payload("claude-3"), json!({ "run-id": "claude-3" }));
    }

    #[test]
    fn every_engine_error_code_maps_onto_this_seams_vocabulary() {
        let cases = [
            (EngineErrorCode::Invalid, ErrorCode::Invalid, "invalid"),
            (EngineErrorCode::NotFound, ErrorCode::NotFound, "not-found"),
            (EngineErrorCode::Refused, ErrorCode::Refused, "refused"),
            (
                EngineErrorCode::Unavailable,
                ErrorCode::Unavailable,
                "unavailable",
            ),
            (EngineErrorCode::Failed, ErrorCode::Refused, "failed"),
        ];
        for (engine_code, api_code, name) in cases {
            let mapped = engine_api_error(&EngineError::new(engine_code, "why"));
            assert_eq!(mapped.code, api_code, "{name}");
            assert_eq!(mapped.detail, "why");
            assert_eq!(
                mapped.extra["engine-code"], name,
                "the seam's own code rides along, so {name} stays itself"
            );
        }
        // The environment gate the composition suite keys on stays
        // distinguishable from every other refusal.
        let gated = engine_api_error(&EngineError::unavailable("no claude CLI on this host"));
        assert_eq!(gated.code, ErrorCode::Unavailable);
        assert_ne!(
            gated.code,
            engine_api_error(&EngineError::new(EngineErrorCode::Failed, "boom")).code
        );
    }

    #[test]
    fn an_engine_answer_decodes_into_this_seams_outcome() {
        let ok = jinn_engine::Answer::ok(json!({ "run-id": "claude-3" }));
        assert_eq!(
            decode_engine_answer(&ok.encode()).expect("ok"),
            json!({ "run-id": "claude-3" })
        );
        let refused = jinn_engine::Answer::error(EngineError::unavailable("no CLI"));
        let error = decode_engine_answer(&refused.encode()).expect_err("typed");
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert_eq!(error.extra["engine-code"], "unavailable");
        let malformed = decode_engine_answer(b"not json").expect_err("typed");
        assert_eq!(
            malformed.code,
            ErrorCode::Refused,
            "a provider that did not speak this contract is a refusal, not a panic"
        );
    }

    #[test]
    fn the_engine_list_carries_a_describe_or_a_typed_error_per_engine() {
        let list = engine_list([
            (
                "default".to_owned(),
                Ok(json!({ "engine": "default", "provider": "engines/jinn-engine-echo" })),
            ),
            (
                "claude".to_owned(),
                Err(ApiError::new(
                    ErrorCode::Unavailable,
                    "jinn:engine.claude is not resolvable",
                )),
            ),
        ]);
        assert_eq!(list.api_version, API_VERSION);
        assert_eq!(
            list.engines
                .iter()
                .map(|entry| entry.engine.as_str())
                .collect::<Vec<_>>(),
            ["claude", "default"],
            "sorted by engine id"
        );
        assert_eq!(list.engines[0].contract, "jinn:engine.claude");
        assert!(list.engines[0].describe.is_none());
        assert_eq!(
            list.engines[0].error.as_ref().expect("typed").code,
            ErrorCode::Unavailable,
            "an unmounted engine is an ordinary answer, not a fault"
        );
        assert_eq!(
            list.engines[1].describe.as_ref().expect("described")["engine"],
            "default"
        );
        assert!(list.engines[1].error.is_none());
        assert_eq!(
            serde_json::to_value(&list.engines[1]).expect("encodes"),
            json!({ "engine": "default", "contract": "jinn:engine.default",
                    "describe": { "engine": "default", "provider": "engines/jinn-engine-echo" } }),
            "no empty error key on a live engine"
        );
    }

    #[test]
    fn the_engine_list_round_trips_unknown_fields() {
        let wire = json!({
            "api-version": "0.9.0",
            "engines": [{ "engine": "e", "contract": "jinn:engine.e", "future": true }],
            "list-future": [1]
        });
        let list: EngineList = serde_json::from_value(wire.clone()).expect("decodes");
        assert_eq!(list.engines[0].extra["future"], true);
        assert_eq!(serde_json::to_value(&list).expect("encodes"), wire);
    }
}
