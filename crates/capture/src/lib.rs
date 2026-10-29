//! Screen capture. Each platform backend delivers BGRA frames of the primary display.

use std::time::{Duration, Instant};

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

    /// Like `next_frame`, but gives up after `timeout`. Backends that only deliver
    /// frames when the screen changes return `None` while it is static.
    fn next_frame_timeout(&mut self, timeout: Duration) -> anyhow::Result<Option<Frame>> {
        let _ = timeout;
        self.next_frame().map(Some)
    }
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
