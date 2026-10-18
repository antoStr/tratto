//! Telling a pen from a finger, and its pressure. The window system reports both as touches; on
//! Windows the pointer messages say which is which, and whether the barrel button or the eraser
//! end is in use, so they are read here before winit turns them into touches. In a browser the
//! page's own pointer and touch events say it (Apple Pencil, S Pen, a tablet's pen).
//!
//! Some pens drive the mouse pointer instead: a drawing tablet whose driver has Windows Ink
//! turned off (Wacom, Huion, XP-Pen set up for other drawing programs) sends only mouse moves,
//! with its pressure on the side through Wintab, the tablets' own interface; a pen in a desktop
//! browser comes as a mouse with the pressure on its pointer events. Both are read here too.
//!
//! Also on Windows: the mouse positions the system folds into one move while the app is busy
//! drawing a frame. Writing fast with the mouse, those are most of the curve (a small loop of
//! cursive can fall between two frames), so they are fetched back here.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PenState {
    /// The barrel (side) button is held.
    pub barrel: bool,
    /// The eraser end is being used.
    pub eraser: bool,
    /// Stylus pressure (0.0..=1.0) when reported by the platform pointer API.
    pub pressure: Option<f32>,
    /// Stylus tilt (-1.0 to 1.0, for -90° to 90°) along X and Y.
    pub tilt_x: f32,
    pub tilt_y: f32,
}

/// A pen moving the mouse pointer, as it is now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MousePen {
    /// 0–1.
    pub pressure: f32,
    pub barrel: bool,
    pub eraser: bool,
    /// Stylus tilt (-1.0 to 1.0, for -90° to 90°) along X and Y.
    pub tilt_x: f32,
    pub tilt_y: f32,
}

impl PenState {
    /// Combined tilt magnitude in 0.0..=1.0 (0 = upright, 1 = flat).
    pub fn tilt(self) -> f32 {
        self.tilt_x.hypot(self.tilt_y).clamp(0.0, 1.0)
    }
}

impl MousePen {
    /// Combined tilt magnitude in 0.0..=1.0 (0 = upright, 1 = flat).
    pub fn tilt(self) -> f32 {
        self.tilt_x.hypot(self.tilt_y).clamp(0.0, 1.0)
    }
}

/// The pen behind a touch id, or None for a finger.
#[cfg(windows)]
pub fn state(id: u64) -> Option<PenState> {
    imp::PENS.lock().ok()?.as_ref()?.get(&id).copied()
}

#[cfg(target_arch = "wasm32")]
pub fn state(id: u64) -> Option<PenState> {
    web::state(id)
}

#[cfg(not(any(windows, target_arch = "wasm32")))]
pub fn state(_id: u64) -> Option<PenState> {
    None
}

/// The pen moving the mouse pointer, when one is (in reach of the tablet, or used a moment ago
/// in a browser); None for a real mouse.
#[cfg(windows)]
pub fn mouse_pen() -> Option<MousePen> {
    wintab::current().or_else(imp::mouse_pen)
}

#[cfg(target_arch = "wasm32")]
pub fn mouse_pen() -> Option<MousePen> {
    web::mouse_pen()
}

#[cfg(not(any(windows, target_arch = "wasm32")))]
pub fn mouse_pen() -> Option<MousePen> {
    None
}

/// The pressure a touch's own force doesn't give (a browser or Windows pointer message whose
/// pressure was read before winit turned it into a touch).
#[cfg(windows)]
pub fn touch_pressure() -> Option<f32> {
    imp::touch_pressure().or_else(|| wintab::current().map(|p| p.pressure))
}

#[cfg(target_arch = "wasm32")]
pub fn touch_pressure() -> Option<f32> {
    web::mouse_pen().map(|p| p.pressure)
}

#[cfg(not(any(windows, target_arch = "wasm32")))]
pub fn touch_pressure() -> Option<f32> {
    None
}

/// How the pen reaches Tratto on this computer, for Settings ▸ Penna: None until one is used.
pub fn source() -> Option<&'static str> {
    match SOURCE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Some("Windows Ink"),
        2 => Some("Wintab (driver della tavoletta)"),
        3 => Some("browser"),
        _ => None,
    }
}

