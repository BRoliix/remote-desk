//! One remote-control session: a WebRTC peer connection carrying the screen as an
//! H.264 video track and a "control" data channel for input and clipboard.
//!
//! Threads: capture+encode and input injection run on dedicated OS threads (they are
//! blocking and, on macOS, not Send-friendly); everything network-facing is async.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use bytes::Bytes;
use rd_encode::{EncoderSettings, H264Encoder};
use rd_proto::{HostMsg, ViewerMsg};
use rtc::media::Sample;
use rtc::media_stream::MediaStreamTrack;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use rtc::rtp_transceiver::PayloadType;
use tokio::sync::{mpsc, oneshot};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::Track;
use webrtc::peer_connection::{
    register_default_interceptors, MediaEngine, PeerConnection, PeerConnectionBuilder,
    PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceGatheringState,
    RTCPeerConnectionState, RTCSessionDescription, Registry,
};
use webrtc::rtp_transceiver::RtpSender;

/// Fixed UDP port for media, so the firewall needs one rule (see windows/setup.ps1).
pub const MEDIA_UDP_PORT: u16 = 47081;
const MAX_FPS: f32 = 30.0;
const IDLE_REFRESH: Duration = Duration::from_secs(1);
const H264_PAYLOAD_TYPE: PayloadType = 102;

/// Shared between the network side and the worker threads of one session.
struct Shared {
    stop: AtomicBool,
    keyframe: AtomicBool,
    display: Mutex<(u32, u32)>,
}

pub struct Session {
    pc: Box<dyn PeerConnection>,
    shared: Arc<Shared>,
}

impl Session {
    /// Answers the viewer's offer. Returns once ICE gathering is complete, so the answer
    /// carries every host candidate (no trickle ICE needed on a LAN or tailnet).
    pub async fn answer(offer: RTCSessionDescription) -> anyhow::Result<(Self, RTCSessionDescription)> {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            keyframe: AtomicBool::new(false),
            display: Mutex::new((0, 0)),
        });

        let codec = RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: "video/H264".to_owned(),
                clock_rate: 90_000,
                channels: 0,
                sdp_fmtp_line: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                    .to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: H264_PAYLOAD_TYPE,
        };
        let mut media_engine = MediaEngine::default();
        media_engine.register_codec(codec.clone(), RtpCodecKind::Video)?;
        let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

        let (gathered_tx, gathered_rx) = oneshot::channel();
        let (connected_tx, connected_rx) = oneshot::channel();
        let handler = Arc::new(Handler {
            shared: shared.clone(),
            gathered: Mutex::new(Some(gathered_tx)),
            connected: Mutex::new(Some(connected_tx)),
        });

        let pc = PeerConnectionBuilder::new()
            .with_configuration(RTCConfigurationBuilder::new().build())
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_handler(handler)
            .with_udp_addrs(vec![format!("0.0.0.0:{MEDIA_UDP_PORT}")])
            .build()
            .await
            .with_context(|| format!("binding UDP {MEDIA_UDP_PORT} (is another rd-host running?)"))?;

        let track = Arc::new(TrackLocalStaticSample::new(
            Instant::now(),
            MediaStreamTrack::new(
                "screen".to_owned(),
                "screen-video".to_owned(),
                "screen".to_owned(),
                RtpCodecKind::Video,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(rand::random::<u32>()),
                        ..Default::default()
                    },
                    codec: codec.rtp_codec.clone(),
                    ..Default::default()
                }],
            ),
        )?);
        let sender = pc.add_track(Arc::clone(&track) as Arc<dyn TrackLocal>).await?;

        pc.set_remote_description(offer).await?;
        let answer = pc.create_answer(None).await?;
        pc.set_local_description(answer).await?;
        let _ = tokio::time::timeout(Duration::from_secs(5), gathered_rx).await;
        let local = pc.local_description().await.context("no local description")?;

        // Start streaming once the viewer is actually connected.
        let media_shared = shared.clone();
        tokio::spawn(async move {
            if connected_rx.await.is_err() {
                return;
            }
            let payload_type = negotiated_payload_type(&sender).await.unwrap_or(H264_PAYLOAD_TYPE);
            let ssrc = match track.ssrcs().await.first() {
                Some(ssrc) => *ssrc,
                None => return eprintln!("video track has no SSRC"),
            };
            let runtime = tokio::runtime::Handle::current();
            std::thread::spawn(move || {
                if let Err(e) = stream_screen(&media_shared, &runtime, &track, ssrc, payload_type) {
                    eprintln!("screen stream stopped: {e:#}");
                }
            });
        });

        Ok((Self { pc: Box::new(pc), shared }, local))
    }

    pub async fn close(self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        let _ = self.pc.close().await;
    }
}

