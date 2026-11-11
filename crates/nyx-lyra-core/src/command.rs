//! Commands to the web player and the coordinator that follows them. A command succeeds only
//! when the page's reported state shows it happened; the bridge accepting it isn't enough.

use crate::bridge;
use crate::playback::{Connection, Playback, Status};
use serde::Serialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Seek { position: f64 },
    Volume { level: u8 },
}

impl Command {
    /// What the bridge's `__lyraCommand` takes.
    pub fn wire(&self) -> Value {
        match self {
            Command::Play => json!({ "cmd": "play" }),
            Command::Pause => json!({ "cmd": "pause" }),
            Command::Toggle => json!({ "cmd": "toggle" }),
            Command::Next => json!({ "cmd": "next" }),
            Command::Previous => json!({ "cmd": "previous" }),
            Command::Seek { position } => json!({ "cmd": "seek", "position": position }),
            Command::Volume { level } => json!({ "cmd": "volume", "level": level }),
        }
    }

    pub fn timeout(&self) -> Duration {
        match self {
            // A new song has to load.
            Command::Next | Command::Previous => Duration::from_secs(12),
            Command::Play | Command::Toggle => Duration::from_secs(10),
            _ => Duration::from_secs(5),
        }
    }

    /// Whether `now` shows the command took effect, given the state when it was sent.
    fn done(&self, before: &Playback, now: &Playback) -> bool {
        let playing = now.is_playing() && now.error.is_none();
        match self {
            Command::Play => playing,
            Command::Pause => now.status == Status::Paused,
            Command::Toggle => playing != before.is_playing(),
            Command::Next => now.title().is_some() && now.title() != before.title(),
            // Previous may restart the song rather than change it.
            Command::Previous => {
                (now.title().is_some() && now.title() != before.title())
                    || (now.position.is_some_and(|p| p < 3.0) && before.position.is_some_and(|p| p >= 3.0))
            }
            Command::Seek { position } => now.position.is_some_and(|p| (p - position).abs() < 2.0),
            Command::Volume { level } => now.volume == Some(*level),
        }
    }

    fn check(&self, now: &Playback) -> Result<(), Failure> {
        match self {
            Command::Seek { position } if !position.is_finite() || *position < 0.0 => Err(Failure::BadArgument),
            Command::Volume { level } if *level > 100 => Err(Failure::BadArgument),
            Command::Next if !now.controls.next => Err(Failure::Unsupported),
            Command::Previous if !now.controls.previous => Err(Failure::Unsupported),
            Command::Seek { .. } if !now.controls.seek => Err(Failure::Unsupported),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// YouTube Music doesn't offer it here (no next song, a live stream that can't seek).
    Unsupported,
    BadArgument,
    /// Nothing is loaded to act on.
    NoMedia,
    /// The page took it but nothing changed in time.
    TimedOut,
    /// The web player isn't there (crashed, reloading).
    Disconnected,
    /// The bridge answered something Lyra doesn't know.
    Refused(String),
}

pub type CommandId = u64;

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    State(Playback),
    Confirmed { id: CommandId, command: Command },
    Failed { id: CommandId, command: Command, failure: Failure },
}

struct Pending {
    id: CommandId,
    command: Command,
    before: Playback,
    deadline: Instant,
}

/// Holds the state and the commands waiting for confirmation. Times are passed in so the
/// caller's clock (and tests) decide.
#[derive(Default)]
pub struct Coordinator {
    state: Playback,
    pending: Vec<Pending>,
    next: CommandId,
}

pub enum Send {
    /// Hand this to the bridge, then report its answer with [`Coordinator::answered`].
    Wire(CommandId, Value),
    /// Failed before reaching the page.
    Failed(Event),
}

impl Coordinator {
    pub fn state(&self) -> &Playback {
        &self.state
    }

    /// The ID the next [`Coordinator::send`] will use.
    pub fn next_id(&self) -> CommandId {
        self.next + 1
    }

    pub fn send(&mut self, command: Command, now: Instant) -> Send {
        self.next += 1;
        let id = self.next;
        let failure =
            if self.state.connection != Connection::Ready { Err(Failure::Disconnected) } else { command.check(&self.state) };
        if let Err(failure) = failure {
            return Send::Failed(Event::Failed { id, command, failure });
        }
        let wire = command.wire();
        let deadline = now + command.timeout();
        self.pending.push(Pending { id, command, before: self.state.clone(), deadline });
        Send::Wire(id, wire)
    }

    /// The bridge's immediate answer: "session" or "element" mean it was passed on.
    pub fn answered(&mut self, id: CommandId, answer: &str) -> Option<Event> {
        let failure = match answer {
            "session" | "element" => return None,
            "unsupported" => Failure::Unsupported,
            "no-media" => Failure::NoMedia,
            "bad-argument" => Failure::BadArgument,
            "no-bridge" => Failure::Disconnected,
            other => Failure::Refused(other.chars().take(64).collect()),
        };
        self.take(id).map(|p| Event::Failed { id, command: p.command, failure })
    }

