//! What the host and its guests say to each other over the WebSocket.
//!
//! The guest's first message is its token as text, `code.ticket`. Then binary messages whose
//! first byte is the kind:
//! - `DOC` + a Yjs (v1) update, both ways;
//! - `PRESENCE` + JSON: from a guest its own state; from the host `{"id": n, "state": {…}}`
//!   (state null when that person left);
//! - `HELLO` + JSON `{"client": n, "access": "edit" | "view"}`, from the host after the token.
//!
//! Close codes: 4001 sharing ended, 4003 access removed (or new link), 4100 reconnect
//! (permission changed).

pub const DOC: u8 = 0;
pub const PRESENCE: u8 = 1;
pub const HELLO: u8 = 2;

pub const CLOSE_ENDED: u16 = 4001;
pub const CLOSE_REMOVED: u16 = 4003;
pub const CLOSE_RECONNECT: u16 = 4100;

pub fn frame(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 1);
    v.push(kind);
    v.extend_from_slice(body);
    v
}

/// A share code (after `#` in the link): 16 to 64 url-safe characters.
pub fn valid_code(code: &str) -> bool {
    (16..=64).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Presence updates are sent at most this often (ms); slower with many people, to spare the
/// host's upload (it relays every update to everyone).
pub fn presence_interval(people: usize) -> f64 {
    if people > 10 { 50.0 * people as f64 / 10.0 } else { 50.0 }
}
