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

/// Light or dark as a Linux desktop prefers (GNOME, KDE… tell it through their portal, which
/// winit doesn't read): 0 not known, 1 dark, 2 light.
#[cfg(target_os = "linux")]
static SCHEME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

#[cfg(target_os = "linux")]
fn scheme(v: &zbus::zvariant::Value) -> u8 {
    match v {
        zbus::zvariant::Value::U32(n @ 1..=2) => *n as u8,
        // The older `Read` wraps it once more.
        zbus::zvariant::Value::Value(v) => scheme(v),
        _ => 0,
    }
}

/// Reads the desktop's colour scheme and follows it as it changes, waking `ctx` each time.
#[cfg(target_os = "linux")]
pub fn watch_color_scheme(ctx: egui::Context) {
    use zbus::zvariant::{OwnedValue, Value};
    let set = move |v: &Value| {
        SCHEME.store(scheme(v), std::sync::atomic::Ordering::Relaxed);
        ctx.request_repaint();
    };
    std::thread::spawn(move || -> zbus::Result<()> {
        let conn = zbus::blocking::Connection::session()?;
        let portal = zbus::blocking::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings")?;
        let changes = portal.receive_signal("SettingChanged")?;
        let key = ("org.freedesktop.appearance", "color-scheme");
        let now: OwnedValue = portal.call("ReadOne", &key).or_else(|_| portal.call("Read", &key))?;
        set(&now);
        for m in changes {
            if let Ok((ns, name, v)) = m.body().deserialize::<(String, String, OwnedValue)>()
                && (ns.as_str(), name.as_str()) == key
            {
                set(&v);
            }
        }
        Ok(())
    });
}

/// Whether the system asks for the dark theme.
pub fn system_dark(ctx: &egui::Context) -> bool {
    #[cfg(target_os = "linux")]
    match SCHEME.load(std::sync::atomic::Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }
    ctx.input(|i| i.raw.system_theme) == Some(egui::Theme::Dark)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use zbus::zvariant::Value;

    #[test]
    fn the_desktop_scheme_is_read_plain_or_wrapped() {
        assert_eq!(super::scheme(&Value::U32(1)), 1);
        assert_eq!(super::scheme(&Value::Value(Box::new(Value::U32(2)))), 2);
        // No preference, or something unexpected: not known.
        assert_eq!(super::scheme(&Value::U32(0)), 0);
        assert_eq!(super::scheme(&Value::from("dark")), 0);
    }
}
