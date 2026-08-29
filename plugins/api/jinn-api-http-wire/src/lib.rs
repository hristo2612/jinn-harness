//! Minimal HTTP/1.1 for the operator API's loopback provider: parse one
//! request out of a growing byte buffer (headers + `Content-Length` body,
//! bounded), frame one JSON response, map the seam's typed error codes to
//! status codes. No chunked encoding, no keep-alive (every response
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

/// Query pairs as a JSON object: integers become numbers, everything else
/// stays a string (the seam's numeric fields are `u64`/`u32`).
#[must_use]
pub fn query_json(pairs: &[(String, String)]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (key, value) in pairs {
        let json = value
            .parse::<u64>()
            .map_or_else(|_| serde_json::Value::String(value.clone()), Into::into);
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
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return invalid(400, "malformed header line");
        };
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
    })
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

/// Frames one JSON response; the connection closes after it.
#[must_use]
pub fn response(status: u16, body: &[u8]) -> Vec<u8> {
    let mut wire = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
