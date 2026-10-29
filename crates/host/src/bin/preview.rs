//! M1 local loop: capture the screen -> H.264 encode -> decode -> show in a window.
//! Proves the media pipeline end to end on one machine and prints live stats.
//!
//!   cargo run --release -p rd-host --bin preview [-- --bitrate 6000 --fps 30]

use std::time::Instant;

use anyhow::Context as _;
use minifb::{Key, ScaleMode, Window, WindowOptions};
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use rd_encode::{EncoderSettings, H264Encoder};
use rd_host::Stats;

fn main() -> anyhow::Result<()> {
    let settings = parse_args()?;
    let mut capturer = rd_capture::primary_display()?;
    let mut encoder = H264Encoder::new(settings);
    let mut decoder = Decoder::new().context("creating H.264 decoder")?;
    let mut stats = Stats::new();

    let first = capturer.next_frame()?;
    let (w, h) = ((first.width / 2) as usize, (first.height / 2) as usize);
    let mut window = Window::new(
        "remote-desk preview (Esc to quit)",
        w,
        h,
        WindowOptions { resize: true, scale_mode: ScaleMode::AspectRatioStretch, ..Default::default() },
    )?;
    let mut rgba = Vec::new();
    let mut pixels: Vec<u32> = Vec::new();

    println!("capturing {}x{} at up to {} fps, {} kbps", first.width, first.height, settings.max_fps, settings.bitrate_bps / 1000);
    let mut frame = first;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let encode_start = Instant::now();
        let bitstream = encoder.encode(&frame)?;
        let encode_time = encode_start.elapsed();

        if !bitstream.is_empty() {
            if let Some(yuv) = decoder.decode(&bitstream).context("H.264 decode")? {
                let (dw, dh) = yuv.dimensions();
                rgba.resize(dw * dh * 4, 0);
                yuv.write_rgba8(&mut rgba);
                pixels.clear();
                pixels.extend(rgba.as_chunks::<4>().0.iter().map(|[r, g, b, _]| u32::from_be_bytes([0, *r, *g, *b])));
                window.update_with_buffer(&pixels, dw, dh)?;
            }
        } else {
            window.update();
        }

        if let Some(r) = stats.record(encode_time, frame.captured_at.elapsed(), bitstream.len()) {
            let line = format!(
                "{:5.1} fps | encode {:5.1} ms | capture->screen {:5.1} ms | {:6} kbps",
                r.fps, r.encode_ms, r.latency_ms, r.bitrate_kbps
            );
            println!("{line}");
            window.set_title(&format!("remote-desk preview | {line}"));
        }
        frame = capturer.next_frame()?;
    }
    Ok(())
}

fn parse_args() -> anyhow::Result<EncoderSettings> {
    let mut s = EncoderSettings::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().context("missing value");
        match arg.as_str() {
            "--bitrate" => s.bitrate_bps = value()?.parse::<u32>()? * 1000,
            "--fps" => s.max_fps = value()?.parse()?,
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    Ok(s)
}
