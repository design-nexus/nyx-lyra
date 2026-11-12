//! The window (direction A of nyx-design's `mockups/lyra`): header, sidebar, the
//! now-playing column, the pages, and the player bar; the expanded player and sign-in
//! replace the body when they show. YouTube Music's page lives beneath the native window,
//! covered by it, and fills the sign-in sheet while signing in.

mod column;
pub mod equalizer;
mod expanded;
mod grid;
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
    History,
    /// A search with one of its filters (Songs, Albums…).
    SearchFilter {
        query: String,
        title: String,
        params: String,
    },
    /// A page with one of its options: Home with a mood (a chip), a mood or genre, a chart.
    Mood {
        id: String,
        title: String,
        params: String,
    },
    /// An album's, artist's or playlist's page.
    Browse {
        id: String,
        title: String,
    },
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
            Route::History => "History".into(),
            Route::Mood { title, .. } => title.clone(),
            Route::SearchFilter { query, title, .. } => format!("“{query}” · {title}"),
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
    (&["L"], "Like or unlike the song"),
    (&["Q"], "Up next"),
    (&["M"], "Mute"),
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
    bar: Rc<playerbar::Bar>,
    now: Rc<expanded::Expanded>,
    web: gtk::Widget,
    web_hidden: gtk::Box,
    signin: Signin,
    /// "Sign in" in the header, after "Not now".
    signin_button: gtk::Button,
    /// "Not now" was chosen: the sheet waits for the header's Sign in.
    declined: Cell<bool>,
    /// The header's way back from Now playing.
    collapse: gtk::Button,
    pub history: RefCell<Vec<Route>>,
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
    search.add_css_class("bar-search");
    header.center.append(&search_with_hint(&search));
    let signin_button = gtk::Button::with_label("Sign in");
    signin_button.add_css_class("suggested-action");
    signin_button.set_visible(false);
    signin_button.connect_clicked(|_| {
        if let Some(ui) = ui() {
            ui.show_signin(true, None);
        }
    });
    header.end.append(&signin_button);
    let collapse = widgets::bar_button("lyra-collapse-symbolic", "Back to the library (Esc)");
    collapse.set_visible(false);
    collapse.connect_clicked(|_| close_now());
    header.end.append(&collapse);
    let eq_button = widgets::bar_button("lyra-eq-symbolic", "Equalizer");
    eq_button.connect_clicked(|_| equalizer::open());
    header.end.append(&eq_button);
    let mini_button = widgets::bar_button("lyra-mini-symbolic", "Mini-player (Ctrl+M)");
    mini_button.connect_clicked(|_| mini::toggle());
    header.end.append(&mini_button);
    let settings_button = chrome::settings_button(settings::open);
    settings_button.set_icon_name("lyra-sliders-symbolic");
    header.end.append(&settings_button);

    let side = sidebar::Side::new(&window);
    let column = column::Column::new();
    let pages = gtk::Stack::new();
    pages.set_hexpand(true);
    pages.set_vexpand(true);
    pages.set_transition_type(gtk::StackTransitionType::Crossfade);
    pages.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 120 });
    let bar = playerbar::Bar::new();

    let main = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    main.set_vexpand(true);
    main.append(&column.root);
    // The line between the now-playing column and the pages, shown with the column.
    let column_line = nyx_ui::frame::pane_line(gtk::Orientation::Vertical);
    column.root.bind_property("visible", &column_line, "visible").sync_create().build();
    main.append(&column_line);
    main.append(&pages);

    let now = expanded::Expanded::new();
    let signin = Signin::new();
    let top = gtk::Stack::new();
    top.set_transition_type(gtk::StackTransitionType::Crossfade);
    top.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 160 });
    top.add_named(&main, Some("main"));
    top.add_named(&now.root, Some("now"));
    top.add_named(&signin.root, Some("signin"));
    // Sized by the page showing: Now playing is taller than the library, which shares
    // the window's height with the player bar.
    top.set_vhomogeneous(false);
    // The sidebar and player bar go with the library; Now playing and signing in
    // take the whole sheet.
    let (side_root, bar_root) = (side.side.root.clone(), bar.root.clone());
    top.connect_visible_child_name_notify(move |top| {
        let library = top.visible_child_name().as_deref() == Some("main");
        side_root.set_visible(library);
        bar_root.set_visible(library);
    });

    let frame = nyx_ui::frame::AppFrame::new(&header.root);
    frame.set_sidebar(&side.side.root);
    frame.set_page(&top);
    frame.set_foot(&bar.root);
    // YouTube Music's page sits full size beneath the native window, which covers it: the
    // web player only plays in a shown page of a real size (a small or hidden one stalls).
    let overlay = gtk::Overlay::new();
    let web_hidden = gtk::Box::new(gtk::Orientation::Vertical, 0);
    web_hidden.set_can_target(false);
    web_hidden.set_can_focus(false);
    web.set_vexpand(true);
    web_hidden.append(web);
    overlay.set_child(Some(&web_hidden));
    frame.root.add_css_class("background");
    overlay.add_overlay(&frame.root);
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
        signin,
        signin_button,
        declined: Cell::new(false),
        collapse,
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
    window.connect_default_width_notify(|w| {
        if let Some(u) = ui() {
            u.fit_column()
        }
        nyx_ui::settings_card::fit(w);
        equalizer::fit(w);
    });
    window.connect_default_height_notify(|w| {
        nyx_ui::settings_card::fit(w);
        equalizer::fit(w);
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
    // Nothing has the keyboard at first, so Space and F work at once (GTK would give it
    // to the search field).
    window.connect_map(|w| gtk::prelude::GtkWindowExt::set_focus(w, None::<&gtk::Widget>));
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
            Event::Failed { id, command, failure } => {
                self.bar.ended(*id);
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
            Event::Confirmed { id, .. } => self.bar.ended(*id),
        }
    }

    fn on_state(self: &Rc<Self>, s: &Playback) {
        if s.track.is_none() && self.top.visible_child_name().as_deref() == Some("now") {
            close_now();
        }
        self.bar.update(s);
        self.column.update(s);
        self.now.update(s);
        mini::update(s);
        // Requests wait while the page loads or reconnects; after it was lost they failed.
        if s.connection == Connection::Ready && self.connection.get() == Connection::Lost && s.signed_in != Some(false) {
            glib::idle_add_local_once(refresh);
        }
        self.connection.set(s.connection);
        if s.connection == Connection::Ready {
            match s.signed_in {
                Some(false) if self.signed_in.get() != Some(false) => {
                    if self.signed_in.get() == Some(true) {
                        self.side.reload();
                    }
                    if !self.declined.get() {
                        self.show_signin(true, None)
                    }
                }
                Some(true) if self.signed_in.get() == Some(false) => {
                    self.show_signin(false, None);
                    self.side.reload();
                    refresh();
                }
                _ => {}
            }
            if s.signed_in.is_some() && s.signed_in != self.signed_in.get() {
                self.side.set_signed_in(s.signed_in == Some(true));
                if s.signed_in == Some(true) {
                    crate::quality::ensure();
                }
            }
            if s.signed_in.is_some() {
                self.signed_in.set(s.signed_in);
            }
            self.signin_button.set_visible(s.signed_in == Some(false) && !self.signing_in());
        }
        // A new song: fetch its queue (YouTube Music's own Up next).
        let video = s.track.as_ref().and_then(|t| t.video_id.clone());
        if video.is_some() && *self.queued_for.borrow() != video {
            let first = self.queued_for.borrow().is_none();
            *self.queued_for.borrow_mut() = video.clone();
            if !first {
                notify_song(s);
            }
            // A song played on its own continues with its radio, as YouTube Music's page does.
            let list =
                s.track.as_ref().and_then(|t| t.playlist_id.clone()).or_else(|| video.as_ref().map(|v| format!("RDAMVM{v}")));
            if let (Some(req), Some(p)) = (youtube::up_next(video.as_deref().unwrap_or_default(), list.as_deref()), player()) {
                let me = Rc::downgrade(self);
                p.request(
                    &req,
                    Box::new(move |r| {
                        let Some(ui) = me.upgrade() else { return };
                        if let Ok(n) =
                            r.and_then(|v| youtube::next(&v).map_err(|e| crate::runtime::RequestError::Failed(e.to_string())))
                        {
                            *ui.next.borrow_mut() = Some(n);
                            ui.column.set_queue(ui.next.borrow().as_ref());
                            ui.now.set_queue(ui.next.borrow().as_ref());
                        }
                    }),
                );
            }
        }
    }

    pub fn side_reload(&self) {
        self.side.reload();
    }

    pub fn signing_in(&self) -> bool {
        self.top.visible_child_name().as_deref() == Some("signin")
    }

    /// Signing in shows Google's page in the sheet, filling the window under Lyra's strip;
    /// otherwise the page is back beneath the native window. `account` is `Some(None)` for
    /// "Use another account" (no email filled in); `None` asks Online Accounts.
    pub fn show_signin(self: &Rc<Self>, on: bool, account: Option<Option<String>>) {
        let web = &self.web;
        if let Some(parent) = web.parent().and_downcast::<gtk::Box>() {
            parent.remove(web);
        }
        web.set_size_request(-1, -1);
        web.set_hexpand(true);
        web.set_vexpand(true);
        if on {
            self.declined.set(false);
            self.signin.slot.append(web);
            self.signin.set_account(None);
            let load = |email: Option<String>| {
                if let Some(ui) = ui() {
                    ui.signin.set_account(email.as_deref());
                }
                if let Some(p) = player() {
                    p.sign_in(email);
                }
            };
            // The checks use a made-up address: Google may prompt a real account's phone.
            let test_email = std::env::var("NYX_LYRA_TEST_EMAIL").ok().filter(|_| crate::dev::enabled());
            match (account, test_email) {
                (Some(email), _) => load(email),
                (None, Some(email)) => load(Some(email)),
                (None, None) => crate::accounts::google_email(load),
            }
        } else {
            self.web_hidden.append(web);
        }
        self.top.set_visible_child_name(if on { "signin" } else { "main" });
        let route = self.history.borrow().last().cloned();
        self.header.set_subtitle(&if on { "Sign in".into() } else { route.map(|r| r.title()).unwrap_or_default() });
        self.search.set_sensitive(!on);
        self.signin_button.set_visible(!on && self.signed_in.get() == Some(false));
        if on {
            web.grab_focus();
        }
    }

    /// "Not now": browse signed out; the header's Sign in brings the sheet back.
    fn decline_signin(self: &Rc<Self>) {
        self.declined.set(true);
        self.show_signin(false, None);
        if let Some(p) = player() {
            p.home();
        }
        // Pages asked for while Google's page was up failed or wait for Home: ask again.
        refresh();
    }
}

