//! Lyra's core: what the app knows and does, independent of GTK and WebKit.
//!
//! - [`bridge`]: the messages the page's bridge script sends, checked as untrusted input.
//! - [`playback`]: the normalized playback state built from them.
//! - [`command`]: typed commands, and the coordinator that confirms or fails each one.
//! - [`capability`]: what Lyra can and can't do with YouTube Music.
//! - [`youtube`]: YouTube Music's data requests and the parsers for their replies.
//! - [`model`]: songs, albums, artists, playlists and the shelves that hold them.

pub mod bridge;
pub mod capability;
pub mod command;
pub mod model;
pub mod playback;
pub mod youtube;