/// 0 none yet, 1 Windows Ink, 2 Wintab, 3 a browser's pen events.
static SOURCE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub(crate) fn seen(source: u8) {
    SOURCE.store(source, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(windows)]
pub use wintab::{focus as wintab_focus, open as wintab_open};

/// The mouse positions (window pixels, oldest first) that came before the move to `to`
/// (window pixels) and were folded into it; nothing outside Windows. Older trails, whose moves
/// never reached the app, are dropped.
#[cfg(windows)]
pub fn mouse_trail(to: (f32, f32)) -> Vec<(f32, f32)> {
    let Ok(mut trails) = imp::TRAILS.lock() else { return Vec::new() };
    while let Some(t) = trails.pop_front() {
        if ((t.to.0 as f32 - to.0).abs()) <= 1.5 && ((t.to.1 as f32 - to.1).abs()) <= 1.5 {
            return t.points.into_iter().map(|(x, y)| (x as f32, y as f32)).collect();
        }
    }
    Vec::new()
}

#[cfg(not(windows))]
pub fn mouse_trail(_to: (f32, f32)) -> Vec<(f32, f32)> {
    Vec::new()
}

/// How long (ms) the left button was up before the latest press, as the system timed it; None
/// outside Windows, where the caller falls back on frame times.
#[cfg(windows)]
pub fn button_gap() -> Option<u32> {
    *imp::GAP.lock().ok()?
}

#[cfg(not(windows))]
pub fn button_gap() -> Option<u32> {
    None
}

#[cfg(windows)]
pub use imp::hook;

#[cfg(windows)]
mod imp {
    use std::collections::{HashMap, VecDeque};
    use std::ffi::c_void;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicIsize, Ordering};

    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GMMP_USE_DISPLAY_POINTS, GetMouseMovePointsEx, MOUSEMOVEPOINT};
    use windows_sys::Win32::UI::Input::Pointer::{GetPointerPenInfo, GetPointerType, POINTER_PEN_INFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{CallWindowProcW, GWLP_WNDPROC, MSG, PEN_FLAG_BARREL, PEN_FLAG_ERASER, PEN_FLAG_INVERTED, PT_PEN, SetWindowLongPtrW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE, WNDPROC};

    pub static PENS: Mutex<Option<HashMap<u64, super::PenState>>> = Mutex::new(None);
    static LAST_PEN: Mutex<Option<(super::MousePen, f64)>> = Mutex::new(None);
    static ORIG_WNDPROC: AtomicIsize = AtomicIsize::new(0);

    /// The positions folded into one mouse move, and that move's position (window pixels).
    pub struct Trail {
        pub to: (i32, i32),
        pub points: Vec<(i32, i32)>,
    }

    pub static TRAILS: Mutex<VecDeque<Trail>> = Mutex::new(VecDeque::new());
    /// The last mouse position seen, as the history reports it: time and 16-bit screen x, y.
    static LAST: Mutex<Option<(u32, i32, i32)>> = Mutex::new(None);
    /// When the left button last went up, and how long it stayed up before the latest press.
    static UP: Mutex<Option<u32>> = Mutex::new(None);
    pub static GAP: Mutex<Option<u32>> = Mutex::new(None);

    pub fn mouse_pen() -> Option<super::MousePen> {
        let (pen, at) = (*LAST_PEN.lock().ok()?)?;
        (crate::platform::now_ms() - at <= 250.0).then_some(pen)
    }

    pub fn touch_pressure() -> Option<f32> {
        let (pen, at) = (*LAST_PEN.lock().ok()?)?;
        (crate::platform::now_ms() - at <= 500.0 && pen.pressure > 0.0).then_some(pen.pressure)
    }

    /// Subclasses the window procedure once so `WM_POINTER*` and `WT_PACKET` messages sent
    /// directly to the window (bypassing `PeekMessageW`) still update pen pressure and tilt.
    pub fn subclass(hwnd: isize) {
        if hwnd == 0 || ORIG_WNDPROC.load(Ordering::Relaxed) != 0 {
            return;
        }
        // SAFETY: `hwnd` is the main window handle on the UI thread; we chain to the original WndProc.
        unsafe {
            let prev = SetWindowLongPtrW(hwnd as *mut c_void, GWLP_WNDPROC, wnd_proc as *const () as isize);
            if prev != 0 {
                ORIG_WNDPROC.store(prev, Ordering::Relaxed);
            }
        }
    }

    unsafe extern "system" fn wnd_proc(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize {
        let _ = super::wintab::on_raw(msg, wparam, lparam);
        if matches!(msg, WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP) {
            on_pointer(wparam);
        }
        let prev = ORIG_WNDPROC.load(Ordering::Relaxed);
        if prev != 0 {
            // SAFETY: forwards to the previous window procedure saved by `SetWindowLongPtrW`.
            let proc: WNDPROC = unsafe { std::mem::transmute(prev) };
            unsafe { CallWindowProcW(proc, hwnd, msg, wparam, lparam) }
        } else {
            0
        }
    }

    /// Screen coordinates from the mouse history are 16 bits: back to signed for monitors left
    /// of or above the main one.
    fn signed(v: i32) -> i32 {
        if v > 32767 { v - 65536 } else { v }
    }

    /// On each mouse move, the positions the system folded into it since the previous one.
    fn mouse_move(msg: &MSG) {
        let here = (msg.time, msg.pt.x & 0xFFFF, msg.pt.y & 0xFFFF);
        let Ok(mut last) = LAST.lock() else { return };
        let before = last.replace(here);
        if msg.message == WM_LBUTTONDOWN {
            if let Ok(mut t) = TRAILS.lock() {
                t.clear();
            }
            return;
        }
        let Some((lt, lx, ly)) = before else { return };
        let cur = MOUSEMOVEPOINT { x: here.1, y: here.2, time: here.0, dwExtraInfo: 0 };
        let mut buf = [MOUSEMOVEPOINT { x: 0, y: 0, time: 0, dwExtraInfo: 0 }; 64];
        // SAFETY: plain Win32 query; the buffer holds 64 points and is told so.
        let n = unsafe { GetMouseMovePointsEx(std::mem::size_of::<MOUSEMOVEPOINT>() as u32, &cur, buf.as_mut_ptr(), 64, GMMP_USE_DISPLAY_POINTS) };
        if n <= 1 {
            return;
        }
        // Newest first; the first is the move itself. Stop at the previous move. A mouse polled
        // at 1000 Hz puts several points in the same millisecond: those of the previous move's
        // millisecond are taken only if the previous move itself is found after them.
        let mut points = Vec::new();
        let mut same_ms = Vec::new();
        let mut found = false;
        for p in &buf[1..n as usize] {
            if p.time < lt || points.len() >= 62 {
                break;
            }
            if p.time == lt && p.x == lx && p.y == ly {
                found = true;
                break;
            }
            let mut q = POINT { x: signed(p.x), y: signed(p.y) };
            // SAFETY: the window of the message; q is a valid POINT.
            if unsafe { ScreenToClient(msg.hwnd, &mut q) } == 0 {
                return;
            }
            if p.time == lt { same_ms.push((q.x, q.y)) } else { points.push((q.x, q.y)) }
        }
        if found {
            points.extend(same_ms);
        }
        // Repeats add nothing (the history holds a point per poll, moved or not).
        points.dedup();
        if points.is_empty() {
            return;
        }
        points.reverse();
        let to = ((msg.lParam & 0xFFFF) as i16 as i32, ((msg.lParam >> 16) & 0xFFFF) as i16 as i32);
        if let Ok(mut t) = TRAILS.lock() {
            if t.len() > 32 {
                t.pop_front();
            }
            t.push_back(Trail { to, points });
        }
    }

    pub fn on_pointer(wparam: usize) {
        let id = (wparam & 0xFFFF) as u32;
        let mut kind = 0;
        // SAFETY: plain Win32 queries about a pointer id taken from the message.
        unsafe {
            if GetPointerType(id, &mut kind) == 0 || kind != PT_PEN {
                return;
            }
            let mut info: POINTER_PEN_INFO = std::mem::zeroed();
            if GetPointerPenInfo(id, &mut info) == 0 {
                return;
            }
            super::seen(1);
            let flags = info.penFlags;
            let pressure = (info.pressure > 0).then(|| (info.pressure as f32 / 1024.0).clamp(0.0, 1.0));
            let state = super::PenState {
                barrel: flags & PEN_FLAG_BARREL != 0,
                eraser: flags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED) != 0,
                pressure,
                tilt_x: (info.tiltX as f32 / 90.0).clamp(-1.0, 1.0),
                tilt_y: (info.tiltY as f32 / 90.0).clamp(-1.0, 1.0),
            };
            if let Ok(mut pens) = PENS.lock() {
                let map = pens.get_or_insert_with(HashMap::new);
                if map.len() > 64 {
                    map.clear();
                }
                map.insert(id as u64, state);
            }
            if let Ok(mut last) = LAST_PEN.lock() {
                *last = Some((
                    super::MousePen {
                        pressure: pressure.unwrap_or(0.0),
                        barrel: state.barrel,
                        eraser: state.eraser,
                        tilt_x: state.tilt_x,
                        tilt_y: state.tilt_y,
                    },
                    crate::platform::now_ms(),
                ));
            }
        }
    }

    /// Message hook for the event loop: notes pens and folded mouse moves, never consumes the
    /// message.
    pub fn hook(msg: *const c_void) -> bool {
        // SAFETY: winit passes a pointer to the MSG it is about to dispatch.
        let msg = unsafe { &*(msg as *const MSG) };
        if msg.message == WM_LBUTTONUP {
            if let Ok(mut up) = UP.lock() {
                *up = Some(msg.time);
            }
            return false;
        }
        if msg.message == WM_LBUTTONDOWN
            && let (Ok(up), Ok(mut gap)) = (UP.lock(), GAP.lock())
        {
            *gap = up.map(|u| msg.time.wrapping_sub(u));
        }
        if matches!(msg.message, WM_MOUSEMOVE | WM_LBUTTONDOWN) {
            mouse_move(msg);
            return false;
        }
        if super::wintab::on_message(msg) {
            return false;
        }
        if matches!(msg.message, WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP) {
            on_pointer(msg.wParam);
        }
        false
    }
}

