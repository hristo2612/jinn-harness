//! A bare HTTP/1.1 client for the operator API: one request per
//! connection (the provider closes after every response), the body read
//! to EOF. Deliberately as small as the provider's own transport — the
//! suite drives the API exactly as an operator's `curl` would.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// How long one request may take end to end. Sized for a LOADED machine,
/// not for an idle one: the suite runs every profile's roots in parallel
/// — the engines profile alone is eleven wasm components per daemon,
/// eight daemons at once, against a debug-built runtime — and a request
/// whose answer is merely slow is not a broken API. A genuinely stuck
/// provider still fails this bound; it just fails it later.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);

/// One response: status code, the JSON body (`Null` if not JSON), the
/// headers as parsed from the head (names lower-cased, values trimmed),
/// and the raw text.
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: serde_json::Value,
    pub headers: Vec<(String, String)>,
    pub raw: String,
}

impl Response {
    /// The first header named `name` (case-insensitive), if any.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(found, _)| *found == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Connects to the loopback port, retrying until the listener is up or
/// `deadline` passes.
///
/// # Errors
///
/// No listener within the deadline (the last connect error).
pub fn connect(port: u16, deadline: Duration) -> Result<TcpStream, std::io::Error> {
    let until = Instant::now() + deadline;
    loop {
        match TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(500))
        {
            Ok(stream) => return Ok(stream),
            Err(error) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(50));
                let _ = error;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Whether anything answers on the port right now (one attempt).
#[must_use]
pub fn listening(port: u16) -> bool {
    TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)).is_ok()
}

/// Performs one request AS THE OPERATOR — presenting the credential every
/// daemon this suite boots is provisioned with
/// ([`crate::kit::suite_credential`]) as a bearer token; panics on
/// transport failure (the listener is expected up — use [`listening`] to
/// assert the opposite).
pub fn request(port: u16, method: &str, target: &str, body: Option<&str>) -> Response {
    request_as(
        port,
        method,
        target,
        body,
        Some(crate::kit::suite_credential()),
    )
}

/// Performs one request presenting `credential` as a bearer token, or
/// presenting NOTHING when `None` — the door's proofs drive both.
pub fn request_as(
    port: u16,
    method: &str,
    target: &str,
    body: Option<&str>,
    credential: Option<&str>,
) -> Response {
    let mut stream = connect(port, REQUEST_TIMEOUT)
        .unwrap_or_else(|error| panic!("connect 127.0.0.1:{port}: {error}"));
    stream
        .set_read_timeout(Some(REQUEST_TIMEOUT))
        .expect("read timeout");
    let body = body.unwrap_or_default();
    let authorization = credential.map_or(String::new(), |credential| {
        format!("Authorization: Bearer {credential}\r\n")
    });
    let wire = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n{authorization}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(wire.as_bytes()).expect("request written");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("response read to EOF");
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let status = raw
        .strip_prefix("HTTP/1.1 ")
        .and_then(|rest| rest.get(..3))
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status line in:\n{raw}"));
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let headers = head
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let body = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    Response {
        status,
        body,
        headers,
        raw,
    }
}

/// One raw response: status, headers (names lower-cased) and the body
/// BYTES — for the static bundle's assets, whose bytes are hashed and
/// must not pass through a lossy string. Presents `credential` as a
/// bearer when given, nothing otherwise.
pub fn fetch_bytes(
    port: u16,
    target: &str,
    credential: Option<&str>,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = connect(port, REQUEST_TIMEOUT)
        .unwrap_or_else(|error| panic!("connect 127.0.0.1:{port}: {error}"));
    stream
        .set_read_timeout(Some(REQUEST_TIMEOUT))
        .expect("read timeout");
    let authorization = credential.map_or(String::new(), |credential| {
        format!("Authorization: Bearer {credential}\r\n")
    });
    let wire = format!(
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n{authorization}Connection: close\r\n\r\n"
    );
    stream.write_all(wire.as_bytes()).expect("request written");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("response read to EOF");
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a head");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let body = raw[split + 4..].to_vec();
    let status = head
        .strip_prefix("HTTP/1.1 ")
        .and_then(|rest| rest.get(..3))
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status line in:\n{head}"));
    let headers = head
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    (status, headers, body)
}

/// `GET target`.
pub fn get(port: u16, target: &str) -> Response {
    request(port, "GET", target, None)
}

/// `PATCH target` with a JSON body.
pub fn patch(port: u16, target: &str, body: &serde_json::Value) -> Response {
    request(port, "PATCH", target, Some(&body.to_string()))
}

/// `POST target` with a JSON body — starting an engine run.
pub fn post(port: u16, target: &str, body: &serde_json::Value) -> Response {
    request(port, "POST", target, Some(&body.to_string()))
}

/// `DELETE target` — cancelling an engine run.
pub fn delete(port: u16, target: &str) -> Response {
    request(port, "DELETE", target, None)
}
