//! Songs, albums, artists and playlists as Lyra shows them.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Song,
    Video,
    Album,
    Single,
    Ep,
    Artist,
    Playlist,
    /// Podcasts and anything else YouTube Music mixes in; Lyra leaves them to Nyx Music.
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Thumbnail {
    pub url: String,
    pub width: u32,
    pub height: u32,
}

/// A name that may lead somewhere (an artist's or album's page).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Link {
    pub name: String,
    pub browse_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Item {
    pub kind: Kind,
    pub title: String,
    /// The line under the title as YouTube Music writes it ("Song • Daft Punk • 3:45").
    pub subtitle: String,
    pub artists: Vec<Link>,
    pub album: Option<Link>,
    pub duration: Option<u32>,
    pub video_id: Option<String>,
    pub playlist_id: Option<String>,
    pub browse_id: Option<String>,
    pub thumbnails: Vec<Thumbnail>,
    pub explicit: bool,
}

impl Item {
    /// The smallest thumbnail at least `size` pixels wide, else the largest.
    pub fn thumbnail(&self, size: u32) -> Option<&Thumbnail> {
        self.thumbnails.iter().filter(|t| t.width >= size).min_by_key(|t| t.width).or_else(|| self.thumbnails.iter().max_by_key(|t| t.width))
    }

    pub fn playable(&self) -> bool {
        self.video_id.is_some() || self.playlist_id.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Shelf {
    pub title: String,
    pub items: Vec<Item>,
    /// The page with the whole shelf, when it has one.
    pub more: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Header {
    pub title: String,
    pub subtitle: String,
    pub second_subtitle: String,
    pub description: String,
    pub thumbnails: Vec<Thumbnail>,
}

/// A page of YouTube Music: Home, a search, an album, an artist, a playlist, the library.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Page {
    pub header: Option<Header>,
    pub shelves: Vec<Shelf>,
}

impl Page {
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.shelves.iter().flat_map(|s| s.items.iter())
    }
}

/// "3:45" or "1:02:03" in seconds.
pub fn parse_duration(text: &str) -> Option<u32> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    parts.iter().try_fold(0u32, |acc, p| Some(acc * 60 + p.parse::<u32>().ok().filter(|n| *n < 60 || acc == 0)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("3:45"), Some(225));
        assert_eq!(parse_duration("1:02:03"), Some(3723));
        assert_eq!(parse_duration("62:00"), Some(3720));
        assert_eq!(parse_duration("3:75"), None);
        assert_eq!(parse_duration("Song"), None);
        assert_eq!(parse_duration("3"), None);
    }
}