/// Wintab, the drawing tablets' own interface (wintab32.dll, installed with their drivers): the
/// pressure of a pen that moves the mouse pointer because Windows Ink is off in its driver.
#[cfg(windows)]
mod wintab {
    use std::ffi::c_void;
    use std::sync::Mutex;

    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
    use windows_sys::Win32::UI::WindowsAndMessaging::MSG;

    const WT_PACKET: u32 = 0x7FF0;
    const WT_PROXIMITY: u32 = 0x7FF5;
    const WTI_DEFSYSCTX: u32 = 4;
    const WTI_DEVICES: u32 = 100;
    const DVC_NAME: u32 = 1;
    const DVC_NPRESSURE: u32 = 15;
    const CXO_MESSAGES: u32 = 0x0004;
    const PK_STATUS: u32 = 0x0002;
    const PK_TIME: u32 = 0x0004;
    const PK_CURSOR: u32 = 0x0020;
    const PK_BUTTONS: u32 = 0x0040;
    const PK_X: u32 = 0x0080;
    const PK_Y: u32 = 0x0100;
    const PK_NORMAL_PRESSURE: u32 = 0x0400;
    /// Set when the pen has left the tablet's reach.
    const TPS_PROXIMITY: u32 = 0x0001;
    /// The pen is turned round: its eraser end.
    const TPS_INVERT: u32 = 0x0010;
    /// What each packet holds; the fields of `Packet` follow, in the order of these bits.
    const DATA: u32 = PK_STATUS | PK_TIME | PK_CURSOR | PK_BUTTONS | PK_X | PK_Y | PK_NORMAL_PRESSURE;

