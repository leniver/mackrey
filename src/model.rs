//! Data model for macros: the events, their timing, and the on-disk library.

use serde::{Deserialize, Serialize};

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl MouseButton {
    pub fn label(self) -> &'static str {
        match self {
            MouseButton::Left => "Left",
            MouseButton::Right => "Right",
            MouseButton::Middle => "Middle",
            MouseButton::X1 => "X1",
            MouseButton::X2 => "X2",
        }
    }
}

/// A single recorded action. Coordinates are absolute screen pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    KeyDown { vk: u32, scan: u32 },
    KeyUp { vk: u32, scan: u32 },
    MouseMove { x: i32, y: i32 },
    MouseDown { button: MouseButton, x: i32, y: i32 },
    MouseUp { button: MouseButton, x: i32, y: i32 },
    /// Positive = wheel up / away from user.
    Wheel { delta: i32, x: i32, y: i32 },
}

impl Action {
    /// Human-readable description for the editor list.
    pub fn describe(&self) -> String {
        match self {
            Action::KeyDown { vk, .. } => format!("Key ↓  {}", vk_name(*vk)),
            Action::KeyUp { vk, .. } => format!("Key ↑  {}", vk_name(*vk)),
            Action::MouseMove { x, y } => format!("Move    ({x}, {y})"),
            Action::MouseDown { button, x, y } => {
                format!("Mouse ↓ {} ({x}, {y})", button.label())
            }
            Action::MouseUp { button, x, y } => {
                format!("Mouse ↑ {} ({x}, {y})", button.label())
            }
            Action::Wheel { delta, .. } => format!("Wheel   {delta:+}"),
        }
    }
}

/// One step of a macro: an action plus the delay (ms) to wait *before* it,
/// measured from the previous step (or from recording start for the first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub delay_ms: u64,
    pub action: Action,
}

/// A named, ordered list of steps.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<Step>,
}

impl Macro {
    pub fn new(name: impl Into<String>) -> Self {
        Macro {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    /// Total run time of the macro in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.steps.iter().map(|s| s.delay_ms).sum()
    }
}

/// Best-effort friendly name for a virtual-key code.
pub fn vk_name(vk: u32) -> String {
    use windows_vk::*;
    match vk {
        VK_LBUTTON => "LMouse".into(),
        VK_RBUTTON => "RMouse".into(),
        VK_BACK => "Backspace".into(),
        VK_TAB => "Tab".into(),
        VK_RETURN => "Enter".into(),
        VK_SHIFT => "Shift".into(),
        VK_CONTROL => "Ctrl".into(),
        VK_MENU => "Alt".into(),
        VK_PAUSE => "Pause".into(),
        VK_CAPITAL => "CapsLock".into(),
        VK_ESCAPE => "Esc".into(),
        VK_SPACE => "Space".into(),
        VK_PRIOR => "PageUp".into(),
        VK_NEXT => "PageDown".into(),
        VK_END => "End".into(),
        VK_HOME => "Home".into(),
        VK_LEFT => "Left".into(),
        VK_UP => "Up".into(),
        VK_RIGHT => "Right".into(),
        VK_DOWN => "Down".into(),
        VK_INSERT => "Insert".into(),
        VK_DELETE => "Delete".into(),
        VK_LWIN | VK_RWIN => "Win".into(),
        0x30..=0x39 => ((vk as u8) as char).to_string(), // 0-9
        0x41..=0x5A => ((vk as u8) as char).to_string(), // A-Z
        0x70..=0x87 => format!("F{}", vk - 0x6F),         // F1-F24
        _ => format!("VK_0x{vk:02X}"),
    }
}

/// A handful of virtual-key constants we name explicitly.
mod windows_vk {
    pub const VK_LBUTTON: u32 = 0x01;
    pub const VK_RBUTTON: u32 = 0x02;
    pub const VK_BACK: u32 = 0x08;
    pub const VK_TAB: u32 = 0x09;
    pub const VK_RETURN: u32 = 0x0D;
    pub const VK_SHIFT: u32 = 0x10;
    pub const VK_CONTROL: u32 = 0x11;
    pub const VK_MENU: u32 = 0x12;
    pub const VK_PAUSE: u32 = 0x13;
    pub const VK_CAPITAL: u32 = 0x14;
    pub const VK_ESCAPE: u32 = 0x1B;
    pub const VK_SPACE: u32 = 0x20;
    pub const VK_PRIOR: u32 = 0x21;
    pub const VK_NEXT: u32 = 0x22;
    pub const VK_END: u32 = 0x23;
    pub const VK_HOME: u32 = 0x24;
    pub const VK_LEFT: u32 = 0x25;
    pub const VK_UP: u32 = 0x26;
    pub const VK_RIGHT: u32 = 0x27;
    pub const VK_DOWN: u32 = 0x28;
    pub const VK_INSERT: u32 = 0x2D;
    pub const VK_DELETE: u32 = 0x2E;
    pub const VK_LWIN: u32 = 0x5B;
    pub const VK_RWIN: u32 = 0x5C;
}
