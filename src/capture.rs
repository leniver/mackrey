//! Global keyboard + mouse capture using Win32 low-level hooks.
//!
//! Hooks must live on a thread that pumps a message loop, so we spawn a
//! dedicated thread at startup that installs `WH_KEYBOARD_LL` + `WH_MOUSE_LL`
//! and runs `GetMessage` forever. The hook callbacks are plain functions (no
//! captured state), so they communicate through process-global statics:
//! an mpsc `Sender`, plus a few atomics for live configuration.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::time::Instant;

use crate::model::{Action, MouseButton};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WM_KEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

/// One thing the hooks observed, tagged with the moment it happened.
pub enum Captured {
    /// The configured record hotkey was pressed.
    ToggleHotkey,
    /// A real input action (only sent while recording is enabled).
    Input(Action),
}

pub struct Event {
    pub at: Instant,
    pub what: Captured,
}

// --- Global state shared with the C-ABI hook callbacks -------------------

static SENDER: OnceLock<Sender<Event>> = OnceLock::new();
static RECORDING: AtomicBool = AtomicBool::new(false);
static CAPTURE_MOVES: AtomicBool = AtomicBool::new(false);
/// Virtual-key code that toggles recording (default F9 = 0x78).
static HOTKEY_VK: AtomicU32 = AtomicU32::new(0x78);

/// Handle held by the app to receive captured events.
pub struct CaptureHandle {
    pub rx: Receiver<Event>,
}

impl CaptureHandle {
    /// Enable/disable recording of real input. The hotkey is always watched.
    pub fn set_recording(&self, on: bool) {
        RECORDING.store(on, Ordering::SeqCst);
    }
    pub fn set_capture_moves(&self, on: bool) {
        CAPTURE_MOVES.store(on, Ordering::SeqCst);
    }
    pub fn set_hotkey(&self, vk: u32) {
        HOTKEY_VK.store(vk, Ordering::SeqCst);
    }
}

/// Install the hooks on a dedicated background thread. Returns a handle that
/// receives captured events. Call once at startup.
pub fn start() -> CaptureHandle {
    let (tx, rx) = mpsc::channel();
    let _ = SENDER.set(tx);

    std::thread::spawn(|| unsafe {
        // hmod = None is fine for low-level hooks (they don't need a module).
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0);
        let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);
        if kb.is_err() || ms.is_err() {
            eprintln!("Mackrey: failed to install input hooks: kb={kb:?} ms={ms:?}");
            return;
        }

        // Pump messages so the low-level hooks fire.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            // No translation/dispatch needed; just keep the loop alive.
        }
    });

    CaptureHandle { rx }
}

fn send(what: Captured) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(Event {
            at: Instant::now(),
            what,
        });
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = info.vkCode;
        let scan = info.scanCode;
        let msg = wparam.0 as u32;

        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        // Hotkey: toggle on key-down, and never record the hotkey itself.
        if is_down && vk == HOTKEY_VK.load(Ordering::SeqCst) {
            send(Captured::ToggleHotkey);
            return CallNextHookEx(None, code, wparam, lparam);
        }

        if RECORDING.load(Ordering::SeqCst) {
            if is_down {
                send(Captured::Input(Action::KeyDown { vk, scan }));
            } else if is_up {
                send(Captured::Input(Action::KeyUp { vk, scan }));
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && RECORDING.load(Ordering::SeqCst) {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let x = info.pt.x;
        let y = info.pt.y;
        let msg = wparam.0 as u32;

        let action = match msg {
            WM_MOUSEMOVE => {
                if CAPTURE_MOVES.load(Ordering::SeqCst) {
                    Some(Action::MouseMove { x, y })
                } else {
                    None
                }
            }
            WM_LBUTTONDOWN => Some(Action::MouseDown { button: MouseButton::Left, x, y }),
            WM_LBUTTONUP => Some(Action::MouseUp { button: MouseButton::Left, x, y }),
            WM_RBUTTONDOWN => Some(Action::MouseDown { button: MouseButton::Right, x, y }),
            WM_RBUTTONUP => Some(Action::MouseUp { button: MouseButton::Right, x, y }),
            WM_MBUTTONDOWN => Some(Action::MouseDown { button: MouseButton::Middle, x, y }),
            WM_MBUTTONUP => Some(Action::MouseUp { button: MouseButton::Middle, x, y }),
            WM_XBUTTONDOWN => {
                let btn = xbutton(info.mouseData);
                Some(Action::MouseDown { button: btn, x, y })
            }
            WM_XBUTTONUP => {
                let btn = xbutton(info.mouseData);
                Some(Action::MouseUp { button: btn, x, y })
            }
            WM_MOUSEWHEEL => {
                // High word of mouseData is the signed wheel delta.
                let delta = ((info.mouseData >> 16) & 0xFFFF) as i16 as i32;
                Some(Action::Wheel { delta, x, y })
            }
            _ => None,
        };

        if let Some(a) = action {
            send(Captured::Input(a));
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Decode which X button from the high word of `mouseData`.
fn xbutton(mouse_data: u32) -> MouseButton {
    if ((mouse_data >> 16) & 0xFFFF) == 0x0002 {
        MouseButton::X2
    } else {
        MouseButton::X1
    }
}
