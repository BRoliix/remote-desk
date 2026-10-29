//! Baseline macOS backend: polls the primary display through xcap.
//! Good enough for M1/M2; ScreenCaptureKit replaces it in M5 for higher fps.
//! Needs the Screen Recording permission (System Settings > Privacy & Security).

use std::time::{Duration, Instant};

use anyhow::Context as _;
use xcap::Monitor;

use crate::{Capturer, Frame};

const FRAME_INTERVAL: Duration = Duration::from_millis(33);

pub struct XcapCapturer {
    monitor: Monitor,
    last: Option<Instant>,
}

impl XcapCapturer {
    pub fn new() -> anyhow::Result<Self> {
        let monitors = Monitor::all().context("listing displays")?;
        let monitor = monitors
            .iter()
            .position(|m| m.is_primary().unwrap_or(false))
            .map(|i| monitors[i].clone())
            .or_else(|| monitors.first().cloned())
            .context("no display found")?;
        Ok(Self { monitor, last: None })
    }
}

impl Capturer for XcapCapturer {
    fn next_frame(&mut self) -> anyhow::Result<Frame> {
        if let Some(last) = self.last {
            if let Some(wait) = FRAME_INTERVAL.checked_sub(last.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        self.last = Some(Instant::now());
        let captured_at = Instant::now();
        let image = self.monitor.capture_image().context("capturing display")?;
        let (width, height) = image.dimensions();
        let mut data = image.into_raw();
        for px in data.as_chunks_mut::<4>().0 {
            px.swap(0, 2); // RGBA -> BGRA
        }
        Ok(Frame { width, height, data, captured_at })
    }
}
