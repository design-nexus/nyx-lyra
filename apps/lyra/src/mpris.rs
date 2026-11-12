//! MPRIS (org.mpris.MediaPlayer2): media keys, the bar, Control Center and the dock. The
//! D-Bus side runs on its own thread with zbus' blocking API, reads a snapshot the GTK
//! thread keeps current, and sends commands back; every command goes through the player,
//! which only reports what YouTube Music confirms. LoopStatus follows YouTube Music's repeat;
//! Shuffle is an action there (it reorders the queue once), so it reads false and setting it
//! shuffles.

use crate::{APP_ID, art, player};
use gtk::{glib, prelude::*};
use nyx_lyra_core::command::{Command, Event};
use nyx_lyra_core::playback::{Connection, Playback, Repeat, Status};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";

#[derive(Debug, Clone)]
enum Do {
    Raise,
    Quit,
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
    /// Relative, in microseconds.
    Seek(i64),
    /// Absolute, in microseconds, for the given track ID.
    SetPosition(String, i64),
    Volume(f64),
    Loop(Repeat),
    Shuffle,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Meta {
    track_id: String,
    length_us: i64,
    title: String,
    artist: String,
    album: String,
    art_url: String,
    url: String,
}

#[derive(Debug, Clone)]
struct Snapshot {
    status: &'static str,
    volume: f64,
    position_us: i64,
    at: Instant,
    can_next: bool,
    can_previous: bool,
    can_play: bool,
    can_seek: bool,
    /// "None", "Playlist" or "Track".
    loop_status: &'static str,
    meta: Option<Meta>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            status: "Stopped",
            volume: 1.0,
            position_us: 0,
            at: Instant::now(),
            can_next: false,
            can_previous: false,
            can_play: false,
            can_seek: false,
            loop_status: "None",
            meta: None,
        }
    }
}

impl Snapshot {
    fn position(&self) -> i64 {
        if self.status == "Playing" { self.position_us + self.at.elapsed().as_micros() as i64 } else { self.position_us }
    }
}

type Shared = Arc<Mutex<Snapshot>>;

enum Signal {
    Changed(Vec<&'static str>),
    Seeked(i64),
}

/// An object path for a song: letters, digits and `_` only.
fn track_id(video: &str) -> String {
    let safe: String = video.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("/io/github/design_nexus/nyx/Lyra/track/v{safe}")
}

fn snapshot(s: &Playback) -> Snapshot {
    let ready = s.connection == Connection::Ready;
    let meta = s.track.as_ref().map(|t| {
        let video = t.video_id.clone().unwrap_or_default();
        Meta {
            track_id: if video.is_empty() { track_id(&t.title) } else { track_id(&video) },
            length_us: (s.duration.unwrap_or(0.0) * 1e6) as i64,
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone().unwrap_or_default(),
            art_url: t
                .artwork
                .as_deref()
                .and_then(art::file)
                .and_then(|p| glib::filename_to_uri(p, None).ok())
                .map(|u| u.to_string())
                .unwrap_or_default(),
            url: if video.is_empty() { String::new() } else { format!("https://music.youtube.com/watch?v={video}") },
        }
    });
    Snapshot {
        status: match s.status {
            Status::Playing | Status::Buffering => "Playing",
            Status::Paused => "Paused",
            Status::Stopped => "Stopped",
        },
        volume: s.volume.map_or(1.0, |v| v as f64 / 100.0),
        position_us: (s.position.unwrap_or(0.0) * 1e6) as i64,
        at: Instant::now(),
        can_next: ready && s.controls.next,
        can_previous: ready && s.controls.previous,
        can_play: ready && s.track.is_some(),
        can_seek: ready && s.controls.seek,
        loop_status: match s.repeat {
            Some(Repeat::All) => "Playlist",
            Some(Repeat::One) => "Track",
            _ => "None",
        },
        meta,
    }
}

fn value(v: Value<'_>) -> Option<OwnedValue> {
    v.try_to_owned().ok()
}

fn metadata(meta: &Option<Meta>) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    let Some(t) = meta else {
        if let Ok(p) = ObjectPath::try_from("/org/mpris/MediaPlayer2/TrackList/NoTrack")
            && let Some(v) = value(Value::from(p))
        {
            m.insert("mpris:trackid".into(), v);
        }
        return m;
    };
    let mut put = |k: &str, v: Value<'_>| {
        if let Some(v) = value(v) {
            m.insert(k.to_string(), v);
        }
    };
    if let Ok(p) = ObjectPath::try_from(t.track_id.as_str()) {
        put("mpris:trackid", Value::from(p));
    }
    if t.length_us > 0 {
        put("mpris:length", Value::from(t.length_us));
    }
    put("xesam:title", Value::from(t.title.as_str()));
    put("xesam:artist", Value::from(vec![t.artist.as_str()]));
    if !t.album.is_empty() {
        put("xesam:album", Value::from(t.album.as_str()));
    }
    if !t.art_url.is_empty() {
        put("mpris:artUrl", Value::from(t.art_url.as_str()));
    }
    if !t.url.is_empty() {
        put("xesam:url", Value::from(t.url.as_str()));
    }
    m
}

