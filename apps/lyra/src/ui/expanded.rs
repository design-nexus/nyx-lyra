//! Now playing, expanded (the art-and-tabs player, the mockup's): the cover large with the
//! song, its heart and menu, the seek bar and the controls on the left; Up next (what it
//! plays from, Save queue, the queue with the song playing marked, Autoplay), Lyrics and
//! Related on the right.

use super::column::queue_row;
use super::playerbar::{Clock, like_button, repeat_button, send, show_like, show_repeat, shuffle_button};
use super::{clock, pages_for, ui};
use crate::{art, player};
use gtk::{glib, prelude::*};
use nyx_lyra_core::command::Command;
use nyx_lyra_core::playback::{Connection, Playback};
use nyx_lyra_core::youtube::{self, Next};
use nyx_ui::{media::Artwork, widgets};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Expanded {
    pub root: gtk::Box,
    art: Artwork,
    title: gtk::Label,
    artist: gtk::Label,
    like: gtk::Button,
    shuffle: gtk::Button,
    play: gtk::Button,
    prev: gtk::Button,
    next: gtk::Button,
    repeat: gtk::Button,
    seek: gtk::Scale,
    elapsed: gtk::Label,
    total: gtk::Label,
    tabs: gtk::Stack,
    from: gtk::Label,
    queue: gtk::Box,
    autoplay: gtk::Switch,
    autoplay_row: gtk::Box,
    setting_autoplay: Rc<Cell<bool>>,
    lyrics: gtk::Box,
    related: gtk::Box,
    clock: Rc<Clock>,
    dragging: Rc<Cell<bool>>,
    last_art: RefCell<Option<String>>,
    lyrics_for: RefCell<Option<String>>,
    related_for: RefCell<Option<String>>,
    next_ids: RefCell<(Option<String>, Option<String>)>,
    /// The queue as last fetched, for Save queue.
    current: RefCell<Option<Next>>,
}

