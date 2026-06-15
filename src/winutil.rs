//! Enumerate top-level windows so the user can pick a playback target.

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
};

#[derive(Clone)]
pub struct WindowInfo {
    /// Raw HWND value (kept as isize so it's Send and easy to store).
    pub hwnd: isize,
    pub title: String,
}

impl WindowInfo {
    pub fn handle(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }
}

/// List visible top-level windows that have a non-empty title.
pub fn list_windows() -> Vec<WindowInfo> {
    let mut out: Vec<WindowInfo> = Vec::new();
    let ptr = &mut out as *mut Vec<WindowInfo> as isize;
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(ptr));
    }
    out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    out
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return TRUE;
    }
    let len = GetWindowTextLengthW(hwnd);
    if len == 0 {
        return TRUE;
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    let read = GetWindowTextW(hwnd, &mut buf);
    if read > 0 {
        let title = String::from_utf16_lossy(&buf[..read as usize]);
        let out = &mut *(lparam.0 as *mut Vec<WindowInfo>);
        out.push(WindowInfo {
            hwnd: hwnd.0 as isize,
            title,
        });
    }
    TRUE
}
