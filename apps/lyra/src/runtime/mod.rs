//! The web runtime: what plays YouTube Music and answers its data requests. The rest of
//! Lyra uses only this interface, so the engine (WebKitGTK today) can be replaced.

pub mod webkit;

use nyx_lyra_core::bridge;
use nyx_lyra_core::command::CommandId;
use nyx_lyra_core::youtube::Request;
use serde_json::Value;

/// What to show in the page.
#[derive(Clone, Debug, PartialEq)]
pub enum Load {
    Home,
    /// A song, optionally in a playlist or album's list, optionally from a position (seconds).
    Watch {
        video_id: String,
        playlist_id: Option<String>,
        at: Option<u32>,
    },
    /// A playlist or album's list from its start.
    List {
        playlist_id: String,
    },
    /// Google's sign-in page for YouTube Music (where its own Sign in button leads).
    SignIn,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// The bridge started in a new page.
    Hello,
    State(Box<bridge::State>),
    /// The bridge's immediate answer to a command.
    Answer(CommandId, String),
    /// A page message that failed the checks (counted, never acted on).
    Rejected(bridge::Rejected),
    /// The web process ended; `again` says whether the runtime is reloading the page.
    Ended {
        again: bool,
    },
    /// A page failed to load (offline, DNS).
    LoadFailed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum RequestError {
    /// No page to ask from yet, or it isn't YouTube Music's.
    NotReady,
    /// The page has no signed-in session.
    SignedOut,
    Http(u16),
    Failed(String),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RequestError::NotReady => write!(f, "YouTube Music isn't loaded yet"),
            RequestError::SignedOut => write!(f, "Sign in to YouTube Music to see this"),
            RequestError::Http(code) => write!(f, "YouTube Music answered {code}"),
            RequestError::Failed(m) => write!(f, "{m}"),
        }
    }
}

pub type Reply = Box<dyn FnOnce(Result<Value, RequestError>)>;

pub trait Runtime {
    /// Pass a command's wire form to the page; its answer arrives as [`Event::Answer`].
    fn send(&self, id: CommandId, wire: Value);
    /// One of YouTube Music's data requests, answered with the parsed reply.
    fn request(&self, request: &Request, reply: Reply);
    fn load(&self, target: Load);
    /// Mute the page's audio entirely (tests run muted).
    fn set_muted(&self, muted: bool);
    /// The page itself, for signing in and for anything shown as YouTube Music's own page.
    fn widget(&self) -> gtk::Widget;
    /// End the web process, as a crash would (the recovery tests).
    fn terminate(&self);
}
