//! Minimal HTTP/1.1 for the operator API's loopback provider: parse one
//! request out of a growing byte buffer (headers + `Content-Length` body,
//! bounded), read the credential it presents (`Authorization: Bearer`),
//! frame one JSON response, map the seam's typed error codes to status
//! codes. No chunked encoding, no keep-alive (every response
//! closes), no percent-decoding beyond what an operator path needs —
//! R10-sized on purpose: this is the whole transport.

use jinn_api::{Answer, ApiError, ErrorCode, Outcome};

/// The largest request head (request line + headers) accepted.
pub const HEAD_CAP: usize = 16 * 1024;
/// The largest request body accepted (a profile patch is small).
pub const BODY_CAP: usize = 256 * 1024;

/// One parsed request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The path without its query string.
    pub path: String,
    /// The query string's `key=value` pairs, in order (no decoding).
    pub query: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Bytes of the buffer this request consumed.
    pub consumed: usize,
    /// The credential the connection PRESENTED: the token of an
    /// `Authorization: Bearer <token>` header (RFC 6750; scheme
    /// case-insensitive, value trimmed), or `None` when the request
    /// carries none. Nothing else on a request is a credential — not a
    /// query parameter (URLs land in logs and shell history), not a
    /// cookie (a session is not an identity here), not another scheme
    /// (a username would imply accounts). The door presents exactly
    /// this to `jinn:auth`, and presents NOTHING when it is `None`.
    pub bearer: Option<String>,
}

impl Request {
    /// The query parameters as a JSON object (last value wins; the seam's
    /// request schemas read numbers from strings leniently via
    /// [`query_json`]).
    #[must_use]
    pub fn query_json(&self) -> serde_json::Value {
        query_json(&self.query)
    }
}

/// Query pairs as a JSON object: integers become numbers, `true`/`false`
/// become booleans, everything else stays a string.
///
/// The two coercions are the seam's scalar kinds and nothing more. A
/// query is the only way a READ carries a parameter (a read never takes a
/// body), so a seam field that is a `bool` — `roots-only` on the todos
/// surface — is unreachable over HTTP unless this codec can produce one.
/// The coercion is deliberately NOT generous: `"yes"`, `"1"` and `"True"`
/// stay strings and are refused by the seam that reads them, because a
/// codec that guessed at intent would make a typo mean `true`.
#[must_use]
pub fn query_json(pairs: &[(String, String)]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (key, value) in pairs {
        let json = match value.as_str() {
            "true" => serde_json::Value::Bool(true),
            "false" => serde_json::Value::Bool(false),
            other => other
                .parse::<u64>()
                .map_or_else(|_| serde_json::Value::String(value.clone()), Into::into),
        };
        object.insert(key.clone(), json);
    }
    serde_json::Value::Object(object)
}

/// The outcome of one parse attempt over the bytes so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parse {
    /// More bytes needed.
    Incomplete,
    /// The bytes can never become a request (answer 400 / 413 and close).
    Invalid {
        status: u16,
        detail: String,
    },
    Request(Request),
}

/// Parses one request from the front of `buffer`.
#[must_use]
pub fn parse(buffer: &[u8]) -> Parse {
    let Some(head_end) = find(buffer, b"\r\n\r\n") else {
        return if buffer.len() > HEAD_CAP {
            invalid(431, "request head too large")
        } else {
            Parse::Incomplete
        };
    };
    if head_end > HEAD_CAP {
        return invalid(431, "request head too large");
    }
    let Ok(head) = std::str::from_utf8(&buffer[..head_end]) else {
        return invalid(400, "request head is not UTF-8");
    };
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return invalid(400, "malformed request line");
    };
    if !version.starts_with("HTTP/1.") {
        return invalid(400, "unsupported HTTP version");
    }
    let mut content_length = 0usize;
    let mut bearer = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return invalid(400, "malformed header line");
        };
        if name.trim().eq_ignore_ascii_case("authorization") {
            bearer = bearer_token(value);
        }
        if name.trim().eq_ignore_ascii_case("content-length") {
            match value.trim().parse::<usize>() {
                Ok(length) if length <= BODY_CAP => content_length = length,
                Ok(_) => return invalid(413, "request body too large"),
                Err(_) => return invalid(400, "malformed content-length"),
            }
        }
    }
    let body_start = head_end + 4;
    if buffer.len() < body_start + content_length {
        return Parse::Incomplete;
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };
    let query = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (pair.to_owned(), String::new()),
        })
        .collect();
    Parse::Request(Request {
        method: method.to_owned(),
        path: path.to_owned(),
        query,
        body: buffer[body_start..body_start + content_length].to_vec(),
        consumed: body_start + content_length,
        bearer,
    })
}

