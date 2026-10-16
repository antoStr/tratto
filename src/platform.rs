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

/// How far the host's clock is ahead of this one (ms): 0 on the host, measured by each guest
/// when it connects. Shared moments (a timer's end, when the music started) use this clock.
static CLOCK_OFFSET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_clock_offset(ms: f64) {
    CLOCK_OFFSET.store(ms.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Now on the host's clock, the one every guest agrees on.
pub fn shared_now() -> f64 {
    now_ms() + f64::from_bits(CLOCK_OFFSET.load(std::sync::atomic::Ordering::Relaxed))
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
