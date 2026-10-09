//! What differs between the desktop app and the guest page in the browser.

/// Random bytes. Security-sensitive ids (share codes, tickets) are only made on the desktop,
/// which uses the operating system's generator.
#[cfg(not(target_arch = "wasm32"))]
pub fn random(buf: &mut [u8]) {
    getrandom::fill(buf).expect("system random generator");
}

#[cfg(target_arch = "wasm32")]
pub fn random(buf: &mut [u8]) {
    for b in buf {
        *b = (js_sys::Math::random() * 256.0) as u8;
    }
}

/// Milliseconds since 1970.
#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    js_sys::Date::now()
}