/// The token of an `Authorization` header VALUE when its scheme is
/// `Bearer` (case-insensitive) and a non-empty token follows; any other
/// value presents nothing.
fn bearer_token(value: &str) -> Option<String> {
    let value = value.trim();
    let (scheme, token) = value.split_once(char::is_whitespace)?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

fn invalid(status: u16, detail: &str) -> Parse {
    Parse::Invalid {
        status,
        detail: detail.to_owned(),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The reason phrase of the status codes this transport emits.
#[must_use]
pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        431 => "Request Header Fields Too Large",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// Frames one JSON response; the connection closes after it. A 401
/// carries the `WWW-Authenticate: Bearer` challenge RFC 7235 requires of
/// it — the framer's business, because a 401 without one is not HTTP.
#[must_use]
pub fn response(status: u16, body: &[u8]) -> Vec<u8> {
    framed(status, "application/json", None, body)
}

/// Frames one response of any `Content-Type`, with an optional
/// `Cache-Control` — the static-bundle rows (UI-1): the document and its
/// assets carry their own MIME and their cache class (`jinn_ui`'s serving
/// law), a JSON answer carries neither. Every response still closes.
#[must_use]
pub fn framed(
    status: u16,
    content_type: &str,
    cache_control: Option<&str>,
    body: &[u8],
) -> Vec<u8> {
    let challenge = if status == 401 {
        "WWW-Authenticate: Bearer\r\n"
    } else {
        ""
    };
    let cache = cache_control.map_or(String::new(), |value| format!("Cache-Control: {value}\r\n"));
    let mut wire = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{cache}{challenge}Connection: close\r\n\r\n",
        reason(status),
        body.len()
    )
    .into_bytes();
    wire.extend_from_slice(body);
    wire
}

/// A typed error as a response: the versioned envelope
/// `{"api-version": …, "error": {...}}` under the error's status.
#[must_use]
pub fn error_response(error: &ApiError) -> Vec<u8> {
    error_answer_response(&Answer::error(error.clone()))
}

/// A decoded error answer as a response, the envelope verbatim — its
/// version and every unknown sibling ride through to the operator. An
/// `ok` outcome is not an error and is answered 200 with its value.
#[must_use]
pub fn error_answer_response(answer: &Answer) -> Vec<u8> {
    match &answer.outcome {
        Outcome::Error(error) => response(status_for(error.code), &answer.encode()),
        Outcome::Ok(value) => response(200, &serde_json::to_vec(value).expect("encodes")),
    }
}

/// The status code of each typed error class.
#[must_use]
pub fn status_for(code: ErrorCode) -> u16 {
    match code {
        ErrorCode::NotFound => 404,
        ErrorCode::Invalid => 422,
        ErrorCode::Unavailable => 503,
        ErrorCode::Refused => 502,
        ErrorCode::Unauthenticated => 401,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_get_parses_with_its_query() {
        let wire = b"GET /v1/ledger/tail?after=12&limit=5&x=y HTTP/1.1\r\nHost: h\r\n\r\n";
        let Parse::Request(request) = parse(wire) else {
            panic!("a complete request")
        };
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("GET", "/v1/ledger/tail")
        );
        assert_eq!(
            request.query_json(),
            serde_json::json!({ "after": 12, "limit": 5, "x": "y" })
        );
        assert!(request.body.is_empty());
        assert_eq!(request.consumed, wire.len());
    }

    #[test]
    fn a_body_arriving_in_pieces_is_incomplete_until_whole() {
        let head = b"PATCH /v1/profile/entries/a HTTP/1.1\r\nContent-Length: 9\r\n\r\n";
        assert_eq!(parse(&head[..10]), Parse::Incomplete);
        assert_eq!(
            parse(head),
            Parse::Incomplete,
            "head complete, body pending"
        );
        let mut whole = head.to_vec();
        whole.extend_from_slice(b"{\"a\":1}XXtrailing");
        let Parse::Request(request) = parse(&whole) else {
            panic!("complete")
        };
        assert_eq!(request.body, b"{\"a\":1}XX");
        assert_eq!(request.consumed, head.len() + 9);
    }

    #[test]
    fn bounds_and_malformed_heads_are_typed_refusals() {
        let huge = vec![b'a'; HEAD_CAP + 1];
        assert!(matches!(parse(&huge), Parse::Invalid { status: 431, .. }));
        let big_body = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            BODY_CAP + 1
        );
        assert!(matches!(
            parse(big_body.as_bytes()),
            Parse::Invalid { status: 413, .. }
        ));
        assert!(matches!(
            parse(b"nonsense\r\n\r\n"),
            Parse::Invalid { status: 400, .. }
        ));
        assert!(matches!(
            parse(b"GET / HTTP/1.1\r\nbroken header\r\n\r\n"),
            Parse::Invalid { status: 400, .. }
        ));
        assert!(matches!(
            parse(b"GET / HTTP/1.1\r\nContent-Length: x\r\n\r\n"),
            Parse::Invalid { status: 400, .. }
        ));
    }

    /// A static row carries its own MIME and cache class and nothing of
    /// the JSON envelope; a JSON answer carries no cache header at all.
    #[test]
    fn a_static_response_carries_its_mime_and_cache_class_and_closes() {
        let wire = String::from_utf8(framed(
            200,
            "text/html; charset=utf-8",
            Some("no-cache"),
            b"<html>",
        ))
        .expect("ascii");
        assert!(wire.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(wire.contains("Content-Type: text/html; charset=utf-8\r\n"));
        assert!(wire.contains("Cache-Control: no-cache\r\n"));
        assert!(wire.contains("Content-Length: 6\r\n"));
        assert!(wire.contains("Connection: close\r\n"));
        assert!(wire.ends_with("\r\n\r\n<html>"));
        let json = String::from_utf8(response(200, b"{}")).expect("ascii");
        assert!(!json.contains("Cache-Control"));
        let missing = String::from_utf8(framed(
            404,
            "text/plain; charset=utf-8",
            None,
            b"no such asset",
        ))
        .expect("ascii");
        assert!(missing.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(!missing.contains("Cache-Control"));
    }

    #[test]
    fn responses_are_framed_and_close() {
        let wire = String::from_utf8(response(200, b"{}")).expect("ascii");
        assert!(wire.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(wire.contains("Content-Length: 2\r\n"));
        assert!(wire.contains("Connection: close\r\n"));
        assert!(wire.ends_with("\r\n\r\n{}"));
        let error =
            String::from_utf8(error_response(&ApiError::unavailable(20, "none"))).expect("ascii");
        assert!(error.starts_with("HTTP/1.1 503 Service Unavailable\r\n"));
        let body: serde_json::Value =
            serde_json::from_str(error.split("\r\n\r\n").nth(1).expect("body")).expect("json body");
        assert_eq!(
            body,
            serde_json::json!({ "api-version": jinn_api::API_VERSION,
                                "error": { "code": "unavailable", "detail": "none", "finding": 20 } }),
            "an error answer is versioned like every other answer"
        );
        assert_eq!(status_for(ErrorCode::NotFound), 404);
        assert_eq!(status_for(ErrorCode::Invalid), 422);
        assert_eq!(status_for(ErrorCode::Refused), 502);
    }

    /// The whole chain an operator sees for an engine refusal: the
    /// engines seam's own code, this seam's code, the status line. The
    /// environment gate (`unavailable`) must arrive as its own status —
    /// the composition suite gates on it and cannot read a flattened 502.
    #[test]
    fn an_engine_refusal_reaches_the_operator_under_its_own_status() {
        use jinn_api::engine_api_error;
        use jinn_engine::{EngineError, ErrorCode as EngineErrorCode};

        let chain = |code: EngineErrorCode| {
            let mapped = engine_api_error(&EngineError::new(code, "why"));
            let wire = String::from_utf8(error_response(&mapped)).expect("ascii");
            let status: u16 = wire
                .split(' ')
                .nth(1)
                .and_then(|status| status.parse().ok())
                .expect("a status line");
            let body: serde_json::Value =
                serde_json::from_str(wire.split("\r\n\r\n").nth(1).expect("body"))
                    .expect("json body");
            (status, body)
        };
        for (code, status, name) in [
            (EngineErrorCode::Invalid, 422, "invalid"),
            (EngineErrorCode::NotFound, 404, "not-found"),
            (EngineErrorCode::Refused, 502, "refused"),
            (EngineErrorCode::Unavailable, 503, "unavailable"),
            (EngineErrorCode::Failed, 502, "failed"),
        ] {
            let (seen, body) = chain(code);
            assert_eq!(seen, status, "{name}");
            assert_eq!(body["error"]["engine-code"], name, "{name} stays itself");
            assert_eq!(body["api-version"], jinn_api::API_VERSION);
        }
        let (unavailable, _) = chain(EngineErrorCode::Unavailable);
        let (failed, _) = chain(EngineErrorCode::Failed);
        assert_ne!(
            unavailable, failed,
            "the environment gate is not flattened into a generic failure"
        );
    }
}

#[cfg(test)]
mod door_tests {
    use super::*;

    /// The presented credential is the `Authorization: Bearer` token —
    /// scheme case-insensitive, value trimmed — and NOTHING else on the
    /// request is one: another scheme, a query parameter, a cookie all
    /// present nothing (and `verify` is then asked about nothing).
    #[test]
    fn the_bearer_token_is_the_presented_credential_and_nothing_else_is() {
        let with = |head: &str| {
            let wire = format!("GET /v1/status HTTP/1.1\r\nHost: h\r\n{head}\r\n\r\n");
            let Parse::Request(request) = parse(wire.as_bytes()) else {
                panic!("a complete request")
            };
            request.bearer
        };
        assert_eq!(
            with("Authorization: Bearer abc.def-123").as_deref(),
            Some("abc.def-123")
        );
        assert_eq!(
            with("authorization:   bearer   spaced   ").as_deref(),
            Some("spaced"),
            "the scheme is case-insensitive and the value is trimmed"
        );
        assert_eq!(with("Authorization: Basic dXNlcjpwdw=="), None);
        assert_eq!(
            with("Authorization: Bearer"),
            None,
            "an empty token is nothing"
        );
        assert_eq!(with("X-Token: abc"), None);
        assert_eq!(with("Cookie: token=abc"), None);
        let Parse::Request(query) = parse(b"GET /v1/status?token=abc HTTP/1.1\r\nHost: h\r\n\r\n")
        else {
            panic!("complete")
        };
        assert_eq!(query.bearer, None, "a query parameter is not a credential");
    }

    /// The refusal is its own status, and a 401 carries the challenge
    /// RFC 7235 requires — parsed from the head, not searched for.
    #[test]
    fn a_refusal_is_401_with_a_bearer_challenge_and_its_own_code() {
        assert_eq!(status_for(ErrorCode::Unauthenticated), 401);
        assert_eq!(reason(401), "Unauthorized");
        let wire = String::from_utf8(error_response(&ApiError::unauthenticated(
            "presented credential does not match",
        )))
        .expect("ascii");
        let (head, body) = wire.split_once("\r\n\r\n").expect("framed");
        let mut lines = head.split("\r\n");
        assert_eq!(lines.next(), Some("HTTP/1.1 401 Unauthorized"));
        let headers: Vec<(String, String)> = lines
            .map(|line| {
                let (name, value) = line.split_once(':').expect("a header line");
                (name.trim().to_ascii_lowercase(), value.trim().to_owned())
            })
            .collect();
        assert!(
            headers.contains(&("www-authenticate".into(), "Bearer".into())),
            "{headers:?}"
        );
        assert!(
            headers.contains(&("connection".into(), "close".into())),
            "{headers:?}"
        );
        let body: serde_json::Value = serde_json::from_str(body).expect("json body");
        assert_eq!(
            body,
            serde_json::json!({ "api-version": jinn_api::API_VERSION,
                                "error": { "code": "unauthenticated",
                                           "detail": "presented credential does not match" } })
        );
        // Every other status carries no challenge: the header means
        // exactly one thing.
        let ok = String::from_utf8(response(200, b"{}")).expect("ascii");
        assert!(!ok.to_ascii_lowercase().contains("www-authenticate"));
        let refused = String::from_utf8(error_response(&ApiError::new(
            ErrorCode::Refused,
            "off the allowlist",
        )))
        .expect("ascii");
        assert!(refused.starts_with("HTTP/1.1 502 "));
        assert!(!refused.to_ascii_lowercase().contains("www-authenticate"));
    }
}

#[cfg(test)]
mod query_tests {
    use super::query_json;

    fn pairs(input: &[(&str, &str)]) -> Vec<(String, String)> {
        input
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn a_query_carries_the_seams_scalar_kinds_and_guesses_at_nothing() {
        let json = query_json(&pairs(&[
            ("limit", "10"),
            ("roots-only", "true"),
            ("archived", "false"),
            ("department", "platform"),
            ("nearly", "True"),
            ("numeric", "1"),
        ]));
        assert_eq!(json["limit"], 10);
        assert_eq!(json["roots-only"], true);
        assert_eq!(json["archived"], false);
        assert_eq!(json["department"], "platform");
        // A codec that guessed would make a typo mean `true`, and `1`
        // mean it too. Both stay what they are, and the seam that reads
        // them refuses them.
        assert_eq!(json["nearly"], "True");
        assert_eq!(json["numeric"], 1);
    }
}
