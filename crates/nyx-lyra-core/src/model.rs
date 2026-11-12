//! Songs, albums, artists and playlists as Lyra shows them.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Song,
    Video,
    Album,
    Single,
    Ep,
    Artist,
    Playlist,
    /// A page to open: Explore's New releases and Charts, a mood or genre.
    Link,
    /// Podcasts and anything else YouTube Music mixes in; Lyra leaves them to Nyx Music.
    #[default]
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thumbnail {
    pub url: String,
    pub width: u32,
    pub height: u32,
}

/// A name that may lead somewhere (an artist's or album's page).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub name: String,
    pub browse_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    /// A link's page options (a mood's or a chart's).
    #[serde(default)]
    pub params: Option<String>,
    /// A mood's colour stripe, as 0xRRGGBB.
    #[serde(default)]
    pub color: Option<u32>,
    /// A chart's place ("1") and how it moved: 1 up, -1 down, 0 the same.
    #[serde(default)]
    pub rank: Option<String>,
    #[serde(default)]
    pub trend: Option<i8>,
    /// A top result's Shuffle and Radio (YouTube Music's Mix): the lists they play.
    #[serde(default)]
    pub shuffle: Option<String>,
    #[serde(default)]
    pub radio: Option<String>,
    /// The song is liked, when YouTube Music's row says.
    #[serde(default)]
    pub liked: Option<bool>,
}

impl Item {
    /// The smallest thumbnail at least `size` pixels wide, else the largest.
    pub fn thumbnail(&self, size: u32) -> Option<&Thumbnail> {
        self.thumbnails
            .iter()
            .filter(|t| t.width >= size)
            .min_by_key(|t| t.width)
            .or_else(|| self.thumbnails.iter().max_by_key(|t| t.width))
    }

    pub fn playable(&self) -> bool {
        self.video_id.is_some() || self.playlist_id.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shelf {
    pub title: String,
    pub items: Vec<Item>,
    /// The page with the whole shelf, when it has one.
    pub more: Option<String>,
    /// YouTube Music shows it as square tiles (not rows of songs).
    #[serde(default)]
    pub tiles: bool,
}

/// A choice a page offers: Home's moods, the library's sort orders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chip {
    pub title: String,
    /// The page's browse ID and the option's parameters; `None` for the page without it.
    pub browse_id: Option<String>,
    pub params: Option<String>,
    pub selected: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub title: String,
    pub subtitle: String,
    pub second_subtitle: String,
    pub description: String,
    pub thumbnails: Vec<Thumbnail>,
    /// An artist's page: its Shuffle and Radio lists, and its channel with whether the
    /// account subscribes to it.
    #[serde(default)]
    pub artist: Option<ArtistActions>,
    /// An album's or playlist's page: its lists, whether it's in the library, its maker.
    #[serde(default)]
    pub list: Option<ListActions>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ListActions {
    /// The list Play plays, and the one Shuffle plays.
    pub play: Option<String>,
    pub shuffle: Option<String>,
    /// No shuffled list of its own: Shuffle plays `play` and shuffles it (a playlist of yours).
    #[serde(default)]
    pub shuffle_play: bool,
    /// Saved in the library (an album, someone else's playlist), and the list saving keeps.
    pub saved: Option<bool>,
    pub save_target: Option<String>,
    /// The artist or the playlist's owner, with their photo.
    pub by: Option<Link>,
    pub by_photo: Vec<Thumbnail>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtistActions {
    pub shuffle: Option<String>,
    pub radio: Option<String>,
    pub channel_id: Option<String>,
    pub subscribed: Option<bool>,
    /// "1.35M subscribers".
    pub subscribers: String,
}

/// A channel the signed-in Google account can use YouTube Music as (itself or a brand account).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    /// "@handle" for the active channel, the subscriber line in the list.
    pub byline: String,
    pub photo: Vec<Thumbnail>,
    pub selected: bool,
    /// YouTube Music's own address that makes this channel the active one
    /// (`/signin?action_handle_signin=true…`); only on the list's entries.
    pub switch: Option<String>,
}

/// A page of YouTube Music: Home, a search, an album, an artist, a playlist, the library.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub header: Option<Header>,
    pub shelves: Vec<Shelf>,
    /// More of the last shelf, fetched with [`crate::youtube::more`].
    #[serde(default)]
    pub continuation: Option<String>,
    /// Home's moods.
    #[serde(default)]
    pub chips: Vec<Chip>,
    /// The library's sort orders.
    #[serde(default)]
    pub sorts: Vec<Chip>,
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
