//! Telling a pen from a finger. The window system reports both as touches; on Windows the
//! pointer messages say which is which, and whether the barrel button or the eraser end is in
//! use, so they are read here before winit turns them into touches.
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
}

/// The pen behind a touch id, or None for a finger.
#[cfg(windows)]
pub fn state(id: u64) -> Option<PenState> {
    imp::PENS.lock().ok()?.as_ref()?.get(&id).copied()
}

#[cfg(not(windows))]
pub fn state(_id: u64) -> Option<PenState> {
    None
}

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

#[cfg(windows)]
pub use imp::hook;

#[cfg(windows)]
mod imp {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::sync::Mutex;

    use std::collections::VecDeque;

    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GMMP_USE_DISPLAY_POINTS, GetMouseMovePointsEx, MOUSEMOVEPOINT};
    use windows_sys::Win32::UI::Input::Pointer::{GetPointerPenInfo, GetPointerType, POINTER_PEN_INFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, PEN_FLAG_BARREL, PEN_FLAG_ERASER, PEN_FLAG_INVERTED, PT_PEN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE};

    pub static PENS: Mutex<Option<HashMap<u64, super::PenState>>> = Mutex::new(None);

    /// The positions folded into one mouse move, and that move's position (window pixels).
    pub struct Trail {
        pub to: (i32, i32),
        pub points: Vec<(i32, i32)>,
    }

    pub static TRAILS: Mutex<VecDeque<Trail>> = Mutex::new(VecDeque::new());
    /// The last mouse position seen, as the history reports it: time and 16-bit screen x, y.
    static LAST: Mutex<Option<(u32, i32, i32)>> = Mutex::new(None);

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
        // Newest first; the first is the move itself. Stop at the previous move.
        let mut points = Vec::new();
        for p in &buf[1..n as usize] {
            if p.time < lt || (p.time == lt && p.x == lx && p.y == ly) || points.len() >= 62 {
                break;
            }
            let mut q = POINT { x: signed(p.x), y: signed(p.y) };
            // SAFETY: the window of the message; q is a valid POINT.
            if unsafe { ScreenToClient(msg.hwnd, &mut q) } == 0 {
                return;
            }
            points.push((q.x, q.y));
        }
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

    /// Message hook for the event loop: notes pens and folded mouse moves, never consumes the
    /// message.
    pub fn hook(msg: *const c_void) -> bool {
        // SAFETY: winit passes a pointer to the MSG it is about to dispatch.
        let msg = unsafe { &*(msg as *const MSG) };
        if matches!(msg.message, WM_MOUSEMOVE | WM_LBUTTONDOWN) {
            mouse_move(msg);
            return false;
        }
        if !matches!(msg.message, WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP) {
            return false;
        }
        let id = (msg.wParam & 0xFFFF) as u32;
        let mut kind = 0;
        // SAFETY: plain Win32 queries about a pointer id taken from the message.
        unsafe {
            if GetPointerType(id, &mut kind) == 0 || kind != PT_PEN {
                return false;
            }
            let mut info: POINTER_PEN_INFO = std::mem::zeroed();
            if GetPointerPenInfo(id, &mut info) == 0 {
                return false;
            }
            let flags = info.penFlags;
            let state = super::PenState { barrel: flags & PEN_FLAG_BARREL != 0, eraser: flags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED) != 0 };
            if let Ok(mut pens) = PENS.lock() {
                let map = pens.get_or_insert_with(HashMap::new);
                if map.len() > 64 {
                    map.clear();
                }
                map.insert(id as u64, state);
            }
        }
        false
    }
}
