//! Macro playback.
//!
//! Two strategies:
//!  * [`PlayMode::Blocking`] uses `SendInput` to synthesize real OS input. It
//!    is reliable (works with games / DirectInput) but takes over the user's
//!    actual keyboard & mouse. Optionally brings the target window forward.
//!  * [`PlayMode::Background`] uses `PostMessage` to deliver window messages to
//!    a specific target window without stealing focus, so the user can keep
//!    working. Best-effort: many games and some apps ignore posted input.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use crate::model::{Action, Macro, MouseButton};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEINPUT, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, PostMessageW, SetForegroundWindow, ShowWindow, SM_CXSCREEN,
    SM_CYSCREEN, SW_RESTORE, WHEEL_DELTA, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

/// XBUTTON identifiers for the high word of mouse messages / `mouseData`.
const XBUTTON1: u32 = 0x0001;
const XBUTTON2: u32 = 0x0002;

/// Pack a low/high u16 pair into an LPARAM-style i32 (like the MAKELPARAM macro).
fn makelparam(low: u16, high: u16) -> i32 {
    ((low as u32) | ((high as u32) << 16)) as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    /// Synthesize real input with SendInput (takes over the machine).
    Blocking,
    /// Post messages to a specific window (background, best-effort).
    Background,
}

/// Options for a single playback run.
pub struct PlayOptions {
    pub mode: PlayMode,
    /// Target window. Required for Background; optional for Blocking (when set,
    /// the window is brought to the foreground before input is synthesized).
    pub target: Option<HWND>,
    /// Speed multiplier (1.0 = recorded speed, 2.0 = twice as fast).
    pub speed: f32,
    /// How many times to run the macro.
    pub repeat: u32,
}

/// Spawn playback on a background thread. The returned flag can be set to
/// `false` to request an early stop (checked between steps).
pub fn play(mac: Macro, opts: PlayOptions) -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    let target = opts.target.map(|h| h.0 as isize); // HWND isn't Send; pass the raw handle

    std::thread::spawn(move || {
        let speed = if opts.speed <= 0.0 { 1.0 } else { opts.speed };
        let target = target.map(|h| HWND(h as *mut _));

        if opts.mode == PlayMode::Blocking {
            if let Some(hwnd) = target {
                bring_to_front(hwnd);
                sleep(Duration::from_millis(120));
            }
        }

        for _ in 0..opts.repeat.max(1) {
            for step in &mac.steps {
                if !flag.load(Ordering::SeqCst) {
                    return;
                }
                let wait = (step.delay_ms as f32 / speed) as u64;
                if wait > 0 {
                    sleep(Duration::from_millis(wait));
                }
                match opts.mode {
                    PlayMode::Blocking => send_input(&step.action),
                    PlayMode::Background => {
                        if let Some(hwnd) = target {
                            post_to_window(hwnd, &step.action);
                        }
                    }
                }
            }
        }
        flag.store(false, Ordering::SeqCst);
    });

    running
}

/// Bring a window to the foreground (restoring it if minimized).
fn bring_to_front(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
    }
}

// --- SendInput (blocking) ------------------------------------------------

fn send_input(action: &Action) {
    let input = match action {
        Action::KeyDown { vk, scan } => key_input(*vk, *scan, false),
        Action::KeyUp { vk, scan } => key_input(*vk, *scan, true),
        Action::MouseMove { x, y } => mouse_move(*x, *y),
        Action::MouseDown { button, x, y } => mouse_button(*button, *x, *y, true),
        Action::MouseUp { button, x, y } => mouse_button(*button, *x, *y, false),
        Action::Wheel { delta, x, y } => mouse_wheel(*delta, *x, *y),
    };
    unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

fn key_input(vk: u32, scan: u32, up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if scan != 0 {
        flags |= KEYEVENTF_SCANCODE;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk as u16),
                wScan: scan as u16,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Convert absolute screen pixels to the 0..65535 normalized space SendInput
/// expects with MOUSEEVENTF_ABSOLUTE.
fn normalize(x: i32, y: i32) -> (i32, i32) {
    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN).max(1);
        let h = GetSystemMetrics(SM_CYSCREEN).max(1);
        let nx = (x as f64 * 65535.0 / w as f64).round() as i32;
        let ny = (y as f64 * 65535.0 / h as f64).round() as i32;
        (nx, ny)
    }
}

