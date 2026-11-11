//! Lyra's view of what's playing, built only from what the page reports. The web player is
//! the authority: nothing here changes until the page says so.

use crate::bridge;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Stopped,
    Playing,
    Paused,
    /// Asked to play but waiting for data (a slow or lost network).
    Buffering,
}

/// The web player itself, as far as Lyra can tell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    /// No page yet, or it's loading.
    #[default]
    Starting,
    Ready,
    /// The web process ended and Lyra is loading the page again.
    Reconnecting,
    /// It ended too often; Lyra stopped trying until asked.
    Lost,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub artwork: Option<String>,
    pub video_id: Option<String>,
    pub playlist_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Controls {
    pub play: bool,
    pub pause: bool,
    pub next: bool,
    pub previous: bool,
    pub seek: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Playback {
    pub status: Status,
    pub track: Option<Track>,
    /// Seconds.
    pub position: Option<f64>,
    pub duration: Option<f64>,
    /// 0 to 100.
    pub volume: Option<u8>,
    pub muted: bool,
    pub controls: Controls,
    pub connection: Connection,
    /// The media element reported an error (its MediaError code).
    pub error: Option<u16>,
    /// Whether the page has a signed-in YouTube Music session, once it says.
    pub signed_in: Option<bool>,
}

impl Playback {
    /// The state as the latest bridge report has it; the connection is the caller's.
    pub fn from_bridge(s: &bridge::State, connection: Connection) -> Self {
        let has = |a: &str| s.actions.iter().any(|x| x == a);
        let status = match (s.media, s.paused, s.ended) {
            (false, _, _) | (_, None, _) | (_, _, Some(true)) => Status::Stopped,
            (true, Some(true), _) => Status::Paused,
            (true, Some(false), _) if s.ready.is_some_and(|r| r < 3) => Status::Buffering,
            (true, Some(false), _) => Status::Playing,
        };
        let track = s.title.as_ref().map(|t| Track {
            title: t.clone(),
            artist: s.artist.clone().unwrap_or_default(),
            album: s.album.clone(),
            artwork: s.artwork.clone(),
            video_id: s.video.clone(),
            playlist_id: s.list.clone(),
        });
        Self {
            status,
            track,
            position: s.position,
            duration: s.duration.filter(|d| *d > 0.0),
            volume: s.volume.map(|v| (v * 100.0).round() as u8),
            muted: s.muted.unwrap_or(false),
            controls: Controls {
                play: has("play") || s.media,
                pause: has("pause") || s.media,
                next: has("nexttrack"),
                previous: has("previoustrack"),
                seek: (has("seekto") || s.media) && s.duration.is_some_and(|d| d > 0.0),
            },
            connection,
            error: s.error,
            signed_in: s.signed,
        }
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.status, Status::Playing | Status::Buffering)
    }

    pub fn title(&self) -> Option<&str> {
        self.track.as_ref().map(|t| t.title.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(media: bool, paused: Option<bool>, ready: u8) -> bridge::State {
        bridge::State {
            title: Some("Midnight City".into()),
            artist: Some("M83".into()),
            media,
            paused,
            ready: Some(ready),
            duration: Some(243.2),
            volume: Some(0.4),
            actions: vec!["play".into(), "pause".into(), "nexttrack".into()],
            ..Default::default()
        }
    }

    #[test]
    fn status() {
        assert_eq!(Playback::from_bridge(&state(false, None, 0), Connection::Ready).status, Status::Stopped);
        assert_eq!(Playback::from_bridge(&state(true, Some(true), 4), Connection::Ready).status, Status::Paused);
        assert_eq!(Playback::from_bridge(&state(true, Some(false), 2), Connection::Ready).status, Status::Buffering);
        let p = Playback::from_bridge(&state(true, Some(false), 4), Connection::Ready);
        assert_eq!(p.status, Status::Playing);
        assert_eq!(p.volume, Some(40));
        assert!(p.controls.next && !p.controls.previous && p.controls.seek);
        assert_eq!(p.title(), Some("Midnight City"));
    }
}
