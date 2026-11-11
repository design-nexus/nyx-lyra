//! The player bar across the foot: the song, the controls with the seek bar, and lyrics,
//! Up next, volume, the mini-player and Now playing. Every control is a command to
//! YouTube Music; the bar shows what the page reports, with the position counted forward
//! between its reports.

use super::{clock, open_now};
use crate::{art, player};
use gtk::{glib, prelude::*};
use nyx_lyra_core::command::Command;
use nyx_lyra_core::playback::{Connection, Playback, Status};
use nyx_ui::{media::Artwork, widgets};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

pub fn send(c: Command) {
    if let Some(p) = player() {
        p.send(c);
    }
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

pub struct Bar {
    pub root: gtk::Box,
    art: Artwork,
    title: gtk::Label,
    sub: gtk::Label,
    play: gtk::Button,
    prev: gtk::Button,
    next: gtk::Button,
    seek: gtk::Scale,
    elapsed: gtk::Label,
    total: gtk::Label,
    volume: gtk::Scale,
    state: gtk::Label,
    clock: Rc<Clock>,
    duration: Cell<f64>,
    dragging: Rc<Cell<bool>>,
    setting: Rc<Cell<bool>>,
    last_art: RefCell<Option<String>>,
}

impl Bar {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        root.add_css_class("player-bar");

        let np = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        np.set_hexpand(true);
        np.set_width_request(220);
        let art = Artwork::new(48, 48, "audio-x-generic-symbolic");
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
        root.append(&np);

        let middle = gtk::Box::new(gtk::Orientation::Vertical, 2);
        middle.set_hexpand(true);
        middle.set_width_request(360);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons.set_halign(gtk::Align::Center);
        let prev = widgets::icon_button("media-skip-backward-symbolic", "Previous (Ctrl+←)");
        let play = widgets::icon_button("media-playback-start-symbolic", "Play (Space)");
        play.add_css_class("play-button");
        let next = widgets::icon_button("media-skip-forward-symbolic", "Next (Ctrl+→)");
        prev.connect_clicked(|_| send(Command::Previous));
        play.connect_clicked(|_| send(Command::Toggle));
        next.connect_clicked(|_| send(Command::Next));
        buttons.append(&prev);
        buttons.append(&play);
        buttons.append(&next);
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
        right.set_width_request(220);
        let state = widgets::label("", "bar-state");
        state.set_visible(false);
        right.append(&state);
        let lyrics = widgets::icon_button("format-justify-left-symbolic", "Lyrics");
        lyrics.connect_clicked(|_| open_now("lyrics"));
        let queue = widgets::icon_button("view-list-bullet-symbolic", "Up next");
        queue.connect_clicked(|_| open_now("queue"));
        let mute = gtk::Image::from_icon_name("audio-volume-high-symbolic");
        mute.add_css_class("dim-label");
        mute.set_margin_start(6);
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        volume.set_draw_value(false);
        volume.set_width_request(96);
        volume.add_css_class("volume");
        let mini = widgets::icon_button("view-restore-symbolic", "Mini-player (Ctrl+M)");
        mini.connect_clicked(|_| super::mini::toggle());
        let expand = widgets::icon_button("view-fullscreen-symbolic", "Now playing (F)");
        expand.connect_clicked(|_| open_now("queue"));
        for w in [
            lyrics.upcast_ref::<gtk::Widget>(),
            queue.upcast_ref(),
            mute.upcast_ref(),
            volume.upcast_ref(),
            mini.upcast_ref(),
            expand.upcast_ref(),
        ] {
            right.append(w);
        }
        root.append(&right);

        let bar = Self {
            root,
            art,
            title,
            sub,
            play,
            prev,
            next,
            seek,
            elapsed,
            total,
            volume,
            state,
            clock: Rc::default(),
            duration: Cell::new(0.0),
            dragging: Rc::default(),
            setting: Rc::default(),
            last_art: RefCell::default(),
        };
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

    pub fn update(&self, s: &Playback) {
        self.clock.set(s);
        let playing = s.is_playing();
        self.play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
        self.play.set_tooltip_text(Some(if playing { "Pause (Space)" } else { "Play (Space)" }));
        let ready = s.connection == Connection::Ready;
        self.play.set_sensitive(ready && s.track.is_some());
        self.prev.set_sensitive(ready && s.controls.previous);
        self.next.set_sensitive(ready && s.controls.next);
        self.seek.set_sensitive(ready && s.controls.seek);
        let (state, show) = match (s.connection, s.status) {
            (Connection::Reconnecting, _) | (Connection::Starting, _) => ("Reconnecting…", s.track.is_some()),
            (Connection::Lost, _) => ("YouTube Music stopped", true),
            (_, Status::Buffering) => ("Loading…", true),
            _ => ("", false),
        };
        self.state.set_text(state);
        self.state.set_visible(show);
        match &s.track {
            Some(t) => {
                self.title.set_text(&t.title);
                let sub = match &t.album {
                    Some(a) if !a.is_empty() => format!("{} • {a}", t.artist),
                    _ => t.artist.clone(),
                };
                self.sub.set_text(&sub);
                if *self.last_art.borrow() != t.artwork {
                    *self.last_art.borrow_mut() = t.artwork.clone();
                    art::into(&self.art, t.artwork.as_deref());
                }
            }
            None => {
                self.title.set_text("Nothing playing");
                self.sub.set_text("Pick something to play");
                self.art.set_texture(None);
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
        }
        self.volume.set_sensitive(ready && s.track.is_some());
    }
}
