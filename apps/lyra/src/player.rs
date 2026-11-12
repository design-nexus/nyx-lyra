//! The playback service: nyx-lyra-core's coordinator fed by the runtime, with timeouts,
//! recovery when the web process ends or a page fails to load, and listeners (the window,
//! MPRIS, the nyx command). The page is the authority; Lyra keeps no queue of its own.

use crate::runtime::{self, Load, Reply, Runtime};
use gtk::glib;
use nyx_lyra_core::command::{Command, CommandId, Coordinator, Event, Failure, Send};
use nyx_lyra_core::model::Item;
use nyx_lyra_core::playback::{Connection, Playback, Status};
use nyx_lyra_core::youtube::Request;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Where to pick up after the web process comes back.
#[derive(Clone, Debug)]
struct Resume {
    video_id: String,
    at: u32,
    playing: bool,
}

type Waiter = Box<dyn FnOnce(Result<Command, Failure>)>;

/// How long a request waits for the page to load (a cold start takes a few seconds).
const REQUEST_WAIT: Duration = Duration::from_secs(30);

type Listener = Rc<dyn Fn(&Event)>;

pub struct Player {
    runtime: RefCell<Option<Rc<dyn Runtime>>>,
    /// Who waits for a command's outcome (the actions answer only once the page confirms).
    waiters: RefCell<std::collections::HashMap<CommandId, Waiter>>,
    coord: RefCell<Coordinator>,
    listeners: RefCell<Vec<Listener>>,
    last: RefCell<Option<Resume>>,
    resume: RefCell<Option<Resume>>,
    retries: Cell<u32>,
    rejected: Cell<u64>,
    /// Lyra asked to play (a song chosen, Play, a resume) and hasn't seen it play yet.
    want_play: Cell<bool>,
    /// Plays re-sent for this song after a media error, and the song.
    replays: RefCell<(Option<String>, u32)>,
    /// Where to seek once the song plays again after a recovery (seeking earlier fails).
    seek_after_play: Cell<Option<u32>>,
    /// The song Lyra was asked to play (or a recovery brought back), until it plays: if the
    /// page skips it (its stream kept failing), Lyra loads it once more.
    recovering: RefCell<Option<(String, bool)>>,
    /// Requests made while the page was loading, with when they were first asked: sent once
    /// the new page's bridge reports.
    pending: RefCell<Vec<(Request, Reply, Instant)>>,
    /// Counts the pages Lyra loads: a request cut off by a newer page is asked again there.
    page: Cell<u64>,
    /// Lyra loaded a page whose bridge hasn't reported yet (`hello` once it started).
    loading: Cell<bool>,
    hello: Cell<bool>,
    /// Called once when the page Lyra loaded next is ready.
    on_ready: RefCell<Vec<Box<dyn FnOnce()>>>,
}

/// In WebKitGTK 2.54, the first song a new web process plays often fails: the audio branch
/// of WebKit's GStreamer pipeline rejects an event while it's being set up, the page reports
/// MediaError 4 and gives up after a few quick tries, and a play a few seconds later works.
/// Lyra asks again after these delays (see docs/phase2-core.md).
const REPLAY_DELAYS_MS: &[u64] = &[500, 1500, 3000, 5000];

