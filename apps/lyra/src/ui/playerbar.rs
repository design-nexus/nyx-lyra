//! The player bar across the foot (the mockup's): the song with its heart; shuffle, the
//! transport and repeat over the seek bar; lyrics, Up next, volume and Now playing. Every
//! control is a command to YouTube Music: a pressed button stays pressed (and play spins)
//! until the page confirms it; the bar shows what the page reports, with the position
//! counted forward between its reports.

use super::{clock, open_now};
use crate::{art, player};
use gtk::{glib, prelude::*};
use nyx_lyra_core::command::{Command, CommandId};
use nyx_lyra_core::playback::{Connection, Playback, Repeat, Status};
use nyx_ui::{media::Artwork, widgets};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

pub fn send(c: Command) -> Option<CommandId> {
    player().map(|p| p.send(c))
}

/// The playing position, counted forward from the page's last report.
#[derive(Default)]
pub struct Clock {
    at: Cell<f64>,
    since: Cell<Option<Instant>>,
    playing: Cell<bool>,
}

impl Clock {
    pub fn set(&self, s: &Playback) {
        self.at.set(s.position.unwrap_or(0.0));
        self.since.set(Some(Instant::now()));
        self.playing.set(s.status == Status::Playing);
    }
    pub fn now(&self) -> f64 {
        let extra = if self.playing.get() { self.since.get().map_or(0.0, |t| t.elapsed().as_secs_f64()) } else { 0.0 };
        self.at.get() + extra
    }
}

/// Like, shuffle and repeat, the same in the bar, the column and Now playing.
pub fn like_button() -> gtk::Button {
    let b = widgets::icon_button("lyra-heart-symbolic", "Like (L)");
    b.add_css_class("like");
    b.connect_clicked(|_| toggle_like());
    b
}

pub fn toggle_like() {
    let Some(p) = player() else { return };
    let s = p.state();
    if s.signed_in == Some(false) {
        if let Some(ui) = super::ui() {
            ui.show_signin(true, None);
        }
        return;
    }
    if let Some(liked) = s.liked {
        p.send(Command::Like { on: !liked });
    }
}

pub fn show_like(b: &gtk::Button, s: &Playback) {
    let liked = s.liked == Some(true);
    b.set_icon_name(if liked { "lyra-heartf-symbolic" } else { "lyra-heart-symbolic" });
    b.set_tooltip_text(Some(if liked { "Unlike (L)" } else { "Like (L)" }));
    if liked {
        b.add_css_class("on");
    } else {
        b.remove_css_class("on");
    }
    b.set_visible(s.track.is_some() && (s.liked.is_some() || s.signed_in == Some(false)));
    b.set_sensitive(s.connection == Connection::Ready);
}

pub fn shuffle_button() -> gtk::Button {
    let b = widgets::icon_button("lyra-shuffle-symbolic", "Shuffle");
    b.connect_clicked(|_| {
        send(Command::Shuffle);
    });
    b
}

pub fn repeat_button() -> gtk::Button {
    let b = widgets::icon_button("lyra-repeat-symbolic", "Repeat");
    b.connect_clicked(|_| {
        if let Some(p) = player() {
            let now = p.state().repeat.unwrap_or_default();
            p.send(Command::Repeat { mode: now.next() });
        }
    });
    b
}

pub fn show_repeat(b: &gtk::Button, s: &Playback) {
    let mode = s.repeat.unwrap_or_default();
    b.set_icon_name(if mode == Repeat::One { "lyra-repeat1-symbolic" } else { "lyra-repeat-symbolic" });
    b.set_tooltip_text(Some(match mode {
        Repeat::Off => "Repeat: off",
        Repeat::All => "Repeat: all",
        Repeat::One => "Repeat: this song",
    }));
    if mode == Repeat::Off {
        b.remove_css_class("pressed");
    } else {
        b.add_css_class("pressed");
    }
    b.set_sensitive(s.connection == Connection::Ready && s.repeat.is_some());
}

pub struct Bar {
    pub root: gtk::Box,
    art: Artwork,
    title: gtk::Label,
    sub: gtk::Label,
    like: gtk::Button,
    shuffle: gtk::Button,
    play: gtk::Button,
    play_icon: gtk::Image,
    spinner: gtk::Spinner,
    prev: gtk::Button,
    next: gtk::Button,
    repeat: gtk::Button,
    seek: gtk::Scale,
    elapsed: gtk::Label,
    total: gtk::Label,
    volume: gtk::Scale,
    mute: gtk::Button,
    offline: gtk::Label,
    sign_in: gtk::Button,
    clock: Rc<Clock>,
    duration: Cell<f64>,
    dragging: Rc<Cell<bool>>,
    setting: Rc<Cell<bool>>,
    last_art: RefCell<Option<String>>,
    /// Commands sent from the bar and the button that stays pressed until they end.
    waiting: RefCell<HashMap<CommandId, gtk::Button>>,
}

impl Bar {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        root.add_css_class("player-bar");

