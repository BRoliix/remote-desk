//! Windows.Graphics.Capture backend. The OS pushes frames on its own thread;
//! we keep only the newest one so a slow encoder never builds up latency.

use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame as WgcFrame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::{Capturer, Frame};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

struct Handler {
    tx: SyncSender<Frame>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = SyncSender<Frame>;
    type Error = BoxError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { tx: ctx.flags })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut WgcFrame,
        control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let captured_at = Instant::now();
        let mut buffer = frame.buffer()?;
        let (width, height) = (buffer.width(), buffer.height());
        let data = buffer.as_nopadding_buffer()?.to_vec();
        match self.tx.try_send(Frame { width, height, data, captured_at }) {
            // Consumer is still busy with the previous frame: drop this one.
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => control.stop(),
        }
        Ok(())
    }
}

pub struct WgcCapturer {
    rx: Receiver<Frame>,
    _control: CaptureControl<Handler, BoxError>,
}

impl WgcCapturer {
    pub fn start() -> anyhow::Result<Self> {
        let (tx, rx) = sync_channel(1);
        let monitor = Monitor::primary().context("no primary monitor")?;
        let settings = Settings::new(
            monitor,
            CursorCaptureSettings::WithCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            tx,
        );
        let control = Handler::start_free_threaded(settings)
            .map_err(|e| anyhow::anyhow!("failed to start capture: {e}"))?;
        Ok(Self { rx, _control: control })
    }
}

impl Capturer for WgcCapturer {
    fn next_frame(&mut self) -> anyhow::Result<Frame> {
        self.rx.recv().context("capture thread stopped")
    }

    fn next_frame_timeout(&mut self, timeout: Duration) -> anyhow::Result<Option<Frame>> {
        match self.rx.recv_timeout(timeout) {
            Ok(frame) => Ok(Some(frame)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => anyhow::bail!("capture thread stopped"),
        }
    }
}