impl Player {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            runtime: RefCell::default(),
            waiters: RefCell::default(),
            coord: RefCell::default(),
            listeners: RefCell::default(),
            last: RefCell::default(),
            resume: RefCell::default(),
            retries: Cell::new(0),
            rejected: Cell::new(0),
            want_play: Cell::new(false),
            replays: RefCell::default(),
            seek_after_play: Cell::new(None),
            recovering: RefCell::default(),
            pending: RefCell::default(),
            page: Cell::new(0),
            loading: Cell::new(false),
            hello: Cell::new(false),
            on_ready: RefCell::default(),
        })
    }

    pub fn attach(self: &Rc<Self>, rt: Rc<dyn Runtime>) {
        self.load_page(&rt, Load::Home);
        *self.runtime.borrow_mut() = Some(rt);
        let me = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(p) = me.upgrade() else { return glib::ControlFlow::Break };
            let events = p.coord.borrow_mut().tick(Instant::now());
            p.emit(events);
            p.expire_pending();
            glib::ControlFlow::Continue
        });
    }

    fn runtime(&self) -> Option<Rc<dyn Runtime>> {
        self.runtime.borrow().clone()
    }

    /// Every page Lyra loads goes through here, so requests wait for it.
    fn load_page(&self, rt: &Rc<dyn Runtime>, target: Load) {
        self.page.set(self.page.get() + 1);
        self.loading.set(true);
        self.hello.set(false);
        rt.load(target);
    }

    pub fn connect(&self, f: impl Fn(&Event) + 'static) {
        self.listeners.borrow_mut().push(Rc::new(f));
    }

    fn emit(&self, events: Vec<Event>) {
        let listeners = self.listeners.borrow().clone();
        for e in &events {
            for l in &listeners {
                l(e);
            }
            let outcome = match e {
                Event::Confirmed { id, command } => Some((*id, Ok(command.clone()))),
                Event::Failed { id, failure, .. } => Some((*id, Err(failure.clone()))),
                Event::State(_) => None,
            };
            if let Some((id, outcome)) = outcome {
                let waiter = self.waiters.borrow_mut().remove(&id);
                if let Some(w) = waiter {
                    w(outcome);
                }
            }
        }
    }

    /// Send a command and hear how it ended: confirmed by the page, or why not.
    pub fn send_then(&self, command: Command, done: impl FnOnce(Result<Command, Failure>) + 'static) {
        // Registered before sending: a command can fail at once.
        let id = self.coord.borrow().next_id();
        self.waiters.borrow_mut().insert(id, Box::new(done));
        let sent = self.send(command);
        debug_assert_eq!(sent, id);
    }

    pub fn state(&self) -> Playback {
        self.coord.borrow().state().clone()
    }

    /// Send a command; its confirmation or failure arrives as an [`Event`] with this ID.
    pub fn send(&self, command: Command) -> CommandId {
        match command {
            Command::Play => self.want_play.set(true),
            Command::Toggle => self.want_play.set(!self.state().is_playing()),
            Command::Pause => self.want_play.set(false),
            // Moving on by hand ends any wait for a song Lyra was asked to play.
            Command::Next | Command::Previous => *self.recovering.borrow_mut() = None,
            _ => {}
        }
        let sent = self.coord.borrow_mut().send(command, Instant::now());
        match sent {
            Send::Wire(id, wire) => {
                match self.runtime() {
                    Some(rt) => rt.send(id, wire),
                    None => self.emit(self.coord.borrow_mut().answered(id, "no-bridge").into_iter().collect()),
                }
                id
            }
            Send::Failed(e) => {
                let id = match &e {
                    Event::Failed { id, .. } => *id,
                    _ => 0,
                };
                self.emit(vec![e]);
                id
            }
        }
    }

    /// Start playing a song, album or playlist from Lyra's own screens.
    #[allow(dead_code)] // the screens come in Phase 4
    pub fn play(self: &Rc<Self>, item: &Item) -> bool {
        self.play_ids(item.video_id.clone(), item.playlist_id.clone())
    }

    pub fn play_ids(self: &Rc<Self>, video_id: Option<String>, playlist_id: Option<String>) -> bool {
        // A list alone gives the page nothing to start: find its first song, as YouTube
        // Music's own Play does, and play that in the list.
        if let (None, Some(list)) = (&video_id, &playlist_id) {
            let Some(req) = nyx_lyra_core::youtube::list_start(list) else { return false };
            let me = Rc::downgrade(self);
            let list = list.clone();
            self.request(
                &req,
                Box::new(move |r| {
                    let Some(p) = me.upgrade() else { return };
                    let first = r
                        .ok()
                        .and_then(|v| nyx_lyra_core::youtube::next(&v).ok())
                        .and_then(|n| n.queue.into_iter().find_map(|i| i.video_id));
                    match first {
                        Some(v) => {
                            p.play_ids(Some(v), Some(list));
                        }
                        None => eprintln!("nyx-lyra: couldn't find the first song of {list}"),
                    }
                }),
            );
            return true;
        }
        // The page may skip a song whose stream keeps failing; Lyra loads it once more then.
        *self.recovering.borrow_mut() = video_id.clone().map(|v| (v, false));
        *self.replays.borrow_mut() = (None, 0);
        let target = match (video_id, playlist_id) {
            (Some(video_id), playlist_id) => Load::Watch { video_id, playlist_id, at: None },
            (None, Some(playlist_id)) => Load::List { playlist_id },
            (None, None) => return false,
        };
        match self.runtime() {
            Some(rt) => {
                self.want_play.set(true);
                self.load_page(&rt, target);
                true
            }
            None => false,
        }
    }

    /// Re-send Play when a song Lyra means to play stops on a media error.
    fn replay_if_failed(self: &Rc<Self>, now: &Playback) {
        if !self.want_play.get() {
            return;
        }
        if now.status == Status::Playing && now.error.is_none() && now.position.is_some_and(|p| p > 0.5) {
            let playing = now.track.as_ref().and_then(|t| t.video_id.clone());
            let wanted = self.recovering.borrow().clone();
            if let Some((video_id, retried)) = wanted
                && playing.as_deref() != Some(video_id.as_str())
                && !retried
            {
                eprintln!("nyx-lyra: the page skipped the song Lyra was asked for; loading it again");
                *self.recovering.borrow_mut() = Some((video_id.clone(), true));
                *self.replays.borrow_mut() = (None, 0);
                if let Some(rt) = self.runtime() {
                    self.load_page(&rt, Load::Watch { video_id, playlist_id: None, at: None });
                }
                return;
            }
            *self.recovering.borrow_mut() = None;
            self.want_play.set(false);
            if let Some(at) = self.seek_after_play.take() {
                self.send(Command::Seek { position: at as f64 });
            }
            return;
        }
        if now.error.is_none() {
            return;
        }
        let song = now.track.as_ref().and_then(|t| t.video_id.clone());
        {
            let mut r = self.replays.borrow_mut();
            if r.0 == song {
                return; // this song's retries are already running or used up
            }
            *r = (song, 0);
        }
        eprintln!("nyx-lyra: the page's stream failed (MediaError {:?}); asking it to play again", now.error);
        self.replay_step(0);
    }

    /// Ask again after each delay until the song really plays or the delays run out; the
    /// page can give up quietly (stopped, no error), so each step looks again itself.
    fn replay_step(self: &Rc<Self>, step: usize) {
        let Some(&delay) = REPLAY_DELAYS_MS.get(step) else {
            eprintln!("nyx-lyra: the song didn't start after {} tries", REPLAY_DELAYS_MS.len());
            return;
        };
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            let Some(p) = me.upgrade() else { return };
            let s = p.state();
            let plays = s.status == Status::Playing && s.error.is_none();
            if !p.want_play.get() || plays {
                return;
            }
            p.replays.borrow_mut().1 = step as u32 + 1;
            p.send(Command::Play);
            p.replay_step(step + 1);
        });
    }

    pub fn request(self: &Rc<Self>, request: &Request, reply: Reply) {
        self.request_from(request.clone(), reply, Instant::now());
    }

    fn request_from(self: &Rc<Self>, request: Request, reply: Reply, asked: Instant) {
        let connection = self.state().connection;
        let Some(rt) = self.runtime() else { return reply(Err(runtime::RequestError::NotReady)) };
        match connection {
            Connection::Ready if !self.loading.get() => {
                let page = self.page.get();
                let me = Rc::downgrade(self);
                let again = request.clone();
                rt.request(
                    &request,
                    Box::new(move |r| {
                        // Cut off by a page Lyra loaded since, or asked before the page could
                        // answer: ask again once it can, within the wait.
                        let retry = matches!(r, Err(runtime::RequestError::NotReady | runtime::RequestError::Failed(_)));
                        match me.upgrade() {
                            Some(p) if retry && asked.elapsed() < REQUEST_WAIT => {
                                let moved = p.page.get() != page || p.loading.get();
                                if !moved && !matches!(r, Err(runtime::RequestError::NotReady)) {
                                    return reply(r);
                                }
                                p.pending.borrow_mut().push((again, reply, asked));
                                let me = Rc::downgrade(&p);
                                glib::timeout_add_local_once(Duration::from_millis(300), move || {
                                    if let Some(p) = me.upgrade() {
                                        p.send_pending();
                                    }
                                });
                            }
                            _ => reply(r),
                        }
                    }),
                );
            }
            Connection::Ready | Connection::Starting | Connection::Reconnecting => {
                self.pending.borrow_mut().push((request, reply, asked))
            }
            Connection::Lost => reply(Err(runtime::RequestError::NotReady)),
        }
    }

    fn send_pending(self: &Rc<Self>) {
        if self.loading.get() || self.state().connection != Connection::Ready {
            return;
        }
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for (request, reply, asked) in pending {
            self.request_from(request, reply, asked);
        }
    }

    fn expire_pending(&self) {
        let lost = self.state().connection == Connection::Lost;
        let expired: Vec<_> = {
            let mut pending = self.pending.borrow_mut();
            let (old, keep) =
                std::mem::take(&mut *pending).into_iter().partition(|(_, _, at)| lost || at.elapsed() > REQUEST_WAIT);
            *pending = keep;
            old
        };
        for (_, reply, _) in expired {
            reply(Err(runtime::RequestError::NotReady));
        }
    }

    /// Show Google's sign-in page in the page (the sign-in sheet holds it).
    pub fn sign_in(&self, email: Option<String>) {
        if let Some(rt) = self.runtime() {
            self.load_page(&rt, Load::SignIn { email });
        }
    }

    /// Make another channel the active one (YouTube Music's own sign-in address for it);
    /// `then` runs once the page is back.
    pub fn switch_channel(&self, path: String, then: impl FnOnce() + 'static) {
        let Some(rt) = self.runtime() else { return };
        self.on_ready.borrow_mut().push(Box::new(then));
        self.load_page(&rt, Load::Switch { path });
    }

    /// The dev channel's fixed page probes.
    pub fn dev_eval(&self, script: &str, done: Box<dyn FnOnce(String)>) {
        match self.runtime() {
            Some(rt) => rt.dev_eval(script, done),
            None => done("no runtime".into()),
        }
    }

    /// YouTube Music's Home in the page (after leaving Google's sign-in page).
    pub fn home(&self) {
        if let Some(rt) = self.runtime() {
            self.load_page(&rt, Load::Home);
        }
    }

    /// Forget YouTube Music's session, then load Home (signed out, it asks to sign in).
    pub fn sign_out(self: &Rc<Self>) {
        let Some(rt) = self.runtime() else { return };
        let me = Rc::downgrade(self);
        rt.sign_out(Box::new(move || {
            if let Some(p) = me.upgrade() {
                p.home();
            }
        }));
    }

    pub fn terminate(&self) {
        if let Some(rt) = self.runtime() {
            rt.terminate();
        }
    }

    pub fn set_muted(&self, muted: bool) {
        if let Some(rt) = self.runtime() {
            rt.set_muted(muted);
        }
    }

    /// Everything the runtime reports comes through here.
    pub fn on_runtime(self: &Rc<Self>, e: runtime::Event) {
        match e {
            // A new page: its first song gets its own retries.
            runtime::Event::Hello => {
                *self.replays.borrow_mut() = (None, 0);
                self.hello.set(true);
            }
            runtime::Event::State(s) => {
                let mut events = Vec::new();
                if self.state().connection != Connection::Ready {
                    events.extend(self.coord.borrow_mut().connection(Connection::Ready));
                    self.retries.set(0);
                }
                events.extend(self.coord.borrow_mut().report(&s));
                let now = self.state();
                if let (Some(t), Some(pos)) = (&now.track, now.position)
                    && let Some(v) = &t.video_id
                {
                    *self.last.borrow_mut() = Some(Resume { video_id: v.clone(), at: pos as u32, playing: now.is_playing() });
                }
                self.emit(events);
                if self.loading.get() && self.hello.get() {
                    self.loading.set(false);
                    let ready = std::mem::take(&mut *self.on_ready.borrow_mut());
                    ready.into_iter().for_each(|f| f());
                }
                self.send_pending();
                self.finish_resume(&now);
                self.replay_if_failed(&now);
            }
            runtime::Event::Answer(id, answer) => {
                let e = self.coord.borrow_mut().answered(id, &answer);
                self.emit(e.into_iter().collect());
            }
            runtime::Event::Rejected(why) => {
                let n = self.rejected.get() + 1;
                self.rejected.set(n);
                if n == 1 || n.is_multiple_of(100) {
                    eprintln!("nyx-lyra: ignored {n} page message(s) that failed the checks ({why:?})");
                }
            }
            runtime::Event::Ended { again } => {
                let c = if again { Connection::Reconnecting } else { Connection::Lost };
                let events = self.coord.borrow_mut().connection(c);
                self.emit(events);
                if again {
                    *self.resume.borrow_mut() = self.last.borrow().clone();
                    self.reload_after(Duration::from_millis(1500));
                }
            }
            runtime::Event::LoadFailed(why) => {
                eprintln!("nyx-lyra: YouTube Music didn't load: {why}");
                let n = self.retries.get() + 1;
                self.retries.set(n);
                let events = self.coord.borrow_mut().connection(Connection::Reconnecting);
                self.emit(events);
                if self.resume.borrow().is_none() {
                    *self.resume.borrow_mut() = self.last.borrow().clone();
                }
                self.reload_after(Duration::from_secs([5, 15, 30][(n as usize - 1).min(2)]));
            }
        }
    }

    fn reload_after(self: &Rc<Self>, delay: Duration) {
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(delay, move || {
            let Some(p) = me.upgrade() else { return };
            let Some(rt) = p.runtime() else { return };
            match p.resume.borrow().clone() {
                // From the start: a page that starts mid-song fails to play here (see REPLAY_DELAYS_MS).
                Some(r) => p.load_page(&rt, Load::Watch { video_id: r.video_id, playlist_id: None, at: None }),
                None => p.load_page(&rt, Load::Home),
            }
        });
    }

    /// Once the page has the song again, play it if it was playing.
    fn finish_resume(&self, now: &Playback) {
        let Some(r) = self.resume.borrow().clone() else { return };
        let here = now.track.as_ref().and_then(|t| t.video_id.as_deref()) == Some(r.video_id.as_str());
        if !here || now.duration.is_none() {
            return;
        }
        *self.resume.borrow_mut() = None;
        if r.playing {
            *self.recovering.borrow_mut() = Some((r.video_id.clone(), false));
        }
        self.want_play.set(r.playing);
        self.seek_after_play.set((r.at > 3).then_some(r.at));
        if r.playing && !now.is_playing() {
            self.send(Command::Play);
        }
    }
}
