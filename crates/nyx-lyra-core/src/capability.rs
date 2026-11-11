//! What Lyra can do with YouTube Music, and why not when it can't. YouTube Music has no
//! public API: playback goes through the page's Media Session, everything else through the
//! web client's own data requests (see [`crate::youtube`]), which can change without notice.

use crate::playback::Playback;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Play,
    Seek,
    Next,
    Previous,
    Volume,
    Metadata,
    Album,
    Shuffle,
    Repeat,
    Like,
    Queue,
    Lyrics,
    Search,
    Home,
    Explore,
    Library,
    LikedMusic,
    Playlists,
    Artists,
    Albums,
    EditPlaylists,
    SignedOutPlayback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "why", rename_all = "snake_case")]
pub enum Availability {
    Available,
    /// It works, through YouTube Music's own data requests, which aren't a public contract.
    Unofficial,
    /// Not built yet in Lyra.
    NotYet,
    Unavailable(&'static str),
}

use Availability::*;
use Capability::*;

/// What Lyra offers, given what the page reports right now.
pub fn availability(c: Capability, now: &Playback) -> Availability {
    let gone = || Unavailable("YouTube Music isn't loaded");
    let ready = now.connection == crate::playback::Connection::Ready;
    match c {
        Play | Volume | Metadata if ready => Available,
        Seek if ready && now.controls.seek => Available,
        Seek if ready => Unavailable("this can't be sought (a live stream, or nothing is loaded)"),
        Next if ready && now.controls.next => Available,
        Next if ready => Unavailable("YouTube Music has no next song here"),
        Previous if ready && now.controls.previous => Available,
        Previous if ready => Unavailable("YouTube Music has no previous song here"),
        Play | Volume | Metadata | Seek | Next | Previous => gone(),
        Album => Unofficial,
        Search | Home | Library | LikedMusic | Playlists | Artists | Albums | Explore => Unofficial,
        Queue | Lyrics => Unofficial,
        Shuffle | Repeat | Like | EditPlaylists => NotYet,
        SignedOutPlayback => Unavailable("YouTube Music asks to sign in before it plays anything"),
    }
}

pub const ALL: &[Capability] = &[
    Play,
    Seek,
    Next,
    Previous,
    Volume,
    Metadata,
    Album,
    Shuffle,
    Repeat,
    Like,
    Queue,
    Lyrics,
    Search,
    Home,
    Explore,
    Library,
    LikedMusic,
    Playlists,
    Artists,
    Albums,
    EditPlaylists,
    SignedOutPlayback,
];
