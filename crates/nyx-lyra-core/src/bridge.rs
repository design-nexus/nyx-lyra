//! What the bridge script in the page sends. The page is not trusted: a message is a JSON
//! object of a known kind and bounded size, strings are cut to a bounded length, and numbers
//! that aren't finite and non-negative are dropped.

use serde::Deserialize;

pub const MAX_MESSAGE: usize = 16 * 1024;
const MAX_TEXT: usize = 512;
const MAX_ACTIONS: usize = 16;

/// The page's playback as the bridge reads it from the Media Session and the media element.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct State {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub artwork: Option<String>,
    /// The Media Session's playback state: "none", "paused" or "playing".
    pub session: Option<String>,
    pub media: bool,
    pub paused: Option<bool>,
    pub ended: Option<bool>,
    pub position: Option<f64>,
    pub duration: Option<f64>,
    /// 0 to 1.
    pub volume: Option<f64>,
    pub muted: Option<bool>,
    /// The media element's readyState: below 3 it can't play forward yet.
    pub ready: Option<u8>,
    pub error: Option<u16>,
    /// The Media Session actions the page handles ("play", "nexttrack", …).
    pub actions: Vec<String>,
    pub path: Option<String>,
    /// The watch page's song (`?v=`), which the web client keeps in the address.
    pub video: Option<String>,
    /// Whether the page has a signed-in session (only the answer leaves the page).
    pub signed: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Hello { path: String },
    State(State),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejected {
    TooLarge(usize),
    NotJson,
    UnknownKind,
}

pub fn parse(raw: &str) -> Result<Message, Rejected> {
    if raw.len() > MAX_MESSAGE {
        return Err(Rejected::TooLarge(raw.len()));
    }
    let v: serde_json::Value = serde_json::from_str(raw).map_err(|_| Rejected::NotJson)?;
    match v.get("kind").and_then(|k| k.as_str()) {
        Some("hello") => Ok(Message::Hello { path: text(v.get("path").and_then(|p| p.as_str()).unwrap_or_default()) }),
        Some("state") => {
            let mut s: State = serde_json::from_value(v).map_err(|_| Rejected::NotJson)?;
            clean(&mut s);
            Ok(Message::State(s))
        }
        _ => Err(Rejected::UnknownKind),
    }
}

fn text(s: &str) -> String {
    s.chars().take(MAX_TEXT).collect()
}

fn clean(s: &mut State) {
    for t in [&mut s.title, &mut s.artist, &mut s.album, &mut s.session, &mut s.path] {
        *t = t.take().map(|x| text(&x)).filter(|x| !x.is_empty());
    }
    s.artwork = s.artwork.take().filter(|u| u.len() <= 2048 && u.starts_with("https://"));
    s.video = s.video.take().filter(|v| (1..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'));
    for n in [&mut s.position, &mut s.duration] {
        *n = n.filter(|x| x.is_finite() && *x >= 0.0);
    }
    s.volume = s.volume.filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0));
    s.actions.truncate(MAX_ACTIONS);
    s.actions.retain(|a| a.len() <= 32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_is_cleaned() {
        let long = "x".repeat(2000);
        let raw = format!(
            r#"{{"kind":"state","title":"{long}","artist":"","artwork":"javascript:alert(1)","position":-3,"duration":243.2,"volume":7,"paused":false,"media":true,"actions":["play","nexttrack"],"video":"../x"}}"#
        );
        let Ok(Message::State(s)) = parse(&raw) else { panic!() };
        assert_eq!(s.title.unwrap().len(), MAX_TEXT);
        assert_eq!(s.artist, None);
        assert_eq!(s.artwork, None);
        assert_eq!(s.position, None);
        assert_eq!(s.duration, Some(243.2));
        assert_eq!(s.volume, Some(1.0));
        assert_eq!(s.actions, ["play", "nexttrack"]);
        assert_eq!(s.video, None);
    }

    #[test]
    fn rejects() {
        assert_eq!(parse("nope"), Err(Rejected::NotJson));
        assert_eq!(parse(r#"{"kind":"run","code":"rm -rf"}"#), Err(Rejected::UnknownKind));
        assert_eq!(parse(&" ".repeat(MAX_MESSAGE + 1)), Err(Rejected::TooLarge(MAX_MESSAGE + 1)));
        assert_eq!(parse(r#"{"kind":"state","media":"yes"}"#), Err(Rejected::NotJson));
    }
}
