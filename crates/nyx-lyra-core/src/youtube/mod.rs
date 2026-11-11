//! YouTube Music's data, through the web client's own requests (`/youtubei/v1/search`,
//! `browse`, `next`), made by the bridge from inside the signed-in page. These are not a
//! public API: every reply is parsed defensively, and one Lyra can't read is an error the
//! screen shows, never a guess. Lyra never asks for streams.

mod parse;

pub use parse::{Lyrics, Next, ParseError, RESULTS, logged_in, lyrics, next, page};

use serde_json::{Value, json};

pub const HOME: &str = "FEmusic_home";
pub const EXPLORE: &str = "FEmusic_explore";
pub const NEW_RELEASES: &str = "FEmusic_new_releases";
pub const CHARTS: &str = "FEmusic_charts";
pub const MOODS: &str = "FEmusic_moods_and_genres";
pub const LIBRARY_PLAYLISTS: &str = "FEmusic_liked_playlists";
pub const LIBRARY_ALBUMS: &str = "FEmusic_liked_albums";
pub const LIBRARY_ARTISTS: &str = "FEmusic_library_corpus_track_artists";
pub const LIBRARY_SONGS: &str = "FEmusic_liked_videos";
pub const LIKED_MUSIC: &str = "VLLM";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    Search,
    Browse,
    Next,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub endpoint: Endpoint,
    pub body: Value,
}

impl Request {
    /// What the bridge's `__lyraData` takes.
    pub fn wire(&self) -> Value {
        let endpoint = match self.endpoint {
            Endpoint::Search => "search",
            Endpoint::Browse => "browse",
            Endpoint::Next => "next",
        };
        json!({ "endpoint": endpoint, "body": self.body })
    }
}

/// A YouTube ID as the requests and page addresses take it: letters, digits, `_` and `-`.
pub fn valid_id(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn search(query: &str) -> Option<Request> {
    let q = query.trim();
    (!q.is_empty() && q.chars().count() <= 200).then(|| Request { endpoint: Endpoint::Search, body: json!({ "query": q }) })
}

/// A page by its browse ID: one of the constants above, or an album's, artist's or
/// playlist's from an [`crate::model::Item`].
pub fn browse(id: &str) -> Option<Request> {
    valid_id(id).then(|| Request { endpoint: Endpoint::Browse, body: json!({ "browseId": id }) })
}

/// The next part of a long list (a playlist's songs, the library), from
/// [`crate::model::Page::continuation`].
pub fn more(token: &str) -> Option<Request> {
    let ok = (1..=4096).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-%=".contains(&b));
    ok.then(|| Request { endpoint: Endpoint::Browse, body: json!({ "continuation": token }) })
}

/// What plays after a song: its queue, and the IDs of its lyrics and related pages.
pub fn up_next(video_id: &str, playlist_id: Option<&str>) -> Option<Request> {
    if !valid_id(video_id) || playlist_id.is_some_and(|p| !valid_id(p)) {
        return None;
    }
    let mut body = json!({ "videoId": video_id, "isAudioOnly": true });
    if let Some(p) = playlist_id {
        body["playlistId"] = p.into();
    }
    Some(Request { endpoint: Endpoint::Next, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_checked() {
        assert!(search("  ").is_none());
        assert!(search(&"a".repeat(201)).is_none());
        assert_eq!(search(" daft punk ").unwrap().body["query"], "daft punk");
        assert!(browse("FEmusic_home").is_some());
        assert!(browse("../player").is_none());
        assert!(browse("").is_none());
        assert!(up_next("dX3k_QDnzHE", Some("RDAMVM x")).is_none());
        assert_eq!(up_next("dX3k_QDnzHE", None).unwrap().wire()["endpoint"], "next");
    }
}
