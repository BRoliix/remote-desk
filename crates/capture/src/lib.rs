//! Screen capture. Each platform backend delivers BGRA frames of the primary display.

use std::time::Instant;

#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
mod macos;

/// One captured frame, tightly packed BGRA (stride == width * 4).
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub captured_at: Instant,
}

pub trait Capturer: Send {
    /// Blocks until the next frame is available.
    fn next_frame(&mut self) -> anyhow::Result<Frame>;
}

/// Starts capturing the primary display with the best backend for this OS.
pub fn primary_display() -> anyhow::Result<Box<dyn Capturer>> {
    #[cfg(windows)]
    return Ok(Box::new(windows::WgcCapturer::start()?));
    #[cfg(target_os = "macos")]
    return Ok(Box::new(macos::XcapCapturer::new()?));
    #[cfg(not(any(windows, target_os = "macos")))]
    anyhow::bail!("screen capture is not supported on this OS");
}
