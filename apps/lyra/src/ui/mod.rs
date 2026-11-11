//! The window (direction A of nyx-design's `mockups/lyra`): header, sidebar, the
//! now-playing column, the pages, and the player bar; the expanded player and sign-in
//! replace the body when they show. YouTube Music's page lives beneath the native window,
//! covered by it, and in the sign-in card while signing in.

mod column;
mod expanded;
pub mod mini;
mod pages;
mod playerbar;
mod settings;
mod sidebar;

use crate::{player, prefs};
use gtk::{gdk, glib, prelude::*};
use nyx_lyra_core::command::{Command, Event};
use nyx_lyra_core::model::Item;
use nyx_lyra_core::playback::{Connection, Playback};
use nyx_lyra_core::youtube::{self, Next};
use nyx_ui::{chrome, widgets};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Where the pages can go.
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Home,
    Explore,
    Search(String),
    Library(Tab),
    Liked,
    /// An album's, artist's or playlist's page.
    Browse { id: String, title: String },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tab {
    Playlists,
    Albums,
    Artists,
    Songs,
}

impl Route {
    fn title(&self) -> String {
        match self {
            Route::Home => "Home".into(),
            Route::Explore => "Explore".into(),
            Route::Search(q) if q.is_empty() => "Search".into(),
            Route::Search(q) => format!("“{q}”"),
            Route::Library(_) => "Library".into(),
            Route::Liked => "Liked Music".into(),
            Route::Browse { title, .. } => title.clone(),
        }
    }
}

pub const SHORTCUTS: &[(&[&str], &str)] = &[
    (&["Space"], "Play or pause"),
    (&["Ctrl", "→"], "Next song"),
    (&["Ctrl", "←"], "Previous song"),
    (&["Ctrl", "F"], "Search"),
    (&["Alt", "←"], "Back"),
    (&["Alt", "→"], "Forward"),
    (&["F"], "Now playing"),
    (&["Ctrl", "M"], "Mini-player"),
    (&["Ctrl", "B"], "Collapse the sidebar"),
    (&["Esc"], "Close now playing"),
];

pub struct Ui {
    pub window: gtk::ApplicationWindow,
    pub overlay: gtk::Overlay,
    header: chrome::AppHeader,
    top: gtk::Stack,
    pages: gtk::Stack,
    search: gtk::SearchEntry,
    back: gtk::Button,
    forward: gtk::Button,
    side: sidebar::Side,
    column: column::Column,
    bar: playerbar::Bar,
    now: Rc<expanded::Expanded>,
    web: gtk::Widget,
    web_hidden: gtk::Box,
    web_card: gtk::Box,
    history: RefCell<Vec<Route>>,
    ahead: RefCell<Vec<Route>>,
    /// The queue for the song playing (YouTube Music's own, from its Up next).
    pub next: RefCell<Option<Next>>,
    queued_for: RefCell<Option<String>>,
    signed_in: Cell<Option<bool>>,
    connection: Cell<Connection>,
}

