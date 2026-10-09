//! Telling a pen from a finger. The window system reports both as touches; on Windows the
//! pointer messages say which is which, and whether the barrel button or the eraser end is in
//! use, so they are read here before winit turns them into touches.

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

#[cfg(windows)]
pub use imp::hook;

#[cfg(windows)]
mod imp {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::sync::Mutex;

    use windows_sys::Win32::UI::Input::Pointer::{GetPointerPenInfo, GetPointerType, POINTER_PEN_INFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, PEN_FLAG_BARREL, PEN_FLAG_ERASER, PEN_FLAG_INVERTED, PT_PEN, WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE};

    pub static PENS: Mutex<Option<HashMap<u64, super::PenState>>> = Mutex::new(None);

    /// Message hook for the event loop: notes pens, never consumes the message.
    pub fn hook(msg: *const c_void) -> bool {
        // SAFETY: winit passes a pointer to the MSG it is about to dispatch.
        let msg = unsafe { &*(msg as *const MSG) };
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