struct Root {
    tx: async_channel::Sender<Do>,
    desktop: String,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        let _ = self.tx.send_blocking(Do::Raise);
    }

    fn quit(&self) {
        let _ = self.tx.send_blocking(Do::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        "Lyra".into()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        self.desktop.clone()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    tx: async_channel::Sender<Do>,
    state: Shared,
}

impl Player {
    fn snap(&self) -> Snapshot {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn send(&self, c: Do) {
        let _ = self.tx.send_blocking(c);
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.send(Do::Next);
    }

    fn previous(&self) {
        self.send(Do::Previous);
    }

    fn pause(&self) {
        self.send(Do::Pause);
    }

    fn play_pause(&self) {
        self.send(Do::PlayPause);
    }

    /// YouTube Music has no stop; Lyra pauses.
    fn stop(&self) {
        self.send(Do::Pause);
    }

    fn play(&self) {
        self.send(Do::Play);
    }

    fn seek(&self, offset: i64) {
        self.send(Do::Seek(offset));
    }

    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        self.send(Do::SetPosition(track_id.to_string(), position));
    }

    fn open_uri(&self, _uri: String) {}

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.snap().status.into()
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn set_rate(&mut self, _value: f64) {}

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.snap().meta)
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.snap().volume
    }

    #[zbus(property)]
    fn set_volume(&mut self, value: f64) {
        self.send(Do::Volume(value));
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        self.snap().loop_status.into()
    }

    #[zbus(property)]
    fn set_loop_status(&mut self, value: String) {
        let mode = match value.as_str() {
            "Playlist" => Repeat::All,
            "Track" => Repeat::One,
            _ => Repeat::Off,
        };
        self.send(Do::Loop(mode));
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn set_shuffle(&mut self, value: bool) {
        if value {
            self.send(Do::Shuffle);
        }
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.snap().position()
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.snap().can_next
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.snap().can_previous
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.snap().can_play
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.snap().can_play
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.snap().can_seek
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

fn serve(
    name: String,
    desktop: String,
    state: Shared,
    tx: async_channel::Sender<Do>,
    signals: mpsc::Receiver<Signal>,
) -> zbus::Result<()> {
    let conn = zbus::blocking::connection::Builder::session()?
        .name(name)?
        .serve_at(PATH, Root { tx: tx.clone(), desktop })?
        .serve_at(PATH, Player { tx, state })?
        .build()?;
    let iface = conn.object_server().interface::<_, Player>(PATH)?;
    for sig in signals {
        let emitter = iface.signal_emitter();
        let guard = iface.get();
        let r = zbus::block_on(async {
            match sig {
                Signal::Seeked(pos) => Player::seeked(emitter, pos).await,
                Signal::Changed(props) => {
                    for p in props {
                        match p {
                            "PlaybackStatus" => guard.playback_status_changed(emitter).await?,
                            "Metadata" => guard.metadata_changed(emitter).await?,
                            "Volume" => guard.volume_changed(emitter).await?,
                            "CanGoNext" => guard.can_go_next_changed(emitter).await?,
                            "CanGoPrevious" => guard.can_go_previous_changed(emitter).await?,
                            "CanPlay" => {
                                guard.can_play_changed(emitter).await?;
                                guard.can_pause_changed(emitter).await?
                            }
                            "CanSeek" => guard.can_seek_changed(emitter).await?,
                            "LoopStatus" => guard.loop_status_changed(emitter).await?,
                            _ => {}
                        }
                    }
                    Ok(())
                }
            }
        });
        drop(guard);
        if let Err(e) = r {
            eprintln!("nyx-lyra: mpris: {e}");
        }
    }
    Ok(())
}

fn handle(app: &gtk::Application, c: Do) {
    let Some(p) = player() else { return };
    let s = p.state();
    match c {
        Do::Raise => app.activate(),
        Do::Quit => app.quit(),
        Do::Play => {
            p.send(Command::Play);
        }
        Do::Pause => {
            p.send(Command::Pause);
        }
        Do::PlayPause => {
            p.send(Command::Toggle);
        }
        Do::Next => {
            p.send(Command::Next);
        }
        Do::Previous => {
            p.send(Command::Previous);
        }
        Do::Seek(us) => {
            let to = (s.position.unwrap_or(0.0) + us as f64 / 1e6).max(0.0);
            let to = s.duration.map_or(to, |d| to.min(d));
            p.send(Command::Seek { position: to });
        }
        Do::SetPosition(id, us) => {
            let current = s.track.as_ref().and_then(|t| t.video_id.as_deref()).map(track_id);
            if current.as_deref() == Some(id.as_str()) && us >= 0 {
                p.send(Command::Seek { position: us as f64 / 1e6 });
            }
        }
        Do::Volume(v) => {
            p.send(Command::Volume { level: (v.clamp(0.0, 1.0) * 100.0).round() as u8 });
        }
        Do::Loop(mode) => {
            p.send(Command::Repeat { mode });
        }
        Do::Shuffle => {
            p.send(Command::Shuffle);
        }
    }
}

/// Register on the session bus as `org.mpris.MediaPlayer2.lyra` (a test copy under another
/// application ID gets `….lyra.test`, so it never takes the real one's name). Without a
/// session bus Lyra works on, without MPRIS.
pub fn start(app: &gtk::Application) {
    let Some(p) = player() else { return };
    let id = app.application_id().map(|s| s.to_string()).unwrap_or_else(|| APP_ID.into());
    let name =
        if id == APP_ID { "org.mpris.MediaPlayer2.lyra".to_string() } else { "org.mpris.MediaPlayer2.lyra.test".to_string() };
    let shared: Shared = Arc::new(Mutex::new(snapshot(&p.state())));
    let (cmd_tx, cmd_rx) = async_channel::unbounded::<Do>();
    let (sig_tx, sig_rx) = mpsc::channel::<Signal>();
    {
        let shared = shared.clone();
        std::thread::Builder::new()
            .name("mpris".into())
            .spawn(move || {
                if let Err(e) = serve(name, id, shared, cmd_tx, sig_rx) {
                    eprintln!("nyx-lyra: MPRIS unavailable: {e}");
                }
            })
            .ok();
    }
    let app = app.clone();
    glib::spawn_future_local(async move {
        while let Ok(c) = cmd_rx.recv().await {
            handle(&app, c);
        }
    });

    let last = std::rc::Rc::new(std::cell::RefCell::new(snapshot(&p.state())));
    let update = std::rc::Rc::new(move |s: &Playback, seeked: bool| {
        let now = snapshot(s);
        let mut changed = Vec::new();
        {
            let prev = last.borrow();
            if prev.status != now.status {
                changed.push("PlaybackStatus");
            }
            if (prev.volume - now.volume).abs() > 1e-6 {
                changed.push("Volume");
            }
            if prev.can_next != now.can_next {
                changed.push("CanGoNext");
            }
            if prev.can_previous != now.can_previous {
                changed.push("CanGoPrevious");
            }
            if prev.can_play != now.can_play {
                changed.push("CanPlay");
            }
            if prev.can_seek != now.can_seek {
                changed.push("CanSeek");
            }
            if prev.meta != now.meta {
                changed.push("Metadata");
            }
            if prev.loop_status != now.loop_status {
                changed.push("LoopStatus");
            }
        }
        // A jump the clock didn't account for is a seek.
        let jumped = seeked
            || (now.status == "Playing"
                && (now.position_us - last.borrow().position()).abs() > 3_000_000
                && last.borrow().meta == now.meta);
        if let Ok(mut s) = shared.lock() {
            *s = now.clone();
        }
        let pos = now.position_us;
        *last.borrow_mut() = now;
        if !changed.is_empty() {
            let _ = sig_tx.send(Signal::Changed(changed));
        }
        if jumped {
            let _ = sig_tx.send(Signal::Seeked(pos));
        }
    });
    let wanted_art = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
    let u = update.clone();
    p.connect(move |e| {
        let seeked = matches!(e, Event::Confirmed { command: Command::Seek { .. }, .. });
        let Some(p) = player() else { return };
        let s = p.state();
        if !matches!(e, Event::State(_)) && !seeked {
            return;
        }
        // Covers must be files: fetch a new song's, then publish it.
        if let Some(url) = s.track.as_ref().and_then(|t| t.artwork.clone())
            && art::file(&url).is_none()
            && wanted_art.borrow().as_deref() != Some(url.as_str())
        {
            *wanted_art.borrow_mut() = Some(url.clone());
            let u = u.clone();
            art::load(&url, move |_| {
                if let Some(p) = player() {
                    u(&p.state(), false);
                }
            });
        }
        u(&s, seeked);
    });
}