thread_local! {
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

pub fn ui() -> Option<Rc<Ui>> {
    UI.with(|u| u.borrow().clone())
}

/// Build the window around YouTube Music's page (`web`).
pub fn build(app: &gtk::Application, web: &gtk::Widget) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder().application(app).title("Lyra").default_width(1280).default_height(800).build();
    window.add_css_class("lyra-window");
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    window.set_icon_name(Some(crate::APP_ID));

    let header = chrome::app_header(&window, "io.github.design_nexus.nyx.Lyra-symbolic", "Lyra");
    let back = widgets::bar_button("go-previous-symbolic", "Back (Alt+←)");
    let forward = widgets::bar_button("go-next-symbolic", "Forward (Alt+→)");
    back.set_sensitive(false);
    forward.set_sensitive(false);
    header.start.append(&back);
    header.start.append(&forward);
    let search = gtk::SearchEntry::builder().placeholder_text("Search YouTube Music").width_request(340).build();
    search.add_css_class("header-search");
    header.center.append(&search);
    let mini_button = widgets::bar_button("view-restore-symbolic", "Mini-player (Ctrl+M)");
    mini_button.connect_clicked(|_| mini::toggle());
    header.end.append(&mini_button);
    header.end.append(&chrome::settings_button(settings::open));

    let side = sidebar::Side::new(&window);
    let column = column::Column::new();
    let pages = gtk::Stack::new();
    pages.set_hexpand(true);
    pages.set_vexpand(true);
    pages.set_transition_type(gtk::StackTransitionType::Crossfade);
    pages.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 120 });
    let bar = playerbar::Bar::new();

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    body.append(&side.side.root);
    body.append(&column.root);
    body.append(&pages);
    let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
    main.append(&body);
    main.append(&bar.root);

    let now = expanded::Expanded::new();
    let (signin, web_card) = signin_page();
    let top = gtk::Stack::new();
    top.set_transition_type(gtk::StackTransitionType::Crossfade);
    top.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 160 });
    top.add_named(&main, Some("main"));
    top.add_named(&now.root, Some("now"));
    top.add_named(&signin, Some("signin"));

    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("window-frame");
    frame.append(&header.root);
    frame.append(&top);
    // YouTube Music's page sits full size beneath the native window, which covers it: the
    // web player only plays in a shown page of a real size (a small or hidden one stalls).
    let overlay = gtk::Overlay::new();
    let web_hidden = gtk::Box::new(gtk::Orientation::Vertical, 0);
    web_hidden.set_can_target(false);
    web_hidden.set_can_focus(false);
    web.set_vexpand(true);
    web_hidden.append(web);
    overlay.set_child(Some(&web_hidden));
    frame.add_css_class("background");
    overlay.add_overlay(&frame);
    window.set_child(Some(&overlay));

    let this = Rc::new(Ui {
        window: window.clone(),
        overlay,
        header,
        top,
        pages,
        search: search.clone(),
        back: back.clone(),
        forward: forward.clone(),
        side,
        column,
        bar,
        now,
        web: web.clone(),
        web_hidden,
        web_card,
        history: RefCell::default(),
        ahead: RefCell::default(),
        next: RefCell::default(),
        queued_for: RefCell::default(),
        signed_in: Cell::new(None),
        connection: Cell::new(Connection::Starting),
    });
    UI.with(|u| *u.borrow_mut() = Some(this.clone()));

    back.connect_clicked(|_| go_back());
    forward.connect_clicked(|_| go_forward());
    search.connect_activate(|e| {
        let q = e.text().trim().to_string();
        if !q.is_empty() {
            remember_search(&q);
            navigate(Route::Search(q));
        }
    });
    nyx_ui::keys::install(&window, nyx_ui::keys::Common { settings: Some(settings::open), ..Default::default() });
    install_keys(&window);
    this.side.side.follow_window(&window);
    this.side.side.set_collapsed(prefs::get().sidebar_collapsed);
    window.connect_default_width_notify(|_| {
        if let Some(u) = ui() {
            u.fit_column()
        }
    });
    this.fit_column();

    if let Some(p) = player() {
        p.connect(|e| {
            if let Some(ui) = ui() {
                ui.on_event(e)
            }
        });
        this.on_state(&p.state());
    }
    navigate(Route::Home);
    window
}

impl Ui {
    /// The now-playing column shows on wide windows only.
    pub fn fit_column(&self) {
        let w = if self.window.width() > 0 { self.window.width() } else { self.window.default_width() };
        self.column.fit(w);
    }

    fn on_event(self: &Rc<Self>, e: &Event) {
        match e {
            Event::State(s) => self.on_state(s),
            Event::Failed { command, failure, .. } => {
                use nyx_lyra_core::command::Failure;
                let what = match command {
                    Command::Next => "skip to the next song",
                    Command::Previous => "go back a song",
                    Command::Seek { .. } => "seek",
                    Command::Volume { .. } => "change the volume",
                    _ => "play or pause",
                };
                let why = match failure {
                    Failure::Unsupported => "YouTube Music doesn't offer that here".to_string(),
                    Failure::TimedOut => "YouTube Music didn't confirm it".to_string(),
                    Failure::Disconnected => "YouTube Music is reconnecting".to_string(),
                    Failure::NoMedia => "nothing is loaded".to_string(),
                    Failure::BadArgument => "that isn't possible".to_string(),
                    Failure::Refused(r) => r.clone(),
                };
                nyx_ui::toast::show(&self.overlay, &format!("Couldn't {what}: {why}"));
            }
            Event::Confirmed { .. } => {}
        }
    }

