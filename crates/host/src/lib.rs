//! Host agent. M1 only contains the stats helper used by the preview tool;
//! the WebRTC session lands here in M2.

use std::time::{Duration, Instant};

/// Rolling once-per-second counters for fps, encode time, bitrate and latency.
pub struct Stats {
    window_start: Instant,
    frames: u32,
    encode_total: Duration,
    latency_total: Duration,
    bytes: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct StatsReport {
    pub fps: f32,
    pub encode_ms: f32,
    pub latency_ms: f32,
    pub bitrate_kbps: u32,
}

impl Default for Stats {
    fn default() -> Self {
        Self::new()
    }
}

impl Stats {
    pub fn new() -> Self {
        Self {
            window_start: Instant::now(),
            frames: 0,
            encode_total: Duration::ZERO,
            latency_total: Duration::ZERO,
            bytes: 0,
        }
    }

    /// Records one frame; returns a report when a full second has elapsed.
    pub fn record(&mut self, encode: Duration, latency: Duration, bytes: usize) -> Option<StatsReport> {
        self.frames += 1;
        self.encode_total += encode;
        self.latency_total += latency;
        self.bytes += bytes;

        let elapsed = self.window_start.elapsed();
        if elapsed < Duration::from_secs(1) {
            return None;
        }
        let secs = elapsed.as_secs_f32();
        let n = self.frames as f32;
        let report = StatsReport {
            fps: n / secs,
            encode_ms: self.encode_total.as_secs_f32() * 1000.0 / n,
            latency_ms: self.latency_total.as_secs_f32() * 1000.0 / n,
            bitrate_kbps: (self.bytes as f32 * 8.0 / secs / 1000.0) as u32,
        };
        *self = Self::new();
        Some(report)
    }
}