        let np = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        np.set_hexpand(true);
        np.set_width_request(220);
        let art = Artwork::new(48, 48, "lyra-note-symbolic");
        art.root.add_css_class("bar-art");
        let open = gtk::Button::new();
        open.add_css_class("flat");
        open.add_css_class("art-button");
        open.set_child(Some(&art.root));
        open.set_tooltip_text(Some("Now playing (F)"));
        open.connect_clicked(|_| open_now("queue"));
        np.append(&open);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_valign(gtk::Align::Center);
        let title = widgets::label("Nothing playing", "bar-title");
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let sub = widgets::label("Pick something to play", "bar-sub");
        sub.set_xalign(0.0);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&title);
        text.append(&sub);
        np.append(&text);
        let like = like_button();
        like.set_valign(gtk::Align::Center);
        np.append(&like);
        root.append(&np);

        let middle = gtk::Box::new(gtk::Orientation::Vertical, 2);
        middle.set_hexpand(true);
        middle.set_width_request(360);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons.set_halign(gtk::Align::Center);
        let shuffle = shuffle_button();
        let prev = widgets::icon_button("lyra-prev-symbolic", "Previous (Ctrl+←)");
        let play = gtk::Button::new();
        play.add_css_class("play-button");
        play.set_tooltip_text(Some("Play (Space)"));
        let play_icon = gtk::Image::from_icon_name("lyra-play-symbolic");
        let spinner = gtk::Spinner::new();
        play.set_child(Some(&play_icon));
        let next = widgets::icon_button("lyra-next-symbolic", "Next (Ctrl+→)");
        let repeat = repeat_button();
        for b in [&shuffle, &prev, &play, &next, &repeat] {
            buttons.append(b);
        }
        middle.append(&buttons);
        let seek_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let elapsed = widgets::label("", "bar-time");
        elapsed.set_width_chars(5);
        elapsed.set_xalign(1.0);
        let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        seek.set_hexpand(true);
        seek.set_draw_value(false);
        seek.add_css_class("seek");
        let total = widgets::label("", "bar-time");
        total.set_width_chars(5);
        total.set_xalign(0.0);
        seek_row.append(&elapsed);
        seek_row.append(&seek);
        seek_row.append(&total);
        middle.append(&seek_row);
        root.append(&middle);

        let right = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        right.set_hexpand(true);
        right.set_halign(gtk::Align::End);
        right.set_valign(gtk::Align::Center);
        right.set_width_request(220);
        let offline = widgets::label("Offline", "state-chip");
        offline.add_css_class("warn");
        offline.set_visible(false);
        offline.set_margin_end(6);
        right.append(&offline);
        let sign_in = gtk::Button::with_label("Sign in");
        sign_in.set_visible(false);
        sign_in.set_margin_end(6);
        sign_in.connect_clicked(|_| {
            if let Some(ui) = super::ui() {
                ui.show_signin(true, None);
            }
        });
        right.append(&sign_in);
        let lyrics = widgets::icon_button("lyra-lyrics-symbolic", "Lyrics");
        lyrics.connect_clicked(|_| open_now("lyrics"));
        let queue = widgets::icon_button("lyra-queue-symbolic", "Up next (Q)");
        queue.connect_clicked(|_| open_now("queue"));
        let mute = widgets::icon_button("lyra-volume-symbolic", "Mute (M)");
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        volume.set_draw_value(false);
        volume.set_width_request(96);
        volume.add_css_class("volume");
        let expand = widgets::icon_button("lyra-expand-symbolic", "Now playing (F)");
        expand.connect_clicked(|_| open_now("queue"));
        for w in
            [lyrics.upcast_ref::<gtk::Widget>(), queue.upcast_ref(), mute.upcast_ref(), volume.upcast_ref(), expand.upcast_ref()]
        {
            right.append(w);
        }
        root.append(&right);

        let bar = Rc::new(Self {
            root,
            art,
            title,
            sub,
            like,
            shuffle,
            play,
            play_icon,
            spinner,
            prev,
            next,
            repeat,
            seek,
            elapsed,
            total,
            volume,
            mute,
            offline,
            sign_in,
            clock: Rc::default(),
            duration: Cell::new(0.0),
            dragging: Rc::default(),
            setting: Rc::default(),
            last_art: RefCell::default(),
            waiting: RefCell::default(),
        });
        for (button, command) in [(&bar.prev, Command::Previous), (&bar.play, Command::Toggle), (&bar.next, Command::Next)] {
            let me = Rc::downgrade(&bar);
            button.connect_clicked(move |b| {
                if let (Some(me), Some(id)) = (me.upgrade(), send(command.clone())) {
                    me.wait(id, b);
                }
            });
        }
        let me = Rc::downgrade(&bar);
        bar.mute.connect_clicked(move |_| {
            if let (Some(me), Some(p)) = (me.upgrade(), player()) {
                let level = if p.state().volume.unwrap_or(0) > 0 { 0 } else { 70 };
                me.volume.set_value(level as f64);
            }
        });
        // A seek is sent once the user lets go; the bar follows the page meanwhile.
        let drag = gtk::GestureClick::new();
        let d = bar.dragging.clone();
        drag.connect_pressed(move |_, _, _, _| d.set(true));
        let d = bar.dragging.clone();
        let s = bar.seek.clone();
        drag.connect_released(move |_, _, _, _| {
            d.set(false);
            send(Command::Seek { position: s.value() });
        });
        bar.seek.add_controller(drag);
        let setting = bar.setting.clone();
        bar.volume.connect_value_changed(move |v| {
            if !setting.get() {
                send(Command::Volume { level: v.value().round() as u8 });
            }
        });
        let (counter, seek, elapsed, dragging) = (bar.clock.clone(), bar.seek.clone(), bar.elapsed.clone(), bar.dragging.clone());
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if !dragging.get() && seek.adjustment().upper() > 1.0 {
                let now = counter.now().min(seek.adjustment().upper());
                seek.set_value(now);
                elapsed.set_text(&clock(now));
            }
            glib::ControlFlow::Continue
        });
        bar
    }

    /// Keep `button` pressed (and play spinning) until command `id` is confirmed or fails.
    fn wait(&self, id: CommandId, button: &gtk::Button) {
        button.add_css_class("waiting");
        if *button == self.play {
            self.spinner.start();
            self.play.set_child(Some(&self.spinner));
        }
        self.waiting.borrow_mut().insert(id, button.clone());
    }

    /// Command `id` ended (confirmed or not).
    pub fn ended(&self, id: CommandId) {
        let Some(button) = self.waiting.borrow_mut().remove(&id) else { return };
        if !self.waiting.borrow().values().any(|b| *b == button) {
            button.remove_css_class("waiting");
            if button == self.play {
                self.spinner.stop();
                self.play.set_child(Some(&self.play_icon));
            }
        }
    }

    /// M: mute, or back to where the volume was.
    pub fn toggle_mute(&self) {
        self.mute.emit_clicked();
    }

    pub fn update(&self, s: &Playback) {
        self.clock.set(s);
        let playing = s.is_playing();
        self.play_icon.set_icon_name(Some(if playing { "lyra-pause-symbolic" } else { "lyra-play-symbolic" }));
        self.play.set_tooltip_text(Some(if playing { "Pause (Space)" } else { "Play (Space)" }));
        let ready = s.connection == Connection::Ready;
        let has = s.track.is_some();
        self.play.set_sensitive(ready && has);
        self.prev.set_sensitive(ready && s.controls.previous);
        self.next.set_sensitive(ready && s.controls.next);
        self.seek.set_sensitive(ready && s.controls.seek);
        self.shuffle.set_sensitive(ready && has);
        show_like(&self.like, s);
        show_repeat(&self.repeat, s);
        let offline = matches!(s.connection, Connection::Reconnecting | Connection::Lost) && has;
        self.offline.set_visible(offline);
        self.sign_in.set_visible(s.signed_in == Some(false));
        if has {
            self.root.remove_css_class("off");
        } else {
            self.root.add_css_class("off");
        }
        match &s.track {
            Some(t) => {
                self.title.set_text(&t.title);
                let sub = if offline {
                    "Reconnecting to YouTube Music…".to_string()
                } else if s.status == Status::Buffering {
                    "Waiting for YouTube Music…".to_string()
                } else {
                    match &t.album {
                        Some(a) if !a.is_empty() => format!("{} · {a}", t.artist),
                        _ => t.artist.clone(),
                    }
                };
                if offline {
                    self.sub.add_css_class("warn");
                } else {
                    self.sub.remove_css_class("warn");
                }
                self.sub.set_text(&sub);
                self.art.root.remove_css_class("empty-art");
                if *self.last_art.borrow() != t.artwork {
                    *self.last_art.borrow_mut() = t.artwork.clone();
                    art::into(&self.art, t.artwork.as_deref());
                }
            }
            None => {
                self.title.set_text("Nothing playing");
                self.sub.set_text("Pick something to play");
                self.sub.remove_css_class("warn");
                self.art.set_texture(None);
                self.art.root.add_css_class("empty-art");
                *self.last_art.borrow_mut() = None;
            }
        }
        let d = s.duration.unwrap_or(0.0);
        if (d - self.duration.get()).abs() > 0.5 {
            self.duration.set(d);
            self.seek.set_range(0.0, d.max(1.0));
            self.total.set_text(&if d > 0.0 { clock(d) } else { String::new() });
        }
        if !self.dragging.get() {
            self.seek.set_value(s.position.unwrap_or(0.0));
            self.elapsed.set_text(&if d > 0.0 { clock(s.position.unwrap_or(0.0)) } else { String::new() });
        }
        if let Some(v) = s.volume {
            self.setting.set(true);
            self.volume.set_value(v as f64);
            self.setting.set(false);
            if v == 0 {
                self.mute.add_css_class("muted");
            } else {
                self.mute.remove_css_class("muted");
            }
        }
        self.volume.set_sensitive(ready && has);
    }
}