    fn on_state(self: &Rc<Self>, s: &Playback) {
        self.bar.update(s);
        self.column.update(s);
        self.now.update(s);
        mini::update(s);
        // Pages asked for before YouTube Music was ready (or while it reconnected) load now.
        if s.connection == Connection::Ready && self.connection.get() != Connection::Ready && s.signed_in != Some(false) {
            glib::idle_add_local_once(refresh);
        }
        self.connection.set(s.connection);
        if s.connection == Connection::Ready {
            match s.signed_in {
                Some(false) if self.signed_in.get() != Some(false) => self.show_signin(true),
                Some(true) if self.signed_in.get() == Some(false) => {
                    self.show_signin(false);
                    self.side.reload();
                    refresh();
                }
                _ => {}
            }
            if s.signed_in.is_some() {
                self.signed_in.set(s.signed_in);
            }
        }
        // A new song: fetch its queue (YouTube Music's own Up next).
        let video = s.track.as_ref().and_then(|t| t.video_id.clone());
        if video.is_some() && *self.queued_for.borrow() != video {
            *self.queued_for.borrow_mut() = video.clone();
            // A song played on its own continues with its radio, as YouTube Music's page does.
            let list = s.track.as_ref().and_then(|t| t.playlist_id.clone()).or_else(|| video.as_ref().map(|v| format!("RDAMVM{v}")));
            if let (Some(req), Some(p)) = (youtube::up_next(video.as_deref().unwrap_or_default(), list.as_deref()), player()) {
                let me = Rc::downgrade(self);
                p.request(
                    &req,
                    Box::new(move |r| {
                        let Some(ui) = me.upgrade() else { return };
                        if let Ok(n) = r.and_then(|v| youtube::next(&v).map_err(|e| crate::runtime::RequestError::Failed(e.to_string()))) {
                            *ui.next.borrow_mut() = Some(n);
                            ui.column.set_queue(ui.next.borrow().as_ref());
                            ui.now.set_queue(ui.next.borrow().as_ref());
                        }
                    }),
                );
            }
        }
    }

    /// Signing in shows YouTube Music's page in the sign-in card; otherwise it's back
    /// beneath the native window.
    fn show_signin(&self, on: bool) {
        let web = &self.web;
        if let Some(parent) = web.parent().and_downcast::<gtk::Box>() {
            parent.remove(web);
        }
        if on {
            web.set_size_request(440, 560);
            web.set_hexpand(false);
            web.set_vexpand(false);
            self.web_card.append(web);
            if let Some(p) = player() {
                p.sign_in();
            }
        } else {
            web.set_size_request(-1, -1);
            web.set_hexpand(true);
            web.set_vexpand(true);
            self.web_hidden.append(web);
        }
        self.top.set_visible_child_name(if on { "signin" } else { "main" });
        self.header.set_subtitle(if on { "Sign in" } else { "" });
    }
}

fn signin_page() -> (gtk::Box, gtk::Box) {
    let page = gtk::Box::new(gtk::Orientation::Horizontal, 56);
    page.add_css_class("signin");
    page.set_halign(gtk::Align::Center);
    page.set_valign(gtk::Align::Center);
    let copy = gtk::Box::new(gtk::Orientation::Vertical, 10);
    copy.set_width_request(360);
    copy.set_valign(gtk::Align::Center);
    let icon = gtk::Image::from_icon_name("io.github.design_nexus.nyx.Lyra-symbolic");
    icon.set_pixel_size(56);
    icon.set_halign(gtk::Align::Start);
    icon.add_css_class("signin-icon");
    copy.append(&icon);
    let title = widgets::label("Sign in to YouTube Music", "signin-title");
    title.set_xalign(0.0);
    copy.append(&title);
    for text in [
        "Lyra plays music through YouTube Music's own web player, so you sign in on Google's page, shown here. Your password goes to Google, never to Lyra.",
        "YouTube Music doesn't play anything signed out.",
    ] {
        let l = widgets::label(text, "dim-label");
        l.set_wrap(true);
        l.set_xalign(0.0);
        l.set_max_width_chars(42);
        copy.append(&l);
    }
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("web-card");
    card.set_size_request(440, -1);
    card.set_overflow(gtk::Overflow::Hidden);
    card.set_valign(gtk::Align::Center);
    let origin = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    origin.add_css_class("web-origin");
    origin.append(&gtk::Image::from_icon_name("channel-secure-symbolic"));
    origin.append(&widgets::label("accounts.google.com · Google's sign-in page", "dim-label"));
    card.append(&origin);
    page.append(&copy);
    page.append(&card);
    (page, card)
}

/* ── Navigation ────────────────────────────────────────────────────────────────── */

pub fn navigate(route: Route) {
    let Some(ui) = ui() else { return };
    if ui.history.borrow().last() == Some(&route) {
        return refresh();
    }
    ui.history.borrow_mut().push(route.clone());
    ui.ahead.borrow_mut().clear();
    show(&ui, &route);
}

fn go_back() {
    let Some(ui) = ui() else { return };
    if ui.top.visible_child_name().as_deref() == Some("now") {
        return close_now();
    }
    if ui.history.borrow().len() < 2 {
        return;
    }
    let current = ui.history.borrow_mut().pop();
    ui.ahead.borrow_mut().extend(current);
    let route = ui.history.borrow().last().cloned();
    if let Some(r) = route {
        show(&ui, &r);
    }
}