    /// Wintab's LOGCONTEXTW.
    #[repr(C)]
    struct LogContext {
        name: [u16; 40],
        options: u32,
        status: u32,
        locks: u32,
        msg_base: u32,
        device: u32,
        pkt_rate: u32,
        pkt_data: u32,
        pkt_mode: u32,
        move_mask: u32,
        btn_dn_mask: u32,
        btn_up_mask: u32,
        in_org: [i32; 3],
        in_ext: [i32; 3],
        out_org: [i32; 3],
        out_ext: [i32; 3],
        sens: [u32; 3],
        sys_mode: i32,
        sys_org: [i32; 2],
        sys_ext: [i32; 2],
        sys_sens: [u32; 2],
    }

    // As wintab.h lays them out (a wrong size would read past the structures).
    const _: () = assert!(std::mem::size_of::<LogContext>() == 212);
    const _: () = assert!(std::mem::size_of::<Packet>() == 28);

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Packet {
        status: u32,
        time: u32,
        cursor: u32,
        buttons: u32,
        x: i32,
        y: i32,
        pressure: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Axis {
        min: i32,
        max: i32,
        units: u32,
        resolution: u32,
    }

    type Info = unsafe extern "system" fn(u32, u32, *mut c_void) -> u32;
    type Open = unsafe extern "system" fn(*mut c_void, *mut LogContext, i32) -> *mut c_void;
    type GetPacket = unsafe extern "system" fn(*mut c_void, u32, *mut c_void) -> i32;
    type Toggle = unsafe extern "system" fn(*mut c_void, i32) -> i32;

    struct Tablet {
        /// The context (a handle), kept as a number to live in a static.
        ctx: usize,
        packet: GetPacket,
        overlap: Toggle,
        enable: Toggle,
        min: f32,
        max: f32,
    }

    static TABLET: Mutex<Option<Tablet>> = Mutex::new(None);

    #[derive(Clone, Copy)]
    struct Last {
        pressure: f32,
        buttons: u32,
        eraser: bool,
        near: bool,
        at: f64,
    }

    static LAST: Mutex<Option<Last>> = Mutex::new(None);

    /// Opens a Wintab context on the window, when a tablet driver provides one. A driver that
    /// crashes Tratto while it opens is skipped afterwards (until the next version of Tratto):
    /// `dir` keeps the mark of an opening that never finished.
    pub fn open(hwnd: isize, dir: &std::path::Path) {
        super::imp::subclass(hwnd);
        if TABLET.lock().map_or(true, |t| t.is_some()) {
            return;
        }
        let name: Vec<u16> = "wintab32.dll\0".encode_utf16().collect();
        // SAFETY: loads the tablet driver's DLL from System32 only (never from the current folder).
        let lib = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
        if lib.is_null() {
            return;
        }
        let guard = dir.join("wintab.lock");
        if std::fs::read_to_string(&guard).is_ok_and(|v| v.trim() == env!("CARGO_PKG_VERSION")) {
            crate::diag::log("Wintab saltato: l'ultima volta Tratto si è chiuso mentre lo apriva");
            return;
        }
        let _ = std::fs::write(&guard, env!("CARGO_PKG_VERSION"));
        let done = |msg: String| {
            let _ = std::fs::remove_file(&guard);
            crate::diag::log(msg);
        };
        // SAFETY: the functions are looked up by their documented names and called with the
        // signatures and structures of wintab.h; every buffer passed is as large as asked for.
        unsafe {
            let find = |f: &[u8]| GetProcAddress(lib, f.as_ptr());
            let (Some(info), Some(open), Some(packet), Some(enable), Some(overlap)) = (find(b"WTInfoW\0"), find(b"WTOpenW\0"), find(b"WTPacket\0"), find(b"WTEnable\0"), find(b"WTOverlap\0")) else {
                return done("Wintab incompleto: nessuna pressione da lì".into());
            };
            let info: Info = std::mem::transmute(info);
            let open: Open = std::mem::transmute(open);
            if info(0, 0, std::ptr::null_mut()) == 0 {
                return done("Wintab presente ma spento".into());
            }
            let mut lc: LogContext = std::mem::zeroed();
            if info(WTI_DEFSYSCTX, 0, (&mut lc as *mut LogContext).cast()) == 0 {
                return done("Wintab: nessun contesto di sistema".into());
            }
            for (d, s) in lc.name.iter_mut().zip("Tratto\0".encode_utf16()) {
                *d = s;
            }
            lc.options |= CXO_MESSAGES;
            lc.pkt_data = DATA;
            lc.pkt_mode = 0;
            lc.move_mask = DATA;
            lc.btn_dn_mask = lc.btn_up_mask;
            let mut axis = Axis::default();
            info(WTI_DEVICES + lc.device, DVC_NPRESSURE, (&mut axis as *mut Axis).cast());
            let min = axis.min.max(0) as f32;
            let max = if axis.max > axis.min { axis.max as f32 } else { 1023.0 };
            let len = info(WTI_DEVICES + lc.device, DVC_NAME, std::ptr::null_mut()) as usize;
            let mut buf = vec![0u16; len / 2 + 1];
            if len > 0 {
                info(WTI_DEVICES + lc.device, DVC_NAME, buf.as_mut_ptr().cast());
            }
            let device = String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]);
            let ctx = open(hwnd as *mut c_void, &mut lc, 1);
            if ctx.is_null() {
                return done(format!("Wintab: non riesco ad aprire {device}"));
            }
            let (packet, enable, overlap): (GetPacket, Toggle, Toggle) = (std::mem::transmute(packet), std::mem::transmute(enable), std::mem::transmute(overlap));
            if let Ok(mut t) = TABLET.lock() {
                *t = Some(Tablet { ctx: ctx as usize, packet, overlap, enable, min, max });
            }
            done(format!("Wintab: {device}, pressione su {max} livelli"));
        }
    }

    /// Back in front: the tablet's packets come to this window again.
    pub fn focus(focused: bool) {
        if !focused {
            return;
        }
        if let Ok(t) = TABLET.lock()
            && let Some(t) = t.as_ref()
        {
            // SAFETY: the context opened above, still open.
            unsafe {
                (t.enable)(t.ctx as *mut c_void, 1);
                (t.overlap)(t.ctx as *mut c_void, 1);
            }
        }
    }

    pub fn on_raw(message: u32, wparam: usize, lparam: isize) -> bool {
        match message {
            WT_PACKET => {
                let Ok(t) = TABLET.lock() else { return true };
                let Some(t) = t.as_ref() else { return true };
                let mut p = Packet::default();
                // SAFETY: the packet the message announces (serial in wparam, context in lparam),
                // read into a structure laid out for the fields asked for.
                if unsafe { (t.packet)(lparam as *mut c_void, wparam as u32, (&mut p as *mut Packet).cast()) } != 0
                    && let Ok(mut last) = LAST.lock()
                {
                    let span = (t.max - t.min).max(1.0);
                    let pressure = ((p.pressure as f32 - t.min) / span).clamp(0.0, 1.0);
                    *last = Some(Last { pressure, buttons: p.buttons, eraser: p.status & TPS_INVERT != 0, near: p.status & TPS_PROXIMITY == 0, at: crate::platform::now_ms() });
                }
                true
            }
            WT_PROXIMITY => {
                // Low word 0: the pen left the context.
                if lparam & 0xFFFF == 0
                    && let Ok(mut last) = LAST.lock()
                    && let Some(l) = last.as_mut()
                {
                    l.near = false;
                }
                true
            }
            _ => false,
        }
    }

    /// Reads Wintab's messages from the event loop; true when the message was one of them.
    pub fn on_message(msg: &MSG) -> bool {
        on_raw(msg.message, msg.wParam, msg.lParam)
    }

    /// The pen, if it is within the tablet's reach and was heard from a moment ago (a pen lying
    /// still sends nothing, and the mouse may be in use meanwhile).
    pub fn current() -> Option<super::MousePen> {
        let last = (*LAST.lock().ok()?)?;
        if !last.near || crate::platform::now_ms() - last.at > 400.0 {
            return None;
        }
        super::seen(2);
        Some(super::MousePen { pressure: last.pressure, barrel: last.buttons & 2 != 0, eraser: last.eraser, tilt_x: 0.0, tilt_y: 0.0 })
    }
}