fn mouse_move(x: i32, y: i32) -> INPUT {
    let (nx, ny) = normalize(x, y);
    mouse_raw(nx, ny, 0, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE)
}

fn mouse_button(button: MouseButton, x: i32, y: i32, down: bool) -> INPUT {
    let (nx, ny) = normalize(x, y);
    let (flag, data) = match (button, down) {
        (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
        (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
        (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
        (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
        (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
        (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
        (MouseButton::X1, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
        (MouseButton::X1, false) => (MOUSEEVENTF_XUP, XBUTTON1),
        (MouseButton::X2, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
        (MouseButton::X2, false) => (MOUSEEVENTF_XUP, XBUTTON2),
    };
    mouse_raw(nx, ny, data, flag | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE)
}

fn mouse_wheel(delta: i32, x: i32, y: i32) -> INPUT {
    let (nx, ny) = normalize(x, y);
    mouse_raw(nx, ny, delta as u32, MOUSEEVENTF_WHEEL | MOUSEEVENTF_ABSOLUTE)
}

fn mouse_raw(dx: i32, dy: i32, data: u32, flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

// --- PostMessage (background, best-effort) -------------------------------

fn post_to_window(hwnd: HWND, action: &Action) {
    unsafe {
        match action {
            Action::KeyDown { vk, .. } => {
                let _ = PostMessageW(hwnd, WM_KEYDOWN, WPARAM(*vk as usize), LPARAM(0));
            }
            Action::KeyUp { vk, .. } => {
                let _ = PostMessageW(hwnd, WM_KEYUP, WPARAM(*vk as usize), LPARAM(0));
            }
            Action::MouseMove { x, y } => {
                let lp = makelparam(*x as u16, *y as u16);
                let _ = PostMessageW(hwnd, WM_MOUSEMOVE, WPARAM(0), LPARAM(lp as isize));
            }
            Action::MouseDown { button, x, y } => {
                if let Some(msg) = down_msg(*button) {
                    let lp = makelparam(*x as u16, *y as u16);
                    let _ = PostMessageW(hwnd, msg, WPARAM(0), LPARAM(lp as isize));
                }
            }
            Action::MouseUp { button, x, y } => {
                if let Some(msg) = up_msg(*button) {
                    let lp = makelparam(*x as u16, *y as u16);
                    let _ = PostMessageW(hwnd, msg, WPARAM(0), LPARAM(lp as isize));
                }
            }
            Action::Wheel { delta, x, y } => {
                let wheel = (*delta / WHEEL_DELTA as i32) * WHEEL_DELTA as i32;
                let wp = makelparam(0, wheel as u16) as usize;
                let lp = makelparam(*x as u16, *y as u16);
                let _ = PostMessageW(hwnd, WM_MOUSEWHEEL, WPARAM(wp), LPARAM(lp as isize));
            }
        }
    }
}

fn down_msg(b: MouseButton) -> Option<u32> {
    match b {
        MouseButton::Left => Some(WM_LBUTTONDOWN),
        MouseButton::Right => Some(WM_RBUTTONDOWN),
        MouseButton::Middle => Some(WM_MBUTTONDOWN),
        _ => None,
    }
}

fn up_msg(b: MouseButton) -> Option<u32> {
    match b {
        MouseButton::Left => Some(WM_LBUTTONUP),
        MouseButton::Right => Some(WM_RBUTTONUP),
        MouseButton::Middle => Some(WM_MBUTTONUP),
        _ => None,
    }
}
