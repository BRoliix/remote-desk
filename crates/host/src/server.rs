//! HTTP + WebSocket signaling for M2: serves the browser viewer and swaps the
//! offer/answer for one session at a time, gated by the session password.
//!
//! Wire format (JSON text frames):
//!   viewer -> host  {"type":"auth","password":"..."}, then {"type":"offer","sdp":"..."}
//!   host -> viewer  {"type":"ready"} | {"type":"answer","sdp":"..."} | {"type":"error","message":"..."}

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use webrtc::peer_connection::RTCSessionDescription;

use crate::session::Session;

const VIEWER_HTML: &str = include_str!("../viewer/index.html");

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Auth { password: String },
    Offer { sdp: String },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMsg<'a> {
    Ready,
    Answer { sdp: &'a str },
    Error { message: &'a str },
}

pub struct AppState {
    pub password: String,
    /// Only one viewer at a time; a new one replaces the old. Tagged with an id so a
    /// replaced viewer's cleanup can't close its successor's session.
    current: Mutex<Option<(u64, Session)>>,
    next_id: AtomicU64,
}

impl AppState {
    pub fn new(password: String) -> Arc<Self> {
        Arc::new(Self { password, current: Mutex::new(None), next_id: AtomicU64::new(1) })
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(|| async { Html(VIEWER_HTML) }))
        .route("/ws", get(ws_upgrade))
        .with_state(state)
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_viewer(socket, state, peer).await {
            eprintln!("viewer {peer}: {e:#}");
        }
    })
}

async fn handle_viewer(mut ws: WebSocket, state: Arc<AppState>, peer: SocketAddr) -> anyhow::Result<()> {
    // 1. Password.
    match next_msg(&mut ws).await? {
        Some(ClientMsg::Auth { password }) if constant_time_eq(&password, &state.password) => {
            send(&mut ws, ServerMsg::Ready).await?;
        }
        _ => {
            eprintln!("viewer {peer}: wrong password");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await; // slow down guessing
            send(&mut ws, ServerMsg::Error { message: "Wrong password" }).await?;
            return Ok(());
        }
    }

    // 2. Offer -> answer.
    let Some(ClientMsg::Offer { sdp }) = next_msg(&mut ws).await? else { return Ok(()) };
    let offer: RTCSessionDescription = serde_json::from_value(serde_json::json!({ "type": "offer", "sdp": sdp }))?;

    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let mut current = state.current.lock().await;
    if let Some((_, old)) = current.take() {
        println!("replacing previous viewer");
        old.close().await;
        // Let the old peer connection release the media UDP port.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    let (session, answer) = match Session::answer(offer).await {
        Ok(ok) => ok,
        Err(e) => {
            send(&mut ws, ServerMsg::Error { message: &format!("{e:#}") }).await?;
            return Err(e);
        }
    };
    *current = Some((id, session));
    drop(current);
    println!("viewer {peer} connected");
    send(&mut ws, ServerMsg::Answer { sdp: &answer.sdp }).await?;

    // 3. The page keeps the socket open; when it goes away, so does the session.
    while let Some(Ok(msg)) = ws.recv().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }
    println!("viewer {peer} left");
    let mut current = state.current.lock().await;
    if current.as_ref().is_some_and(|(cur, _)| *cur == id) {
        if let Some((_, session)) = current.take() {
            session.close().await;
        }
    }
    Ok(())
}

async fn next_msg(ws: &mut WebSocket) -> anyhow::Result<Option<ClientMsg>> {
    while let Some(msg) = ws.recv().await {
        match msg? {
            Message::Text(text) => return Ok(Some(serde_json::from_str(&text)?)),
            Message::Close(_) => return Ok(None),
            _ => {}
        }
    }
    Ok(None)
}

async fn send(ws: &mut WebSocket, msg: ServerMsg<'_>) -> anyhow::Result<()> {
    ws.send(Message::Text(serde_json::to_string(&msg)?.into())).await?;
    Ok(())
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
