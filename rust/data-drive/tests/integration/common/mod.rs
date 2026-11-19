use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::sync::atomic::{AtomicU16, Ordering};

/// Binds a loopback listener on a port outside the ephemeral ranges (Linux 32768–60999,
/// Windows and macOS 49152–65535).
///
/// Tests that drop a listener and later bind its address again need it: the system never
/// hands such a port to the `bind("127.0.0.1:0")` or the outgoing connections of the tests
/// running in parallel in this binary, so the port stays free in between. Each call tries a
/// different port, so two of these tests never share one either.
pub fn listener_outside_ephemeral_range() -> TcpListener {
    const FIRST_PORT: u16 = 20_000;
    const PORT_COUNT: u16 = 12_000;
    static NEXT_OFFSET: AtomicU16 = AtomicU16::new(0);
    // Separate test processes running at the same time start at different ports.
    let process_offset = u16::try_from(std::process::id() % u32::from(PORT_COUNT)).unwrap();
    for _ in 0..PORT_COUNT {
        let offset = NEXT_OFFSET.fetch_add(1, Ordering::Relaxed) % PORT_COUNT;
        let port = FIRST_PORT + (process_offset + offset) % PORT_COUNT;
        if let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            return listener;
        }
    }
    panic!(
        "no free loopback port from {FIRST_PORT} to {}",
        FIRST_PORT + PORT_COUNT - 1
    );
}

pub fn read_request(stream: &mut impl Read) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    let end = loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0, "unexpected EOF while reading request headers");
        bytes.extend_from_slice(&buf[..n]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let length = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|s| s.parse::<usize>().ok())
        })
        .unwrap_or(0);
    while bytes.len() < end + length {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0, "unexpected EOF while reading request body");
        bytes.extend_from_slice(&buf[..n]);
    }
    (headers, bytes[end..end + length].to_vec())
}

pub fn reply_json(stream: &mut impl Write, status: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

pub fn reply_with_headers(stream: &mut impl Write, status: &str, headers: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

pub fn reply(stream: &mut impl Write, status: &str, body: &str) {
    reply_json(stream, status, body);
}

pub fn reply_extra(stream: &mut impl Write, status: &str, extra: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}