/// The sign-in sheet: Lyra's strip (where the page is from, whose account, Use another
/// account, Not now) above Google's page.
struct Signin {
    root: gtk::Box,
    slot: gtk::Box,
    title: gtk::Label,
    other: gtk::Button,
}

impl Signin {
    fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("signin-sheet");
        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        strip.add_css_class("signin-strip");
        let lock = gtk::Image::from_icon_name("channel-secure-symbolic");
        lock.add_css_class("signin-lock");
        strip.append(&lock);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let title = widgets::label("Sign in to YouTube Music", "signin-title");
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&title);
        let note =
            widgets::label("accounts.google.com · Google's own page: your password goes to Google, never to Lyra", "dim-label");
        note.set_xalign(0.0);
        note.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&note);
        strip.append(&text);
        let other = gtk::Button::with_label("Use another account");
        other.add_css_class("flat");
        other.set_visible(false);
        other.connect_clicked(|_| {
            if let Some(ui) = ui() {
                ui.show_signin(true, Some(None));
            }
        });
        strip.append(&other);
        let later = gtk::Button::with_label("Not now");
        later.connect_clicked(|_| {
            if let Some(ui) = ui() {
                ui.decline_signin();
            }
        });
        strip.append(&later);
        root.append(&strip);
        let slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        slot.set_hexpand(true);
        slot.set_vexpand(true);
        slot.set_overflow(gtk::Overflow::Hidden);
        root.append(&slot);
        Self { root, slot, title, other }
    }

    fn set_account(&self, email: Option<&str>) {
        match email {
            Some(e) => self.title.set_text(&format!("Sign in to YouTube Music as {e}")),
            None => self.title.set_text("Sign in to YouTube Music"),
        }
        self.other.set_visible(email.is_some());
    }
}

