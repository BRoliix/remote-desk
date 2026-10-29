//! Video encoding. Software H.264 (OpenH264) for now; hardware encoders arrive in M5.

use anyhow::Context as _;
use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, RateControlMode,
    UsageType,
};
use openh264::formats::{BgraSliceU8, YUVBuffer};
use openh264::OpenH264API;
use rd_capture::Frame;

#[derive(Clone, Copy, Debug)]
pub struct EncoderSettings {
    pub bitrate_bps: u32,
    pub max_fps: f32,
    /// Seconds between keyframes, so a viewer that joins or loses packets recovers quickly.
    pub keyframe_secs: u32,
}

impl Default for EncoderSettings {
    fn default() -> Self {
        Self { bitrate_bps: 6_000_000, max_fps: 30.0, keyframe_secs: 2 }
    }
}

pub struct H264Encoder {
    settings: EncoderSettings,
    encoder: Option<Encoder>,
    yuv: Option<YUVBuffer>,
    dims: (usize, usize),
    scratch: Vec<u8>,
}

impl H264Encoder {
    pub fn new(settings: EncoderSettings) -> Self {
        Self { settings, encoder: None, yuv: None, dims: (0, 0), scratch: Vec::new() }
    }

    /// Encodes one frame into an Annex-B H.264 access unit (may be empty when the
    /// encoder decides to skip the frame).
    pub fn encode(&mut self, frame: &Frame) -> anyhow::Result<Vec<u8>> {
        // H.264 4:2:0 needs even dimensions; crop the odd row/column if present.
        let dims = ((frame.width & !1) as usize, (frame.height & !1) as usize);
        if dims != self.dims || self.encoder.is_none() {
            self.reset(dims)?;
        }

        let bgra = cropped_bgra(frame, dims, &mut self.scratch);
        let yuv = self.yuv.as_mut().expect("initialized in reset");
        yuv.read_bgra8(BgraSliceU8::new(bgra, dims));

        let encoder = self.encoder.as_mut().expect("initialized in reset");
        let bitstream = encoder.encode(yuv).context("H.264 encode")?;
        Ok(bitstream.to_vec())
    }

    pub fn force_keyframe(&mut self) {
        if let Some(e) = self.encoder.as_mut() {
            e.force_intra_frame();
        }
    }

    fn reset(&mut self, dims: (usize, usize)) -> anyhow::Result<()> {
        let s = self.settings;
        let config = EncoderConfig::new()
            // ScreenContentRealTime is ~3x slower at 1080p (see bench_1080p) and the
            // 30 fps target matters more than its sharper text.
            .usage_type(UsageType::CameraVideoRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .bitrate(BitRate::from_bps(s.bitrate_bps))
            .max_frame_rate(FrameRate::from_hz(s.max_fps))
            .intra_frame_period(IntraFramePeriod::from_num_frames(
                (s.max_fps as u32) * s.keyframe_secs,
            ))
            .complexity(Complexity::Low)
            .num_threads(encoder_threads())
            .skip_frames(true);
        self.encoder = Some(
            Encoder::with_api_config(OpenH264API::from_source(), config)
                .context("creating H.264 encoder")?,
        );
        self.yuv = Some(YUVBuffer::new(dims.0, dims.1));
        self.dims = dims;
        Ok(())
    }
}

/// Leave cores free for capture, networking and the rest of the machine.
fn encoder_threads() -> u16 {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cores / 2).clamp(1, 4) as u16
}

/// Returns tightly packed BGRA for `dims`, copying rows only when cropping is needed.
fn cropped_bgra<'a>(frame: &'a Frame, dims: (usize, usize), scratch: &'a mut Vec<u8>) -> &'a [u8] {
    let src_stride = frame.width as usize * 4;
    let dst_stride = dims.0 * 4;
    if src_stride == dst_stride {
        return &frame.data[..dst_stride * dims.1];
    }
    scratch.clear();
    for row in frame.data.chunks_exact(src_stride).take(dims.1) {
        scratch.extend_from_slice(&row[..dst_stride]);
    }
    scratch
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn frame(width: u32, height: u32) -> Frame {
        let data = (0..width * height * 4).map(|i| (i % 251) as u8).collect();
        Frame { width, height, data, captured_at: Instant::now() }
    }

    #[test]
    fn crops_odd_dimensions() {
        let f = frame(5, 3);
        let mut scratch = Vec::new();
        let out = cropped_bgra(&f, (4, 2), &mut scratch);
        assert_eq!(out.len(), 4 * 2 * 4);
        assert_eq!(&out[..16], &f.data[..16]);
        assert_eq!(&out[16..32], &f.data[20..36]);
    }

    /// cargo test --release -p rd-encode -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_1080p() {
        let f = frame(1920, 1080);
        let mut yuv = YUVBuffer::new(1920, 1080);
        let t = Instant::now();
        for _ in 0..30 {
            yuv.read_bgra8(BgraSliceU8::new(&f.data, (1920, 1080)));
        }
        println!("bgra->yuv: {:.1} ms/frame", t.elapsed().as_secs_f64() * 1000.0 / 30.0);

        let mut enc = H264Encoder::new(EncoderSettings::default());
        let t = Instant::now();
        for _ in 0..30 {
            enc.encode(&f).unwrap();
        }
        println!("convert+encode: {:.1} ms/frame", t.elapsed().as_secs_f64() * 1000.0 / 30.0);

        // Realistic content: mostly static desktop with a small changing region.
        let mut desk = frame(1920, 1080);
        desk.data.iter_mut().enumerate().for_each(|(i, b)| *b = ((i / 4 / 1920) % 200) as u8);
        for (name, usage, threads) in [
            ("screen/1t", UsageType::ScreenContentRealTime, 1),
            ("screen/4t", UsageType::ScreenContentRealTime, 4),
            ("camera/1t", UsageType::CameraVideoRealTime, 1),
            ("camera/4t", UsageType::CameraVideoRealTime, 4),
        ] {
            let cfg = EncoderConfig::new()
                .usage_type(usage)
                .rate_control_mode(RateControlMode::Bitrate)
                .bitrate(BitRate::from_bps(6_000_000))
                .max_frame_rate(FrameRate::from_hz(30.0))
                .complexity(Complexity::Low)
                .num_threads(threads);
            let mut e = Encoder::with_api_config(OpenH264API::from_source(), cfg).unwrap();
            let mut y = YUVBuffer::new(1920, 1080);
            let t = Instant::now();
            for n in 0..30usize {
                let row = (n * 37) % 1000;
                desk.data[row * 1920 * 4..(row + 40) * 1920 * 4].fill((n * 7) as u8);
                y.read_bgra8(BgraSliceU8::new(&desk.data, (1920, 1080)));
                e.encode(&y).unwrap();
            }
            println!("{name}: {:.1} ms/frame", t.elapsed().as_secs_f64() * 1000.0 / 30.0);
        }
    }

    #[test]
    fn first_frame_is_a_decodable_keyframe() {
        let mut enc = H264Encoder::new(EncoderSettings::default());
        let bytes = enc.encode(&frame(320, 240)).unwrap();
        assert!(!bytes.is_empty());
        let mut dec = openh264::decoder::Decoder::new().unwrap();
        // The decoder may need the whole access unit before it emits a picture.
        let _ = dec.decode(&bytes).unwrap();
    }
}
