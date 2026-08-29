//! A bare HTTP/1.1 client for the operator API: one request per
//! connection (the provider closes after every response), the body read
//! to EOF. Deliberately as small as the provider's own transport — the
//! suite drives the API exactly as an operator's `curl` would.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// How long one request may take end to end (the provider polls its
/// listener at the kit's 250 ms cadence; a patch reconciles in between).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// One response: status code and the JSON body (`Null` if not JSON).
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: serde_json::Value,
    pub raw: String,
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

/// Performs one request; panics on transport failure (the listener is
/// expected up — use [`listening`] to assert the opposite).
pub fn request(port: u16, method: &str, target: &str, body: Option<&str>) -> Response {
    let mut stream = connect(port, REQUEST_TIMEOUT)
        .unwrap_or_else(|error| panic!("connect 127.0.0.1:{port}: {error}"));
    stream
        .set_read_timeout(Some(REQUEST_TIMEOUT))
        .expect("read timeout");
    let body = body.unwrap_or_default();
    let wire = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    let body = raw
        .split_once("\r\n\r\n")
        .and_then(|(_, body)| serde_json::from_str(body).ok())
        .unwrap_or(serde_json::Value::Null);
    Response { status, body, raw }
}

/// `GET target`.
pub fn get(port: u16, target: &str) -> Response {
    request(port, "GET", target, None)
}

/// `PATCH target` with a JSON body.
pub fn patch(port: u16, target: &str, body: &serde_json::Value) -> Response {
    request(port, "PATCH", target, Some(&body.to_string()))
}
