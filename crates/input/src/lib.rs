//! Injects the viewer's mouse and keyboard into this machine.
//!
//! Keys arrive as W3C `KeyboardEvent.code` values (physical key positions), which each
//! backend maps to scancodes (Windows) or virtual keycodes (macOS). Physical mapping
//! means the host's own keyboard layout decides which character is typed.

use rd_proto::{MouseButton, Point};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

pub trait Injector {
    fn mouse_move(&mut self, at: Point) -> anyhow::Result<()>;
    fn mouse_button(&mut self, button: MouseButton, down: bool, at: Point) -> anyhow::Result<()>;
    fn wheel(&mut self, dx: f32, dy: f32) -> anyhow::Result<()>;
    /// Unknown codes are ignored rather than treated as errors.
    fn key(&mut self, code: &str, down: bool) -> anyhow::Result<()>;
}

/// Injector for the primary display of this machine.
pub fn primary_display() -> anyhow::Result<Box<dyn Injector>> {
    #[cfg(windows)]
    return Ok(Box::new(windows::SendInputInjector::new()));
    #[cfg(target_os = "macos")]
    return Ok(Box::new(macos::CgEventInjector::new()?));
    #[cfg(not(any(windows, target_os = "macos")))]
    anyhow::bail!("input injection is not supported on this OS");
}

pub(crate) fn clamp_unit(v: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
}
