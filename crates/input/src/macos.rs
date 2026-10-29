//! CGEvent backend. Needs the Accessibility permission
//! (System Settings > Privacy & Security > Accessibility) for the host binary.

use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use rd_proto::{MouseButton, Point};

use crate::{clamp_unit, Injector};

pub struct CgEventInjector {
    source: CGEventSource,
    held: Option<MouseButton>,
    last: CGPoint,
}

// CGEventSource is a CoreFoundation object that is safe to use from the one input
// thread that owns this injector.
unsafe impl Send for CgEventInjector {}

impl CgEventInjector {
    pub fn new() -> anyhow::Result<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| anyhow::anyhow!("cannot create CGEventSource"))?;
        Ok(Self { source, held: None, last: CGPoint::new(0.0, 0.0) })
    }

    fn to_screen(&mut self, at: Point) -> CGPoint {
        let bounds = CGDisplay::main().bounds();
        self.last = CGPoint::new(
            bounds.origin.x + clamp_unit(at.x) as f64 * bounds.size.width,
            bounds.origin.y + clamp_unit(at.y) as f64 * bounds.size.height,
        );
        self.last
    }

    fn post_mouse(&self, kind: CGEventType, at: CGPoint, button: CGMouseButton) -> anyhow::Result<()> {
        let event = CGEvent::new_mouse_event(self.source.clone(), kind, at, button)
            .map_err(|_| anyhow::anyhow!("cannot create mouse event"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }
}

impl Injector for CgEventInjector {
    fn mouse_move(&mut self, at: Point) -> anyhow::Result<()> {
        let p = self.to_screen(at);
        // While a button is held, macOS expects drag events, not plain moves.
        let (kind, button) = match self.held {
            Some(MouseButton::Left) => (CGEventType::LeftMouseDragged, CGMouseButton::Left),
            Some(MouseButton::Right) => (CGEventType::RightMouseDragged, CGMouseButton::Right),
            Some(MouseButton::Middle) => (CGEventType::OtherMouseDragged, CGMouseButton::Center),
            None => (CGEventType::MouseMoved, CGMouseButton::Left),
        };
        self.post_mouse(kind, p, button)
    }

    fn mouse_button(&mut self, button: MouseButton, down: bool, at: Point) -> anyhow::Result<()> {
        let p = self.to_screen(at);
        let (kind, cg_button) = match (button, down) {
            (MouseButton::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            (MouseButton::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            (MouseButton::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
            (MouseButton::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
            (MouseButton::Middle, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            (MouseButton::Middle, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
        };
        self.held = if down { Some(button) } else { None };
        self.post_mouse(kind, p, cg_button)
    }

    fn wheel(&mut self, dx: f32, dy: f32) -> anyhow::Result<()> {
        // macOS: positive wheel1 scrolls content up.
        let event = CGEvent::new_scroll_event(
            self.source.clone(),
            ScrollEventUnit::LINE,
            2,
            (-dy).round() as i32,
            (-dx).round() as i32,
            0,
        )
        .map_err(|_| anyhow::anyhow!("cannot create scroll event"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn key(&mut self, code: &str, down: bool) -> anyhow::Result<()> {
        let Some(keycode) = keycode(code) else { return Ok(()) };
        let event = CGEvent::new_keyboard_event(self.source.clone(), keycode, down)
            .map_err(|_| anyhow::anyhow!("cannot create key event"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }
}

/// KeyboardEvent.code -> macOS virtual keycode (kVK_*), ANSI positions.
fn keycode(code: &str) -> Option<u16> {
    Some(match code {
        "KeyA" => 0x00, "KeyS" => 0x01, "KeyD" => 0x02, "KeyF" => 0x03, "KeyH" => 0x04,
        "KeyG" => 0x05, "KeyZ" => 0x06, "KeyX" => 0x07, "KeyC" => 0x08, "KeyV" => 0x09,
        "KeyB" => 0x0B, "KeyQ" => 0x0C, "KeyW" => 0x0D, "KeyE" => 0x0E, "KeyR" => 0x0F,
        "KeyY" => 0x10, "KeyT" => 0x11, "Digit1" => 0x12, "Digit2" => 0x13, "Digit3" => 0x14,
        "Digit4" => 0x15, "Digit6" => 0x16, "Digit5" => 0x17, "Equal" => 0x18, "Digit9" => 0x19,
        "Digit7" => 0x1A, "Minus" => 0x1B, "Digit8" => 0x1C, "Digit0" => 0x1D,
        "BracketRight" => 0x1E, "KeyO" => 0x1F, "KeyU" => 0x20, "BracketLeft" => 0x21,
        "KeyI" => 0x22, "KeyP" => 0x23, "Enter" => 0x24, "KeyL" => 0x25, "KeyJ" => 0x26,
        "Quote" => 0x27, "KeyK" => 0x28, "Semicolon" => 0x29, "Backslash" => 0x2A,
        "Comma" => 0x2B, "Slash" => 0x2C, "KeyN" => 0x2D, "KeyM" => 0x2E, "Period" => 0x2F,
        "Tab" => 0x30, "Space" => 0x31, "Backquote" => 0x32, "Backspace" => 0x33,
        "Escape" => 0x35, "MetaRight" => 0x36, "MetaLeft" => 0x37, "ShiftLeft" => 0x38,
        "CapsLock" => 0x39, "AltLeft" => 0x3A, "ControlLeft" => 0x3B, "ShiftRight" => 0x3C,
        "AltRight" => 0x3D, "ControlRight" => 0x3E, "IntlBackslash" => 0x0A,
        "NumpadEnter" => 0x4C, "NumpadDivide" => 0x4B, "NumpadMultiply" => 0x43,
        "F1" => 0x7A, "F2" => 0x78, "F3" => 0x63, "F4" => 0x76, "F5" => 0x60, "F6" => 0x61,
        "F7" => 0x62, "F8" => 0x64, "F9" => 0x65, "F10" => 0x6D, "F11" => 0x67, "F12" => 0x6F,
        "Insert" => 0x72, "Home" => 0x73, "PageUp" => 0x74, "Delete" => 0x75, "End" => 0x77,
        "PageDown" => 0x79, "ArrowLeft" => 0x7B, "ArrowRight" => 0x7C, "ArrowDown" => 0x7D,
        "ArrowUp" => 0x7E,
        _ => return None,
    })
}
