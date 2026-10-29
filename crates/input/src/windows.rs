//! SendInput backend. Mouse positions use absolute 0..=65535 coordinates on the
//! primary monitor (the one we capture), keys use hardware scancodes.

use std::mem::size_of;

use rd_proto::{MouseButton, Point};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};

use crate::{clamp_unit, Injector};

const WHEEL_DELTA: f32 = 120.0;

pub struct SendInputInjector;

impl SendInputInjector {
    pub fn new() -> Self {
        Self
    }
}

impl Injector for SendInputInjector {
    fn mouse_move(&mut self, at: Point) -> anyhow::Result<()> {
        send(&[mouse(at, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE, 0)])
    }

    fn mouse_button(&mut self, button: MouseButton, down: bool, at: Point) -> anyhow::Result<()> {
        let flag = match (button, down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
            (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        };
        send(&[mouse(at, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | flag, 0)])
    }

    fn wheel(&mut self, dx: f32, dy: f32) -> anyhow::Result<()> {
        let origin = Point { x: 0.0, y: 0.0 };
        let mut events = Vec::new();
        if dy != 0.0 {
            // Windows: positive delta scrolls content up.
            events.push(mouse(origin, MOUSEEVENTF_WHEEL, (-dy * WHEEL_DELTA) as i32));
        }
        if dx != 0.0 {
            events.push(mouse(origin, MOUSEEVENTF_HWHEEL, (dx * WHEEL_DELTA) as i32));
        }
        send(&events)
    }

    fn key(&mut self, code: &str, down: bool) -> anyhow::Result<()> {
        let Some((scan, extended)) = scancode(code) else { return Ok(()) };
        let mut flags = KEYEVENTF_SCANCODE;
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if !down {
            flags |= KEYEVENTF_KEYUP;
        }
        send(&[keyboard(scan, flags)])
    }
}

fn mouse(at: Point, flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: (clamp_unit(at.x) * 65535.0) as i32,
                dy: (clamp_unit(at.y) * 65535.0) as i32,
                mouseData: data as _,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn keyboard(scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

fn send(inputs: &[INPUT]) -> anyhow::Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        // Usually UIPI: the focused window belongs to an elevated process (M7 fixes this).
        anyhow::bail!("SendInput blocked ({sent}/{} events)", inputs.len());
    }
    Ok(())
}

/// KeyboardEvent.code -> (set-1 scancode, needs E0 extended prefix).
fn scancode(code: &str) -> Option<(u16, bool)> {
    const LETTERS: [(char, u16); 26] = [
        ('A', 0x1E), ('B', 0x30), ('C', 0x2E), ('D', 0x20), ('E', 0x12), ('F', 0x21), ('G', 0x22),
        ('H', 0x23), ('I', 0x17), ('J', 0x24), ('K', 0x25), ('L', 0x26), ('M', 0x32), ('N', 0x31),
        ('O', 0x18), ('P', 0x19), ('Q', 0x10), ('R', 0x13), ('S', 0x1F), ('T', 0x14), ('U', 0x16),
        ('V', 0x2F), ('W', 0x11), ('X', 0x2D), ('Y', 0x15), ('Z', 0x2C),
    ];
    if let Some(letter) = code.strip_prefix("Key").and_then(|s| s.chars().next()) {
        return LETTERS.iter().find(|(c, _)| *c == letter).map(|(_, s)| (*s, false));
    }
    if let Some(d) = code.strip_prefix("Digit").and_then(|s| s.parse::<u16>().ok()) {
        return Some((if d == 0 { 0x0B } else { 0x01 + d }, false));
    }
    if let Some(n) = code.strip_prefix('F').and_then(|s| s.parse::<u16>().ok()) {
        return match n {
            1..=10 => Some((0x3A + n, false)),
            11 => Some((0x57, false)),
            12 => Some((0x58, false)),
            _ => None,
        };
    }
    Some(match code {
        "Escape" => (0x01, false),
        "Minus" => (0x0C, false),
        "Equal" => (0x0D, false),
        "Backspace" => (0x0E, false),
        "Tab" => (0x0F, false),
        "BracketLeft" => (0x1A, false),
        "BracketRight" => (0x1B, false),
        "Enter" => (0x1C, false),
        "ControlLeft" => (0x1D, false),
        "Semicolon" => (0x27, false),
        "Quote" => (0x28, false),
        "Backquote" => (0x29, false),
        "ShiftLeft" => (0x2A, false),
        "Backslash" => (0x2B, false),
        "Comma" => (0x33, false),
        "Period" => (0x34, false),
        "Slash" => (0x35, false),
        "ShiftRight" => (0x36, false),
        "NumpadMultiply" => (0x37, false),
        "AltLeft" => (0x38, false),
        "Space" => (0x39, false),
        "CapsLock" => (0x3A, false),
        "IntlBackslash" => (0x56, false),
        "ControlRight" => (0x1D, true),
        "AltRight" => (0x38, true),
        "NumpadEnter" => (0x1C, true),
        "NumpadDivide" => (0x35, true),
        "Home" => (0x47, true),
        "ArrowUp" => (0x48, true),
        "PageUp" => (0x49, true),
        "ArrowLeft" => (0x4B, true),
        "ArrowRight" => (0x4D, true),
        "End" => (0x4F, true),
        "ArrowDown" => (0x50, true),
        "PageDown" => (0x51, true),
        "Insert" => (0x52, true),
        "Delete" => (0x53, true),
        "MetaLeft" => (0x5B, true),
        "MetaRight" => (0x5C, true),
        "ContextMenu" => (0x5D, true),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::scancode;

    #[test]
    fn maps_common_keys() {
        assert_eq!(scancode("KeyA"), Some((0x1E, false)));
        assert_eq!(scancode("KeyZ"), Some((0x2C, false)));
        assert_eq!(scancode("Digit1"), Some((0x02, false)));
        assert_eq!(scancode("Digit0"), Some((0x0B, false)));
        assert_eq!(scancode("F1"), Some((0x3B, false)));
        assert_eq!(scancode("F10"), Some((0x44, false)));
        assert_eq!(scancode("F12"), Some((0x58, false)));
        assert_eq!(scancode("ArrowLeft"), Some((0x4B, true)));
        assert_eq!(scancode("MetaLeft"), Some((0x5B, true)));
        assert_eq!(scancode("Unidentified"), None);
    }
}
