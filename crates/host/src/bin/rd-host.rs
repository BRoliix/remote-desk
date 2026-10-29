//! Share this screen. Open http://<this-machine>:47080 on the other device and enter
//! the password printed below.
//!
//!   rd-host [--port 47080] [--password <pw>]

use std::net::SocketAddr;

use anyhow::Context as _;
use rd_host::server::{router, AppState};
use rd_host::session::MEDIA_UDP_PORT;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (port, password) = parse_args()?;
    let state = AppState::new(password.clone());

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .with_context(|| format!("port {port} is busy (is rd-host already running?)"))?;

    let host = hostname();
    println!("remote-desk host is sharing this screen");
    println!();
    println!("  open on the other device:  http://{host}:{port}");
    println!("  password:                  {password}");
    println!();
    println!("  (needs TCP {port} + UDP {MEDIA_UDP_PORT} open; windows/setup.ps1 does this)");
    println!("  Ctrl+C to stop");

    axum::serve(listener, router(state).into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}

fn parse_args() -> anyhow::Result<(u16, String)> {
    let mut port = 47080;
    let mut password = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().context("missing value");
        match arg.as_str() {
            "--port" => port = value()?.parse()?,
            "--password" => password = Some(value()?),
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    // 8 digits: easy to read out loud, ~27 bits, and the server delays wrong guesses.
    let password = password.unwrap_or_else(|| format!("{:08}", rand::random::<u32>() % 100_000_000));
    Ok((port, password))
}

fn hostname() -> String {
    // Windows sets COMPUTERNAME; macOS shells don't export HOSTNAME, so ask the OS.
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| {
            let out = std::process::Command::new("hostname").arg("-s").output().ok()?;
            Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
        })
        .filter(|h| !h.is_empty())
        .map(|h| h.to_lowercase())
        .unwrap_or_else(|| "this-machine".to_owned())
}