impl Expanded {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 40);
        root.add_css_class("expanded");
        let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left.set_width_request(460);
        let art = Artwork::new(460, 460, "lyra-note-symbolic");
        art.root.add_css_class("big-art");
        left.append(&art.root);
        let meta = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        meta.add_css_class("big-meta");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text.set_hexpand(true);
        let title = widgets::label("Nothing playing", "big-title");
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_max_width_chars(1);
        let artist = widgets::label("", "big-sub");
        artist.set_xalign(0.0);
        artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        artist.set_max_width_chars(1);
        text.append(&title);
        text.append(&artist);
        meta.append(&text);
        let like = like_button();
        like.set_valign(gtk::Align::Start);
        meta.append(&like);
        let more = super::column::song_menu();
        more.set_valign(gtk::Align::Start);
        meta.append(&more);
        left.append(&meta);
        let seek_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        seek_row.add_css_class("big-seek");
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
        left.append(&seek_row);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        buttons.add_css_class("big-controls");
        buttons.set_halign(gtk::Align::Center);
        let shuffle = shuffle_button();
        let prev = widgets::icon_button("lyra-prev-symbolic", "Previous (Ctrl+←)");
        let play = widgets::icon_button("lyra-play-symbolic", "Play (Space)");
        play.add_css_class("play-button");
        play.add_css_class("large");
        let next = widgets::icon_button("lyra-next-symbolic", "Next (Ctrl+→)");
        let repeat = repeat_button();
        prev.connect_clicked(|_| {
            send(Command::Previous);
        });
        play.connect_clicked(|_| {
            send(Command::Toggle);
        });
        next.connect_clicked(|_| {
            send(Command::Next);
        });
        for b in [&shuffle, &prev, &play, &next, &repeat] {
            buttons.append(b);
        }
        left.append(&buttons);
        root.append(&left);

        let right = gtk::Box::new(gtk::Orientation::Vertical, 12);
        right.set_hexpand(true);
        let tabs = gtk::Stack::new();
        tabs.set_vexpand(true);
        // Up next: what it plays from and Save queue, the queue, then Autoplay at the foot.
        let up_next = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.add_css_class("queue-head");
        let from = widgets::label("", "queue-from");
        from.set_xalign(0.0);
        from.set_hexpand(true);
        from.set_ellipsize(gtk::pango::EllipsizeMode::End);
        head.append(&from);
        let save = widgets::labeled_button("lyra-pladd-symbolic", "Save queue");
        save.add_css_class("flat");
        head.append(&save);
        up_next.append(&head);
        let queue = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let queue_scroll =
            gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&queue).build();
        up_next.append(&queue_scroll);
        let autoplay = gtk::Switch::new();
        autoplay.set_valign(gtk::Align::Center);
        let autoplay_row =
            widgets::row("Autoplay", "Keeps playing similar music when the queue ends.", Some(autoplay.upcast_ref()));
        autoplay_row.add_css_class("autoplay-card");
        up_next.append(&autoplay_row);
        tabs.add_named(&up_next, Some("queue"));
        let lyrics = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let related = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (name, child) in [("lyrics", &lyrics), ("related", &related)] {
            let s = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(child).build();
            tabs.add_named(&s, Some(name));
        }
        let me = Rc::new(Self {
            root,
            art,
            title,
            artist,
            like,
            shuffle,
            play,
            prev,
            next,
            repeat,
            seek,
            elapsed,
            total,
            tabs: tabs.clone(),
            from,
            queue,
            autoplay,
            autoplay_row,
            setting_autoplay: Rc::default(),
            lyrics,
            related,
            clock: Rc::default(),
            dragging: Rc::default(),
            last_art: RefCell::default(),
            lyrics_for: RefCell::default(),
            related_for: RefCell::default(),
            next_ids: RefCell::default(),
            current: RefCell::default(),
        });
        let weak = Rc::downgrade(&me);
        let seg = widgets::segmented(
            &widgets::opts(&[("queue", "Up next"), ("lyrics", "Lyrics"), ("related", "Related")]),
            "queue",
            move |v| {
                if let Some(m) = weak.upgrade() {
                    m.show_tab(&v);
                }
            },
        );
        seg.set_halign(gtk::Align::Start);
        seg.set_widget_name("tabs");
        right.append(&seg);
        right.append(&tabs);
        me.root.append(&right);

        let setting = me.setting_autoplay.clone();
        me.autoplay.connect_active_notify(move |s| {
            if !setting.get() {
                send(Command::Autoplay { on: s.is_active() });
            }
        });
        let weak = Rc::downgrade(&me);
        save.connect_clicked(move |_| {
            if let Some(m) = weak.upgrade() {
                m.save_queue();
            }
        });
        let drag = gtk::GestureClick::new();
        let d = me.dragging.clone();
        drag.connect_pressed(move |_, _, _, _| d.set(true));
        let (d, s) = (me.dragging.clone(), me.seek.clone());
        drag.connect_released(move |_, _, _, _| {
            d.set(false);
            send(Command::Seek { position: s.value() });
        });
        me.seek.add_controller(drag);
        let (c, seek, el, dr) = (me.clock.clone(), me.seek.clone(), me.elapsed.clone(), me.dragging.clone());
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if !dr.get() && seek.adjustment().upper() > 1.0 && seek.is_mapped() {
                let now = c.now().min(seek.adjustment().upper());
                seek.set_value(now);
                el.set_text(&clock(now));
            }
            glib::ControlFlow::Continue
        });
        me
    }

    pub fn update(&self, s: &Playback) {
        self.clock.set(s);
        let playing = s.is_playing();
        self.play.set_icon_name(if playing { "lyra-pause-symbolic" } else { "lyra-play-symbolic" });
        let ready = s.connection == Connection::Ready;
        self.play.set_sensitive(ready && s.track.is_some());
        self.prev.set_sensitive(ready && s.controls.previous);
        self.next.set_sensitive(ready && s.controls.next);
        self.seek.set_sensitive(ready && s.controls.seek);
        self.shuffle.set_sensitive(ready && s.track.is_some());
        show_like(&self.like, s);
        show_repeat(&self.repeat, s);
        self.autoplay_row.set_visible(s.autoplay.is_some());
        if let Some(on) = s.autoplay
            && self.autoplay.is_active() != on
        {
            self.setting_autoplay.set(true);
            self.autoplay.set_active(on);
            self.setting_autoplay.set(false);
        }
        if let Some(t) = &s.track {
            self.title.set_text(&t.title);
            self.artist.set_text(&match &t.album {
                Some(a) if !a.is_empty() => format!("{} · {a}", t.artist),
                _ => t.artist.clone(),
            });
            if *self.last_art.borrow() != t.artwork {
                *self.last_art.borrow_mut() = t.artwork.clone();
                art::into(&self.art, t.artwork.as_deref());
            }
        }
        let d = s.duration.unwrap_or(0.0);
        self.seek.set_range(0.0, d.max(1.0));
        self.total.set_text(&if d > 0.0 { clock(d) } else { String::new() });
        if !self.dragging.get() {
            self.seek.set_value(s.position.unwrap_or(0.0));
            self.elapsed.set_text(&if d > 0.0 { clock(s.position.unwrap_or(0.0)) } else { String::new() });
        }
    }

    pub fn set_queue(self: &Rc<Self>, next: Option<&Next>) {
        *self.current.borrow_mut() = next.cloned();
        clear(&self.queue);
        if let Some(n) = next {
            self.from.set_text(&n.playing_from.as_ref().map(|f| format!("Playing from {f}")).unwrap_or_default());
            for (i, item) in n.queue.iter().enumerate().take(60) {
                self.queue.append(&queue_row(item, n.current == Some(i)));
            }
        } else {
            self.from.set_text("");
        }
        *self.next_ids.borrow_mut() = (next.and_then(|n| n.lyrics.clone()), next.and_then(|n| n.related.clone()));
        // A tab that's showing follows the new song.
        if let Some(name) = self.tabs.visible_child_name()
            && name != "queue"
            && self.root.is_mapped()
        {
            self.show_tab(&name);
        }
    }

    /// Save queue: the queue's songs as a new private playlist, named after what it plays from.
    fn save_queue(&self) {
        let Some(n) = self.current.borrow().clone() else { return };
        let ids: Vec<String> = n.queue.iter().filter_map(|i| i.video_id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let name = n.playing_from.clone().unwrap_or_else(|| "Saved queue".into());
        widgets::ask_text(
            "Save queue",
            "A private playlist in your YouTube Music library with these songs.",
            &name,
            "Save",
            move |title| {
                let (Some(req), Some(p)) = (youtube::create_playlist(&title, &ids), player()) else { return };
                p.request(
                    &req,
                    Box::new(move |r| {
                        let Some(ui) = ui() else { return };
                        match r {
                            Ok(_) => {
                                ui.side_reload();
                                nyx_ui::toast::show(&ui.overlay, "Queue saved as a playlist");
                            }
                            Err(e) => nyx_ui::toast::show(&ui.overlay, &format!("Couldn't save the queue: {e}")),
                        }
                    }),
                );
            },
        );
    }

    pub fn set_tab(self: &Rc<Self>, tab: &str) {
        // The segmented control follows too.
        if let Some(seg) = self.root.last_child().and_then(|r| r.first_child()) {
            let mut b = seg.first_child();
            let names = ["queue", "lyrics", "related"];
            let mut i = 0;
            while let Some(w) = b {
                if let Some(t) = w.downcast_ref::<gtk::ToggleButton>() {
                    t.set_active(names.get(i) == Some(&tab));
                }
                i += 1;
                b = w.next_sibling();
            }
        }
        self.show_tab(tab);
    }

    fn show_tab(self: &Rc<Self>, tab: &str) {
        self.tabs.set_visible_child_name(tab);
        let (lyrics_id, related_id) = self.next_ids.borrow().clone();
        match tab {
            "lyrics" => {
                if *self.lyrics_for.borrow() == lyrics_id && lyrics_id.is_some() {
                    return;
                }
                *self.lyrics_for.borrow_mut() = lyrics_id.clone();
                let b = self.lyrics.clone();
                clear(&b);
                let Some(req) = lyrics_id.as_deref().and_then(youtube::browse) else {
                    b.append(&note("No lyrics for this song."));
                    return;
                };
                let Some(p) = player() else { return };
                p.request(
                    &req,
                    Box::new(move |r| {
                        clear(&b);
                        match r.ok().map(|v| youtube::lyrics(&v)) {
                            Some(Ok(Some(l))) => {
                                let text = widgets::label(&l.text, "lyrics");
                                text.set_xalign(0.0);
                                text.set_wrap(true);
                                text.set_selectable(true);
                                b.append(&text);
                                b.append(&note(&l.source));
                            }
                            Some(Ok(None)) => b.append(&note("YouTube Music has no lyrics for this song.")),
                            _ => b.append(&note("Lyra couldn't read the lyrics.")),
                        }
                    }),
                );
            }
            "related" => {
                if *self.related_for.borrow() == related_id && related_id.is_some() {
                    return;
                }
                *self.related_for.borrow_mut() = related_id.clone();
                clear(&self.related);
                match related_id {
                    Some(id) => self.related.append(&pages_for(&id)),
                    None => self.related.append(&note("Nothing related for this song.")),
                }
            }
            _ => {}
        }
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn note(text: &str) -> gtk::Label {
    let l = widgets::label(text, "dim-label");
    l.set_xalign(0.0);
    l.set_wrap(true);
    l
}
