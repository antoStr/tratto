//! What the host and its guests say to each other over the WebSocket.
//!
//! The guest's first message is its token as text, `code.ticket`. Then binary messages whose
//! first byte is the kind:
//! - `DOC` + a Yjs (v1) update, both ways;
//! - `PRESENCE` + JSON: from a guest its own state; from the host `{"id": n, "state": {…}}`
//!   (state null when that person left);
//! - `HELLO` + JSON `{"client": n, "access": "edit" | "view", "now": ms}`, from the host after
//!   the token (`now`: the host's clock, which shared moments like the timer's end refer to).
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

/// Presence updates are sent at most this often (ms), about 30 a second; slower with many
/// people, to spare the host's upload (it relays every update to everyone).
pub fn presence_interval(people: usize) -> f64 {
    if people > 10 { 33.0 * people as f64 / 10.0 } else { 33.0 }
}

/// A guest's presence made harmless before it is passed on and drawn: numbers within ±1e9,
/// texts of at most 200 characters, lists of at most 9 000 values, four levels deep at most.
pub fn tame(v: serde_json::Value, depth: usize) -> serde_json::Value {
    use serde_json::Value;
    match v {
        _ if depth > 4 => Value::Null,
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()).map_or(Value::Null, |f| if f.abs() <= 1e9 { Value::Number(n) } else { f.clamp(-1e9, 1e9).into() }),
        Value::String(s) => Value::String(s.chars().take(200).collect()),
        Value::Array(a) => Value::Array(a.into_iter().take(9000).map(|x| tame(x, depth + 1)).collect()),
        Value::Object(m) => Value::Object(m.into_iter().filter(|(k, _)| k.len() <= 32).take(32).map(|(k, x)| (k, tame(x, depth + 1))).collect()),
        other => other,
    }
}

/// Points of a stroke being drawn kept for the others: enough for a long sentence.
const LIVE_KEEP: usize = 3000;

/// A person's state as just received, merged with what was known of them: the stroke they are
/// drawing comes in pieces (`live.from` is how many points were sent before), and the pieces
/// are put back together here. The result holds the whole stroke (its last points if very
/// long) with `live.n`, the number of points sent so far.
pub fn merge_presence(prev: Option<&serde_json::Value>, mut next: serde_json::Value) -> serde_json::Value {
    let live = &next["live"];
    let (Some(sid), Some(from)) = (live["sid"].as_u64(), live["from"].as_u64()) else { return next };
    let new = live["pts"].as_array().cloned().unwrap_or_default();
    let added = (new.len() / 3) as u64;
    let mut pts = match prev.map(|p| &p["live"]) {
        Some(l) if from > 0 && l["sid"].as_u64() == Some(sid) && l["n"].as_u64() == Some(from) => l["pts"].as_array().cloned().unwrap_or_default(),
        _ => Vec::new(),
    };
    pts.extend(new);
    let extra = pts.len().saturating_sub(LIVE_KEEP * 3) / 3 * 3;
    pts.drain(..extra);
    let l = &mut next["live"];
    l["pts"] = serde_json::Value::Array(pts);
    l["n"] = (from + added).into();
    if let Some(o) = l.as_object_mut() {
        o.remove("from");
    }
    next
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn a_guests_presence_is_tamed_before_it_goes_on() {
        let wild = json!({ "cursor": { "x": 1e300, "y": -5 }, "user": { "name": "x".repeat(5000) }, "a": { "b": { "c": { "d": { "e": { "f": 1 } } } } }, "live": { "pts": vec![1; 20_000] } });
        let t = super::tame(wild, 0);
        assert_eq!(t["cursor"]["x"], json!(1e9));
        assert_eq!(t["cursor"]["y"], json!(-5));
        assert_eq!(t["user"]["name"].as_str().unwrap().len(), 200);
        assert_eq!(t["live"]["pts"].as_array().unwrap().len(), 9000);
        assert!(t["a"]["b"]["c"]["d"]["e"].is_null(), "deep nesting cut");
    }

    #[test]
    fn a_stroke_sent_in_pieces_is_whole_again() {
        let a = super::merge_presence(None, json!({ "live": { "sid": 7, "from": 0, "pts": [0, 0, 1, 1, 1, 1] } }));
        let b = super::merge_presence(Some(&a), json!({ "live": { "sid": 7, "from": 2, "pts": [2, 2, 1] } }));
        assert_eq!(b["live"]["pts"], json!([0, 0, 1, 1, 1, 1, 2, 2, 1]));
        assert_eq!(b["live"]["n"], 3);
        // A new stroke starts afresh; a piece that doesn't follow is shown alone.
        let c = super::merge_presence(Some(&b), json!({ "live": { "sid": 8, "from": 0, "pts": [5, 5, 1] } }));
        assert_eq!(c["live"]["pts"], json!([5, 5, 1]));
        let d = super::merge_presence(Some(&c), json!({ "live": { "sid": 8, "from": 4, "pts": [6, 6, 1] } }));
        assert_eq!(d["live"]["pts"], json!([6, 6, 1]));
        assert_eq!(super::merge_presence(Some(&d), json!({ "live": null }))["live"], json!(null));
    }
}