    pub fn report(&mut self, s: &bridge::State) -> Vec<Event> {
        let next = Playback::from_bridge(s, self.state.connection);
        let mut events = Vec::new();
        let (done, waiting): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.pending).into_iter().partition(|p| p.command.done(&p.before, &next));
        self.pending = waiting;
        if next != self.state {
            self.state = next;
            events.push(Event::State(self.state.clone()));
        }
        events.extend(done.into_iter().map(|p| Event::Confirmed { id: p.id, command: p.command }));
        events
    }

    /// Fails what has waited too long.
    pub fn tick(&mut self, now: Instant) -> Vec<Event> {
        let (late, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending).into_iter().partition(|p| now >= p.deadline);
        self.pending = waiting;
        late.into_iter().map(|p| Event::Failed { id: p.id, command: p.command, failure: Failure::TimedOut }).collect()
    }

    /// The web player came, went or is coming back. Leaving Ready fails every waiting command.
    pub fn connection(&mut self, c: Connection) -> Vec<Event> {
        if self.state.connection == c {
            return Vec::new();
        }
        let mut events = Vec::new();
        if c != Connection::Ready {
            events.extend(std::mem::take(&mut self.pending).into_iter().map(|p| Event::Failed {
                id: p.id,
                command: p.command,
                failure: Failure::Disconnected,
            }));
        }
        self.state.connection = c;
        events.insert(0, Event::State(self.state.clone()));
        events
    }

    fn take(&mut self, id: CommandId) -> Option<Pending> {
        let i = self.pending.iter().position(|p| p.id == id)?;
        Some(self.pending.remove(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(title: &str, paused: bool, position: f64) -> bridge::State {
        bridge::State {
            title: Some(title.into()),
            artist: Some("M83".into()),
            media: true,
            paused: Some(paused),
            ready: Some(4),
            position: Some(position),
            duration: Some(243.0),
            volume: Some(1.0),
            actions: ["play", "pause", "nexttrack", "previoustrack", "seekto"].map(String::from).to_vec(),
            ..Default::default()
        }
    }

    fn ready() -> (Coordinator, Instant) {
        let mut c = Coordinator::default();
        c.connection(Connection::Ready);
        c.report(&s("Midnight City", false, 10.0));
        (c, Instant::now())
    }

    fn wire(send: Send) -> CommandId {
        match send {
            Send::Wire(id, _) => id,
            Send::Failed(e) => panic!("{e:?}"),
        }
    }

    #[test]
    fn confirmed_only_by_state() {
        let (mut c, t) = ready();
        let id = wire(c.send(Command::Pause, t));
        assert_eq!(c.answered(id, "session"), None);
        assert!(c.report(&s("Midnight City", false, 11.0)).iter().all(|e| !matches!(e, Event::Confirmed { .. })));
        let events = c.report(&s("Midnight City", true, 11.0));
        assert!(events.contains(&Event::Confirmed { id, command: Command::Pause }));
    }

    #[test]
    fn next_needs_a_new_song() {
        let (mut c, t) = ready();
        let id = wire(c.send(Command::Next, t));
        assert!(!c.report(&s("Midnight City", false, 12.0)).iter().any(|e| matches!(e, Event::Confirmed { .. })));
        assert!(c.report(&s("Reunion", false, 0.5)).contains(&Event::Confirmed { id, command: Command::Next }));
    }

    #[test]
    fn previous_may_restart() {
        let (mut c, t) = ready();
        let id = wire(c.send(Command::Previous, t));
        assert!(c.report(&s("Midnight City", false, 0.4)).contains(&Event::Confirmed { id, command: Command::Previous }));
    }

    #[test]
    fn failures() {
        let (mut c, t) = ready();
        let id = wire(c.send(Command::Seek { position: 100.0 }, t));
        assert_eq!(
            c.tick(t + Duration::from_secs(6)),
            vec![Event::Failed { id, command: Command::Seek { position: 100.0 }, failure: Failure::TimedOut }]
        );
        let id = wire(c.send(Command::Next, t));
        assert_eq!(
            c.answered(id, "unsupported"),
            Some(Event::Failed { id, command: Command::Next, failure: Failure::Unsupported })
        );
        assert!(matches!(
            c.send(Command::Volume { level: 120 }, t),
            Send::Failed(Event::Failed { failure: Failure::BadArgument, .. })
        ));
        let id = wire(c.send(Command::Play, t));
        let events = c.connection(Connection::Reconnecting);
        assert!(events.contains(&Event::Failed { id, command: Command::Play, failure: Failure::Disconnected }));
        assert!(matches!(c.send(Command::Play, t), Send::Failed(Event::Failed { failure: Failure::Disconnected, .. })));
    }

    #[test]
    fn unsupported_before_sending() {
        let (mut c, t) = ready();
        let mut st = s("Live", false, 1.0);
        st.actions = vec!["play".into(), "pause".into()];
        c.report(&st);
        assert!(matches!(c.send(Command::Next, t), Send::Failed(Event::Failed { failure: Failure::Unsupported, .. })));
    }
}