/// The header's search with the mockup's "Ctrl F" at its right, shown while it's idle.
fn search_with_hint(search: &gtk::SearchEntry) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(search));
    let hint = widgets::label("Ctrl F", "search-key");
    hint.set_halign(gtk::Align::End);
    hint.set_valign(gtk::Align::Center);
    hint.set_can_target(false);
    overlay.add_overlay(&hint);
    let update = {
        let (hint, search) = (hint.clone(), search.clone());
        move || hint.set_visible(search.text().is_empty() && !search.has_focus())
    };
    let focus = gtk::EventControllerFocus::new();
    let (u1, u2) = (update.clone(), update.clone());
    focus.connect_enter(move |_| u1());
    focus.connect_leave(move |_| u2());
    search.add_controller(focus);
    search.connect_search_changed(move |_| update());
    overlay
}

/// The song that started, as a notification, when Lyra isn't in front (Settings → Playback).
fn notify_song(s: &Playback) {
    let Some(ui) = ui() else { return };
    let Some(t) = &s.track else { return };
    if !prefs::get().notify || ui.window.is_active() {
        return;
    }
    let icon = t.artwork.as_deref().and_then(crate::art::file).map(|p| p.to_string_lossy().to_string());
    let line = match &t.album {
        Some(a) if !a.is_empty() => format!("{} · {a}", t.artist),
        _ => t.artist.clone(),
    };
    let _ = std::process::Command::new("notify-send")
        .arg("--app-name=Lyra")
        .arg(format!("--icon={}", icon.unwrap_or_else(|| crate::APP_ID.to_string())))
        .arg("--hint=string:x-canonical-private-synchronous:nyx-lyra")
        .arg("--expire-time=4000")
        .arg(&t.title)
        .arg(line)
        .spawn();
}