fn go_forward() {
    let Some(ui) = ui() else { return };
    let Some(r) = ui.ahead.borrow_mut().pop() else { return };
    ui.history.borrow_mut().push(r.clone());
    show(&ui, &r);
}

/// Load the page that's showing again.
pub fn refresh() {
    let Some(ui) = ui() else { return };
    let route = ui.history.borrow().last().cloned();
    if let Some(r) = route {
        show(&ui, &r);
    }
}

fn show(ui: &Rc<Ui>, route: &Route) {
    // Signing in stays in front until it's done.
    if ui.top.visible_child_name().as_deref() != Some("signin") {
        ui.top.set_visible_child_name("main");
    }
    let page = pages::open(route);
    let name = format!("p{}", glib::random_int());
    ui.pages.add_named(&page, Some(&name));
    ui.pages.set_visible_child(&page);
    // Drop pages that aren't showing (they're cached; going back reloads from the cache).
    let mut child = ui.pages.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if c != page {
            ui.pages.remove(&c);
        }
    }
    if ui.top.visible_child_name().as_deref() != Some("signin") {
        ui.header.set_subtitle(&route.title());
    }
    chrome::set_title(&ui.window, "Lyra", &route.title());
    ui.side.select(route);
    ui.back.set_sensitive(ui.history.borrow().len() > 1);
    ui.forward.set_sensitive(!ui.ahead.borrow().is_empty());
}

/// A page by its browse ID, for places that show one inside themselves (Related).
pub fn pages_for(id: &str) -> gtk::Widget {
    pages::open(&Route::Browse { id: id.to_string(), title: String::new() })
}

/// What clicking an item does: a song plays, anything else opens its page.
pub fn activate(item: &Item) {
    use nyx_lyra_core::model::Kind;
    match (item.kind, &item.browse_id) {
        (Kind::Album | Kind::Single | Kind::Ep | Kind::Playlist | Kind::Artist, Some(id)) => {
            navigate(Route::Browse { id: id.clone(), title: item.title.clone() })
        }
        _ => play(item),
    }
}

pub fn play(item: &Item) {
    let Some(p) = player() else { return };
    if !p.play(item) {
        if let Some(ui) = ui() {
            nyx_ui::toast::show(&ui.overlay, "YouTube Music has nothing to play for this");
        }
    }
}

pub fn open_now(tab: &str) {
    let Some(ui) = ui() else { return };
    ui.now.set_tab(tab);
    ui.top.set_visible_child_name("now");
    ui.header.set_subtitle("Now playing");
}

pub fn close_now() {
    let Some(ui) = ui() else { return };
    ui.top.set_visible_child_name("main");
    let route = ui.history.borrow().last().cloned();
    ui.header.set_subtitle(&route.map(|r| r.title()).unwrap_or_default());
}

fn remember_search(q: &str) {
    prefs::update(|p| {
        p.recent_searches.retain(|s| s != q);
        p.recent_searches.insert(0, q.to_string());
        p.recent_searches.truncate(12);
    });
}

fn install_keys(window: &gtk::ApplicationWindow) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(|_, key, _, mods| {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let alt = mods.contains(gdk::ModifierType::ALT_MASK);
        let typing = ui().is_some_and(|u| u.search.has_focus() || gtk::prelude::GtkWindowExt::focus(&u.window).is_some_and(|f| f.is::<gtk::Text>()));
        let send = |c: Command| {
            if let Some(p) = player() {
                p.send(c);
            }
        };
        match key {
            gdk::Key::space if !typing => send(Command::Toggle),
            gdk::Key::Right if ctrl => send(Command::Next),
            gdk::Key::Left if ctrl => send(Command::Previous),
            gdk::Key::Left if alt => go_back(),
            gdk::Key::Right if alt => go_forward(),
            gdk::Key::f | gdk::Key::F if ctrl => {
                if let Some(u) = ui() {
                    u.search.grab_focus();
                }
            }
            gdk::Key::f if !typing && !ctrl => open_now("queue"),
            gdk::Key::m if ctrl => mini::toggle(),
            gdk::Key::b if ctrl => {
                if let Some(u) = ui() {
                    u.side.side.toggle();
                    prefs::update(|p| p.sidebar_collapsed = u.side.side.is_collapsed());
                }
            }
            gdk::Key::Escape if ui().is_some_and(|u| u.top.visible_child_name().as_deref() == Some("now")) => close_now(),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    window.add_controller(keys);
}

/// "3:05" from seconds.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}
