//! When Tratto doesn't open on someone's computer: a short log of each start next to the boards
//! (startup.log, to send along with the report) and the way of drawing the window that worked
//! last, so the next start goes straight to it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);
/// When this start began (ms since 1970), for the times in the log.
static T0: AtomicU64 = AtomicU64::new(0);
/// The window showed its first frame.
static SHOWN: AtomicBool = AtomicBool::new(false);
static FRAMES: AtomicU64 = AtomicU64::new(0);
/// The way of drawing in use, and whether to remember it once the window shows.
static GFX: Mutex<Option<(String, bool)>> = Mutex::new(None);

/// Never longer than this: the end of the previous starts is kept, the oldest lines dropped.
const MAX_LOG: u64 = 48 * 1024;

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join("startup.log")
}

/// Starts the log of this run in `dir`, keeping the end of the previous ones.
pub fn init(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let path = log_path(dir);
    if let Ok(meta) = std::fs::metadata(&path)
        && meta.len() > MAX_LOG
        && let Ok(old) = std::fs::read(&path)
    {
        let keep = &old[old.len() - (MAX_LOG as usize / 2)..];
        let from = keep.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
        let _ = std::fs::write(&path, &keep[from..]);
    }
    T0.store(crate::platform::now_ms() as u64, Ordering::Relaxed);
    *LOG.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
    let now = crate::platform::now_ms();
    write_line(&format!("\n=== {} · Tratto {} · {} ===", utc(now), env!("CARGO_PKG_VERSION"), system()));
}

/// One line of the log, with the seconds since the start.
pub fn log(msg: impl AsRef<str>) {
    let t = (crate::platform::now_ms() - T0.load(Ordering::Relaxed) as f64).max(0.0) / 1000.0;
    write_line(&format!("[{t:7.3}] {}", msg.as_ref()));
}

fn write_line(line: &str) {
    let Some(path) = LOG.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn set_gfx(name: &str, remember: bool) {
    *GFX.lock().unwrap_or_else(|e| e.into_inner()) = Some((name.to_string(), remember));
}

/// Called at every frame of the app: the second one means the first was drawn, so the window
/// is on screen, and this way of drawing works here.
pub fn frame(ctx: &egui::Context) {
    if SHOWN.load(Ordering::Relaxed) {
        return;
    }
    if FRAMES.fetch_add(1, Ordering::Relaxed) == 0 {
        log("Primo fotogramma preparato");
        // The next frame is the proof the first one reached the screen.
        ctx.request_repaint();
        return;
    }
    SHOWN.store(true, Ordering::Relaxed);
    log("Finestra aperta");
    let gfx = GFX.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let dir = LOG.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|p| p.parent().map(Path::to_path_buf));
    if let (Some((name, true)), Some(dir)) = (gfx, dir)
        && remembered_gfx(&dir).as_deref() != Some(name.as_str())
    {
        remember_gfx(&dir, &name);
    }
}

pub fn shown() -> bool {
    SHOWN.load(Ordering::Relaxed)
}

/// The way of drawing that last opened the window, if this version opened it.
pub fn remembered_gfx(dir: &Path) -> Option<String> {
    let s = std::fs::read_to_string(dir.join("graphics.txt")).ok()?;
    let (version, name) = s.trim().split_once(' ')?;
    (version == env!("CARGO_PKG_VERSION")).then(|| name.to_string())
}

pub fn remember_gfx(dir: &Path, name: &str) {
    let _ = std::fs::write(dir.join("graphics.txt"), format!("{} {name}\n", env!("CARGO_PKG_VERSION")));
}

/// The time as year-month-day hours:minutes:seconds, UTC.
fn utc(ms: f64) -> String {
    let secs = (ms / 1000.0) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970 to a date (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rest / 3600, rest / 60 % 60, rest % 60)
}

/// Which system this is, for the log.
#[cfg(windows)]
fn system() -> String {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let read = |name: &str| -> Option<String> {
        let key: Vec<u16> = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\0".encode_utf16().collect();
        let value: Vec<u16> = format!("{name}\0").encode_utf16().collect();
        let mut buf = [0u16; 128];
        let mut len = std::mem::size_of_val(&buf) as u32;
        // SAFETY: plain registry read into a buffer whose size in bytes is passed along.
        let r = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) };
        (r == 0).then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
    };
    let product = read("ProductName").unwrap_or_else(|| "Windows".into());
    let build = read("CurrentBuild").unwrap_or_default();
    // Windows 11 still calls itself Windows 10 in the registry: the build tells them apart.
    let product = if build.parse::<u32>().is_ok_and(|b| b >= 22_000) { product.replace("Windows 10", "Windows 11") } else { product };
    format!("{product} {} (build {build}), {}", read("DisplayVersion").unwrap_or_default(), std::env::consts::ARCH)
}

#[cfg(not(windows))]
fn system() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates_come_out_right() {
        assert_eq!(super::utc(0.0), "1970-01-01 00:00:00 UTC");
        assert_eq!(super::utc(1_792_330_985_000.0), "2026-10-18 13:43:05 UTC");
        assert_eq!(super::utc(951_782_400_000.0), "2000-02-29 00:00:00 UTC");
    }
}