/// In a browser: the page's pointer events tell a pen (and its pressure and buttons) from a
/// mouse, and its touch events an Apple Pencil from a finger; read before eframe gets them.
#[cfg(target_arch = "wasm32")]
mod web {
    use std::sync::Mutex;

    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    #[derive(Clone, Copy)]
    struct Last {
        pressure: f32,
        buttons: u32,
        tilt_x: f32,
        tilt_y: f32,
        at: f64,
    }

    static LAST: Mutex<Option<Last>> = Mutex::new(None);
    /// When a pen last went down: the touch that follows it is the pen's.
    static DOWN_AT: Mutex<f64> = Mutex::new(0.0);
    /// Touches made with a pen (their ids as egui numbers them).
    static STYLUS: Mutex<Vec<u64>> = Mutex::new(Vec::new());

    fn get(v: &wasm_bindgen::JsValue, k: &str) -> Option<wasm_bindgen::JsValue> {
        js_sys::Reflect::get(v, &k.into()).ok()
    }

    fn recent(ms: f64) -> Option<Last> {
        LAST.lock().ok()?.filter(|l| js_sys::Date::now() - l.at <= ms)
    }

    pub fn listen() {
        let Some(window) = web_sys::window() else { return };
        let on_pointer = Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| {
            let v: &wasm_bindgen::JsValue = e.as_ref();
            if get(v, "pointerType").and_then(|t| t.as_string()).as_deref() != Some("pen") {
                return;
            }
            let pressure = get(v, "pressure").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32;
            let buttons = get(v, "buttons").and_then(|b| b.as_f64()).unwrap_or(0.0) as u32;
            let tilt_x = (get(v, "tiltX").and_then(|t| t.as_f64()).unwrap_or(0.0) as f32 / 90.0).clamp(-1.0, 1.0);
            let tilt_y = (get(v, "tiltY").and_then(|t| t.as_f64()).unwrap_or(0.0) as f32 / 90.0).clamp(-1.0, 1.0);
            if let Ok(mut last) = LAST.lock() {
                *last = Some(Last { pressure: pressure.clamp(0.0, 1.0), buttons, tilt_x, tilt_y, at: js_sys::Date::now() });
            }
            if e.type_() == "pointerdown"
                && let Ok(mut d) = DOWN_AT.lock()
            {
                *d = js_sys::Date::now();
            }
            super::seen(3);
        });
        for name in ["pointerdown", "pointermove", "pointerup"] {
            let _ = window.add_event_listener_with_callback_and_bool(name, on_pointer.as_ref().unchecked_ref(), true);
        }
        on_pointer.forget();
        // A touch is a pen's when the browser says so (Safari: touchType "stylus") or when a
        // pen's pointer went down just before it (the others).
        let on_touch = Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| {
            let v: &wasm_bindgen::JsValue = e.as_ref();
            let Some(changed) = get(v, "changedTouches") else { return };
            let n = get(&changed, "length").and_then(|l| l.as_f64()).unwrap_or(0.0) as u32;
            // Only the first touch after a pen went down is the pen's (a hand put down while
            // writing is not).
            let mut pen_down = DOWN_AT.lock().map(|mut d| js_sys::Date::now() - std::mem::take(&mut *d) <= 120.0).unwrap_or(false);
            let Ok(mut stylus) = STYLUS.lock() else { return };
            for i in 0..n {
                let Ok(t) = js_sys::Reflect::get(&changed, &i.into()) else { continue };
                let Some(id) = get(&t, "identifier").and_then(|x| x.as_f64()) else { continue };
                let id = id as i32 as u64;
                stylus.retain(|s| *s != id);
                if get(&t, "touchType").and_then(|x| x.as_string()).as_deref() == Some("stylus") || std::mem::take(&mut pen_down) {
                    stylus.push(id);
                    super::seen(3);
                }
            }
            let n = stylus.len();
            if n > 32 {
                stylus.drain(..n - 32);
            }
        });
        let _ = window.add_event_listener_with_callback_and_bool("touchstart", on_touch.as_ref().unchecked_ref(), true);
        on_touch.forget();
    }

    pub fn state(id: u64) -> Option<super::PenState> {
        if !STYLUS.lock().ok()?.contains(&id) {
            return None;
        }
        let l = recent(1000.0);
        let b = l.map_or(0, |l| l.buttons);
        let pressure = l.and_then(|l| (l.pressure > 0.0).then_some(l.pressure));
        let (tilt_x, tilt_y) = l.map_or((0.0, 0.0), |l| (l.tilt_x, l.tilt_y));
        Some(super::PenState { barrel: b & 2 != 0, eraser: b & 32 != 0, pressure, tilt_x, tilt_y })
    }

    pub fn mouse_pen() -> Option<super::MousePen> {
        let l = recent(150.0)?;
        Some(super::MousePen { pressure: l.pressure, barrel: l.buttons & 2 != 0, eraser: l.buttons & 32 != 0, tilt_x: l.tilt_x, tilt_y: l.tilt_y })
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::listen as listen_web;