struct Handler {
    shared: Arc<Shared>,
    gathered: Mutex<Option<oneshot::Sender<()>>>,
    connected: Mutex<Option<oneshot::Sender<()>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            if let Some(tx) = self.gathered.lock().unwrap().take() {
                let _ = tx.send(());
            }
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("viewer connection: {state}");
        match state {
            RTCPeerConnectionState::Connected => {
                if let Some(tx) = self.connected.lock().unwrap().take() {
                    let _ = tx.send(());
                }
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                self.shared.stop.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let shared = self.shared.clone();
        tokio::spawn(async move { run_control_channel(dc, shared).await });
    }
}

async fn negotiated_payload_type(sender: &Arc<dyn RtpSender>) -> anyhow::Result<PayloadType> {
    sender
        .get_parameters()
        .await?
        .rtp_parameters
        .codecs
        .first()
        .map(|c| c.payload_type)
        .context("no negotiated codec")
}

/// Capture -> encode -> RTP, paced to MAX_FPS. Runs on its own thread.
fn stream_screen(
    shared: &Shared,
    runtime: &tokio::runtime::Handle,
    track: &TrackLocalStaticSample,
    ssrc: u32,
    payload_type: PayloadType,
) -> anyhow::Result<()> {
    let mut capturer = rd_capture::primary_display()?;
    let mut encoder = H264Encoder::new(EncoderSettings { max_fps: MAX_FPS, ..Default::default() });
    let min_interval = Duration::from_secs_f32(1.0 / MAX_FPS);
    let mut last_sent = Instant::now() - min_interval;
    let mut last_frame = None;

    while !shared.stop.load(Ordering::Relaxed) {
        let frame = match capturer.next_frame_timeout(IDLE_REFRESH)? {
            Some(frame) => {
                if last_sent.elapsed() < min_interval {
                    last_frame = Some(frame);
                    continue;
                }
                last_frame.insert(frame)
            }
            // Static screen: Windows delivers nothing, so resend the current picture as a
            // keyframe. A viewer that lost packets or just joined recovers within a second.
            None => match last_frame.as_ref() {
                Some(frame) => {
                    encoder.force_keyframe();
                    frame
                }
                None => continue,
            },
        };
        *shared.display.lock().unwrap() = (frame.width, frame.height);
        if shared.keyframe.swap(false, Ordering::Relaxed) {
            encoder.force_keyframe();
        }
        let data = encoder.encode(frame)?;
        if data.is_empty() {
            continue;
        }
        let duration = last_sent.elapsed();
        last_sent = Instant::now();
        runtime.block_on(track.sample_writer(ssrc, payload_type).write_sample(&Sample {
            data: Bytes::from(data),
            duration,
            ..Sample::new(Instant::now())
        }))?;
    }
    Ok(())
}

/// Handles the "control" data channel: viewer input in, clipboard and hello out.
async fn run_control_channel(dc: Arc<dyn DataChannel>, shared: Arc<Shared>) {
    let input_tx = spawn_input_thread();
    let (clip_out_tx, mut clip_out_rx) = mpsc::unbounded_channel::<String>();
    let clip_in_tx = spawn_clipboard_thread(shared.clone(), clip_out_tx);

    loop {
        tokio::select! {
            event = dc.poll() => match event {
                Some(DataChannelEvent::OnOpen) => {
                    let (width, height) = *shared.display.lock().unwrap();
                    let hello = HostMsg::Hello { width, height, os: std::env::consts::OS.to_owned() };
                    let _ = dc.send_text(&hello.to_json()).await;
                }
                Some(DataChannelEvent::OnMessage(msg)) => {
                    let Ok(text) = std::str::from_utf8(&msg.data) else { continue };
                    match ViewerMsg::from_json(text) {
                        Ok(ViewerMsg::RequestKeyframe) => shared.keyframe.store(true, Ordering::Relaxed),
                        Ok(ViewerMsg::Clipboard { text }) => { let _ = clip_in_tx.send(text); }
                        Ok(other) => { let _ = input_tx.send(other); }
                        Err(e) => eprintln!("bad control message: {e}"),
                    }
                }
                Some(DataChannelEvent::OnClose) | None => break,
                Some(_) => {}
            },
            Some(text) = clip_out_rx.recv() => {
                let _ = dc.send_text(&HostMsg::Clipboard { text }.to_json()).await;
            }
        }
    }
    shared.stop.store(true, Ordering::Relaxed);
}

fn spawn_input_thread() -> std_mpsc::Sender<ViewerMsg> {
    let (tx, rx) = std_mpsc::channel::<ViewerMsg>();
    std::thread::spawn(move || {
        let mut injector = match rd_input::primary_display() {
            Ok(i) => i,
            Err(e) => return eprintln!("input disabled: {e:#}"),
        };
        let mut warned = false;
        for msg in rx {
            let result = match msg {
                ViewerMsg::MouseMove { at } => injector.mouse_move(at),
                ViewerMsg::MouseButton { button, down, at } => injector.mouse_button(button, down, at),
                ViewerMsg::Wheel { dx, dy } => injector.wheel(dx, dy),
                ViewerMsg::Key { code, down } => injector.key(&code, down),
                _ => Ok(()),
            };
            if let Err(e) = result {
                if !std::mem::replace(&mut warned, true) {
                    eprintln!("input: {e:#}");
                }
            }
        }
    });
    tx
}

/// Mirrors the clipboard: text from the viewer is written here; local changes are
/// sent back. Polling is the only portable way to observe clipboard changes.
fn spawn_clipboard_thread(shared: Arc<Shared>, out: mpsc::UnboundedSender<String>) -> std_mpsc::Sender<String> {
    let (tx, rx) = std_mpsc::channel::<String>();
    std::thread::spawn(move || {
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return eprintln!("clipboard unavailable");
        };
        let mut last = clipboard.get_text().unwrap_or_default();
        while !shared.stop.load(Ordering::Relaxed) {
            if let Ok(text) = rx.recv_timeout(Duration::from_millis(500)) {
                if clipboard.set_text(text.clone()).is_ok() {
                    last = text;
                }
                continue;
            }
            if let Ok(text) = clipboard.get_text() {
                if text != last {
                    last = text.clone();
                    if out.send(text).is_err() {
                        break;
                    }
                }
            }
        }
    });
    tx
}
