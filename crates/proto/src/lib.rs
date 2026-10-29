//! Messages exchanged over the control data channel between viewer and host.
//! JSON-encoded so the browser viewer can speak it without extra tooling.

use serde::{Deserialize, Serialize};

/// Pointer coordinates are normalized to 0.0..=1.0 of the host display, so the
/// viewer never needs to know the host's resolution or DPI scaling.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ViewerMsg {
    MouseMove { at: Point },
    MouseButton { button: MouseButton, down: bool, at: Point },
    /// Scroll in lines; positive `dy` scrolls content down.
    Wheel { dx: f32, dy: f32 },
    /// `code` is a W3C KeyboardEvent.code ("KeyA", "Enter", "ShiftLeft"), which is
    /// layout-independent and identical in browsers and our native viewer.
    Key { code: String, down: bool },
    Clipboard { text: String },
    RequestKeyframe,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum HostMsg {
    Hello { width: u32, height: u32, os: String },
    Clipboard { text: String },
    Stats { fps: f32, encode_ms: f32, bitrate_kbps: u32 },
}

impl ViewerMsg {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("ViewerMsg always serializes")
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

impl HostMsg {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("HostMsg always serializes")
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_messages_round_trip() {
        let msgs = [
            ViewerMsg::MouseMove { at: Point { x: 0.5, y: 0.25 } },
            ViewerMsg::MouseButton { button: MouseButton::Right, down: true, at: Point { x: 0.0, y: 1.0 } },
            ViewerMsg::Wheel { dx: 0.0, dy: -3.0 },
            ViewerMsg::Key { code: "KeyA".into(), down: false },
            ViewerMsg::Clipboard { text: "héllo".into() },
            ViewerMsg::RequestKeyframe,
        ];
        for m in msgs {
            assert_eq!(ViewerMsg::from_json(&m.to_json()).unwrap(), m);
        }
    }

    #[test]
    fn wire_format_is_browser_friendly() {
        let json = ViewerMsg::Key { code: "Enter".into(), down: true }.to_json();
        assert_eq!(json, r#"{"t":"key","code":"Enter","down":true}"#);
        let parsed = HostMsg::from_json(r#"{"t":"hello","width":1920,"height":1080,"os":"windows"}"#).unwrap();
        assert_eq!(parsed, HostMsg::Hello { width: 1920, height: 1080, os: "windows".into() });
    }
}