/// Ask for a name and make a private playlist in the library.
pub fn new_playlist() {
    widgets::ask_text("New playlist", "A private playlist in your YouTube Music library.", "", "Create", |name| {
        let (Some(req), Some(p)) = (youtube::create_playlist(&name, &[]), player()) else { return };
        p.request(
            &req,
            Box::new(move |r| {
                let Some(ui) = ui() else { return };
                match r {
                    Ok(_) => {
                        ui.side.reload();
                        nyx_ui::toast::show(&ui.overlay, "Playlist made");
                    }
                    Err(e) => nyx_ui::toast::show(&ui.overlay, &format!("Couldn't make the playlist: {e}")),
                }
            }),
        );
    });
}

/// Use YouTube Music as another channel: its pages, library and cache start over.
pub fn switch_channel(path: String) {
    let Some(p) = player() else { return };
    crate::cache::clear();
    p.switch_channel(path, || {
        if let Some(ui) = ui() {
            ui.side.reload();
            refresh();
        }
    });
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
    // The field shows the search on screen, as the mockup's.
    match route {
        Route::Search(q) | Route::SearchFilter { query: q, .. } => {
            if ui.search.text() != q.as_str() {
                ui.search.set_text(q);
            }
        }
        _ => ui.search.set_text(""),
    }
    // Signing in stays in front until it's done.
    if !ui.signing_in() {
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
    if !ui.signing_in() {
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
    if !p.play(item)
        && let Some(ui) = ui()
    {
        nyx_ui::toast::show(&ui.overlay, "YouTube Music has nothing to play for this");
    }
}

pub fn open_now(tab: &str) {
    let Some(ui) = ui() else { return };
    // Show lyrics when available (Settings → Playback).
    let lyrics = ui.next.borrow().as_ref().is_some_and(|n| n.lyrics.is_some());
    let tab = if tab == "queue" && lyrics && prefs::get().show_lyrics { "lyrics" } else { tab };
    // Nothing playing: Now playing has nothing to show.
    if ui.signing_in() || player().is_none_or(|p| p.state().track.is_none()) {
        return;
    }
    ui.now.set_tab(tab);
    ui.top.set_visible_child_name("now");
    ui.collapse.set_visible(true);
    // The mockup's Now playing has no search in the header.
    if let Some(field) = ui.search.parent() {
        field.set_visible(false);
    }
    ui.header.set_subtitle("Now playing");
}

pub fn close_now() {
    let Some(ui) = ui() else { return };
    if ui.signing_in() {
        return;
    }
    ui.top.set_visible_child_name("main");
    ui.collapse.set_visible(false);
    if let Some(field) = ui.search.parent() {
        field.set_visible(true);
    }
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
    keys.connect_key_pressed(|_, key, _, mods| on_key(key, mods));
    window.add_controller(keys);
}

/// Lyra's shortcuts; anything else (and everything while signing in) goes on to the focus.
pub fn on_key(key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
    let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
    let alt = mods.contains(gdk::ModifierType::ALT_MASK);
    let Some(u) = ui() else { return glib::Propagation::Proceed };
    if key == gdk::Key::Escape && equalizer::is_open() {
        equalizer::close();
        return glib::Propagation::Stop;
    }
    let focus = gtk::prelude::GtkWindowExt::focus(&u.window);
    // The web page takes typing (Google's sign-in fields, YouTube Music's own).
    let typing = u.search.has_focus() || focus.is_some_and(|f| f.is::<gtk::Text>() || f == u.web || f.is_ancestor(&u.web));
    if !shortcuts_allowed(u.signing_in(), typing, ctrl || alt) {
        return glib::Propagation::Proceed;
    }
    let send = |c: Command| {
        if let Some(p) = player() {
            p.send(c);
        }
    };
    match key {
        gdk::Key::space if !typing => send(Command::Toggle),
        // In a field, Ctrl+←/→ moves by word.
        gdk::Key::Right if ctrl && !typing => send(Command::Next),
        gdk::Key::Left if ctrl && !typing => send(Command::Previous),
        gdk::Key::Left if alt => go_back(),
        gdk::Key::Right if alt => go_forward(),
        gdk::Key::f | gdk::Key::F if ctrl => {
            if let Some(u) = ui() {
                u.search.grab_focus();
            }
        }
        gdk::Key::f if !typing && !ctrl => open_now("queue"),
        gdk::Key::l if !typing && !ctrl => playerbar::toggle_like(),
        gdk::Key::q if !typing && !ctrl => open_now("queue"),
        gdk::Key::m if !typing && !ctrl => u.bar.toggle_mute(),
        gdk::Key::m if ctrl => mini::toggle(),
        gdk::Key::b if ctrl => {
            if let Some(u) = ui() {
                u.side.side.toggle();
                prefs::update(|p| p.sidebar_collapsed = u.side.side.is_collapsed());
            }
        }
        gdk::Key::Escape if !typing && u.top.visible_child_name().as_deref() == Some("now") => close_now(),
        _ => return glib::Propagation::Proceed,
    }
    glib::Propagation::Stop
}

/// For the checks: what's showing and where the keyboard is.
pub fn describe() -> serde_json::Value {
    let Some(u) = ui() else { return serde_json::Value::Null };
    let focus = gtk::prelude::GtkWindowExt::focus(&u.window);
    serde_json::json!({
        "view": u.top.visible_child_name().map(|n| n.to_string()),
        "subtitle": u.header.subtitle.text().to_string(),
        "signin_title": u.signin.title.text().to_string(),
        "focus_in_web": focus.is_some_and(|f| f == u.web || f.is_ancestor(&u.web)),
        "signin_button": u.signin_button.is_visible(),
    })
}

/// For the checks: open the sign-in sheet, or put the keyboard in the page.
pub fn dev_signin() {
    if let Some(u) = ui() {
        u.show_signin(true, None);
    }
}

pub fn dev_account_menu() {
    if let Some(u) = ui() {
        u.side.open_account_menu();
    }
}

pub fn dev_settings() {
    settings::open();
}

pub fn dev_decline() {
    if let Some(u) = ui() {
        u.decline_signin();
    }
}

pub fn dev_focus_web() -> bool {
    ui().is_some_and(|u| u.web.grab_focus())
}

/// Whether a key may act as one of Lyra's shortcuts: none while signing in (every key
/// belongs to Google's page), and single keys only when nothing takes typing.
fn shortcuts_allowed(signing_in: bool, typing: bool, modifier: bool) -> bool {
    !signing_in && (modifier || !typing)
}

/// "3:05" from seconds.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

#[cfg(test)]
mod tests {
    use super::shortcuts_allowed;

    #[test]
    fn shortcuts_leave_typing_alone() {
        // Signing in: nothing, not even Ctrl shortcuts.
        assert!(!shortcuts_allowed(true, true, false));
        assert!(!shortcuts_allowed(true, false, false));
        assert!(!shortcuts_allowed(true, true, true));
        // Typing in the page or a field: only Ctrl and Alt shortcuts.
        assert!(!shortcuts_allowed(false, true, false));
        assert!(shortcuts_allowed(false, true, true));
        // Nothing takes typing: every shortcut.
        assert!(shortcuts_allowed(false, false, false));
        assert!(shortcuts_allowed(false, false, true));
    }
}
