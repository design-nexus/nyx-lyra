//! The mini-player (the strip): the cover, the song, three controls and Open Lyra, with
//! the progress as a line along its foot. Opening it hides the main window.

use super::playerbar::{Clock, like_button, send, show_like};
use super::ui;
use crate::art;
use gtk::{glib, prelude::*};
use nyx_lyra_core::command::Command;
use nyx_lyra_core::playback::{Connection, Playback};
use nyx_ui::{media::Artwork, widgets};
use std::cell::RefCell;
use std::rc::Rc;

struct Mini {
    window: gtk::Window,
    art: Artwork,
    title: gtk::Label,
    artist: gtk::Label,
    play: gtk::Button,
    like: gtk::Button,
    prev: gtk::Button,
    next: gtk::Button,
    progress: gtk::ProgressBar,
    clock: Rc<Clock>,
    duration: std::cell::Cell<f64>,
    last_art: RefCell<Option<String>>,
}

thread_local! {
    static MINI: RefCell<Option<Rc<Mini>>> = const { RefCell::new(None) };
}

fn build() -> Rc<Mini> {
    let window = gtk::Window::builder().title("Lyra").default_width(380).default_height(96).resizable(false).build();
    window.add_css_class("lyra-window");
    window.add_css_class("lyra-mini");
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    if let Some(app) = gtk::gio::Application::default().and_downcast::<gtk::Application>() {
        window.set_application(Some(&app));
    }
    let root = gtk::Overlay::new();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("mini-row");
    let art = Artwork::new(72, 72, "lyra-note-symbolic");
    art.root.add_css_class("mini-art");
    row.append(&art.root);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
    body.set_hexpand(true);
    body.set_valign(gtk::Align::Center);
    let title = widgets::label("Nothing playing", "bar-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let artist = widgets::label("", "bar-sub");
    artist.set_xalign(0.0);
    artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
    body.append(&title);
    body.append(&artist);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    let prev = widgets::icon_button("lyra-prev-symbolic", "Previous");
    let play = widgets::icon_button("lyra-play-symbolic", "Play");
    play.add_css_class("play-button");
    play.add_css_class("small");
    let next = widgets::icon_button("lyra-next-symbolic", "Next");
    prev.connect_clicked(|_| {
        send(Command::Previous);
    });
    play.connect_clicked(|_| {
        send(Command::Toggle);
    });
    next.connect_clicked(|_| {
        send(Command::Next);
    });
    controls.append(&prev);
    controls.append(&play);
    controls.append(&next);
    let gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gap.set_hexpand(true);
    controls.append(&gap);
    let like = like_button();
    controls.append(&like);
    let open = widgets::icon_button("lyra-expand-symbolic", "Open Lyra");
    open.connect_clicked(|_| toggle());
    controls.append(&open);
    body.append(&controls);
    row.append(&body);
    root.set_child(Some(&row));
    let progress = gtk::ProgressBar::new();
    progress.add_css_class("mini-progress");
    progress.set_valign(gtk::Align::End);
    progress.set_can_target(false);
    root.add_overlay(&progress);
    window.set_child(Some(&root));
    window.connect_close_request(|_| {
        toggle();
        glib::Propagation::Stop
    });
    let mini = Rc::new(Mini {
        window,
        art,
        title,
        artist,
        play,
        like,
        prev,
        next,
        progress,
        clock: Rc::default(),
        duration: Default::default(),
        last_art: RefCell::default(),
    });
    let weak = Rc::downgrade(&mini);
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        let Some(m) = weak.upgrade() else { return glib::ControlFlow::Break };
        let d = m.duration.get();
        if d > 0.0 && m.window.is_visible() {
            m.progress.set_fraction((m.clock.now() / d).clamp(0.0, 1.0));
        }
        glib::ControlFlow::Continue
    });
    if let Some(p) = crate::player() {
        update_mini(&mini, &p.state());
    }
    mini
}

/// Open the mini-player and hide the main window, or the other way round.
pub fn toggle() {
    let mini = MINI.with(|m| m.borrow_mut().get_or_insert_with(build).clone());
    let Some(ui) = ui() else { return };
    // The sign-in sheet needs the window: hiding it would leave Google's page unseen.
    if ui.signing_in() && !mini.window.is_visible() {
        return;
    }
    if mini.window.is_visible() {
        mini.window.set_visible(false);
        ui.window.present();
    } else {
        mini.window.present();
        ui.window.set_visible(false);
    }
}

pub fn update(s: &Playback) {
    if let Some(m) = MINI.with(|m| m.borrow().clone()) {
        update_mini(&m, s);
    }
}

fn update_mini(m: &Mini, s: &Playback) {
    m.clock.set(s);
    m.duration.set(s.duration.unwrap_or(0.0));
    let ready = s.connection == Connection::Ready;
    m.play.set_icon_name(if s.is_playing() { "lyra-pause-symbolic" } else { "lyra-play-symbolic" });
    show_like(&m.like, s);
    m.play.set_sensitive(ready && s.track.is_some());
    m.prev.set_sensitive(ready && s.controls.previous);
    m.next.set_sensitive(ready && s.controls.next);
    match &s.track {
        Some(t) => {
            m.title.set_text(&t.title);
            m.artist.set_text(&match &t.album {
                Some(a) if !a.is_empty() => format!("{} · {a}", t.artist),
                _ => t.artist.clone(),
            });
            if *m.last_art.borrow() != t.artwork {
                *m.last_art.borrow_mut() = t.artwork.clone();
                art::into(&m.art, t.artwork.as_deref());
            }
        }
        None => {
            m.title.set_text("Nothing playing");
            m.artist.set_text("Pick something to play");
        }
    }
}
