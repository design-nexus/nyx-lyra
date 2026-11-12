//! The sidebar (the mockup's): Home, Explore and Search; the library with History; the
//! playlists from YouTube Music; and the channel at the foot, which switches channels and
//! signs out.

use super::{Route, Tab, navigate};
use crate::{art, player};
use gtk::prelude::*;
use nyx_lyra_core::model::{Channel, Kind, Page};
use nyx_lyra_core::youtube;
use nyx_ui::sidebar::{self, NavItem, Sidebar};
use std::cell::RefCell;
use std::rc::Rc;

/// At most this many pages of the library's playlists (100 each).
const MAX_PLAYLIST_PAGES: usize = 20;

pub struct Side {
    pub side: Rc<Sidebar>,
    items: Vec<(Route, Rc<NavItem>)>,
    all_playlists: Rc<NavItem>,
    playlists: gtk::Box,
    playlist_items: Rc<RefCell<Vec<(String, gtk::Button)>>>,
    foot: Foot,
}

impl Side {
    pub fn new(_window: &gtk::ApplicationWindow) -> Self {
        let side = Sidebar::new();
        let add = |route: Route, icon: &str, title: &str, tip: &str| {
            let n = sidebar::nav_item(&format!("lyra-{icon}-symbolic"), title, tip);
            let r = route.clone();
            n.button.connect_clicked(move |_| navigate(r.clone()));
            side.append(&n.button);
            (route, Rc::new(n))
        };
        let mut navs = vec![
            add(Route::Home, "home", "Home", "Home"),
            add(Route::Explore, "compass", "Explore", "New releases, charts, moods and genres"),
            add(Route::Search(String::new()), "search", "Search", "Search (Ctrl+F)"),
        ];
        let (library_head, library_toggle) = collapsible_group("Your library");
        side.append(&library_head);
        let library = gtk::Box::new(gtk::Orientation::Vertical, 1);
        side.append(&library);
        let lib = |route: Route, icon: &str, title: &str, tip: &str| {
            let n = sidebar::nav_item(&format!("lyra-{icon}-symbolic"), title, tip);
            let r = route.clone();
            n.button.connect_clicked(move |_| navigate(r.clone()));
            library.append(&n.button);
            (route, Rc::new(n))
        };
        navs.push(lib(Route::Library(Tab::Playlists), "library", "Library", "Your playlists, albums, songs and artists"));
        navs.push(lib(Route::Liked, "heart", "Liked Music", "Songs you liked"));
        navs.push(lib(Route::Library(Tab::Artists), "user", "Artists", "Artists in your library"));
        navs.push(lib(Route::Library(Tab::Albums), "disc", "Albums", "Albums you saved"));
        navs.push(lib(Route::Library(Tab::Songs), "note", "Songs", "Songs in your library"));
        navs.push(lib(Route::History, "history", "History", "What you played lately"));
        let lib_box = library.clone();
        library_toggle.connect_clicked(move |b| {
            let open = !lib_box.is_visible();
            lib_box.set_visible(open);
            b.set_icon_name(if open { "lyra-chev-d-symbolic" } else { "lyra-chev-r-symbolic" });
        });
        side.append(&sidebar::group_label_with_add("Playlists", false, "New playlist", super::new_playlist));
        let all_playlists = Rc::new(sidebar::nav_item("lyra-list-symbolic", "All playlists", "Every playlist in your library"));
        all_playlists.button.connect_clicked(|_| navigate(Route::Library(Tab::Playlists)));
        side.append(&all_playlists.button);
        let playlists = gtk::Box::new(gtk::Orientation::Vertical, 1);
        side.append(&playlists);
        let foot = Foot::new();
        side.append_foot(&foot.root);
        let s = Self { side, items: navs, all_playlists, playlists, playlist_items: Rc::default(), foot };
        s.reload();
        s
    }

    /// The playlists, Liked Music's count and the channel, from YouTube Music (requests
    /// wait for the page).
    pub fn reload(&self) {
        let Some(p) = player() else { return };
        self.load_playlists();
        if let Some(liked) = self.items.iter().find(|(r, _)| *r == Route::Liked).map(|(_, n)| n.clone())
            && let Some(req) = youtube::browse(youtube::LIKED_MUSIC)
        {
            p.request(
                &req,
                Box::new(move |r| {
                    let count = r
                        .ok()
                        .and_then(|v| youtube::page(&v).ok())
                        .and_then(|page| page.header)
                        .and_then(|h| song_count(&h.second_subtitle).or_else(|| song_count(&h.subtitle)));
                    liked.set_count(count);
                }),
            );
        }
        let foot = self.foot.clone();
        p.request(&youtube::account_menu(), Box::new(move |r| foot.show(r.ok().and_then(|v| youtube::account(&v)).as_ref())));
    }

    fn load_playlists(&self) {
        let parts = SideParts {
            playlists: self.playlists.clone(),
            rows: self.playlist_items.clone(),
            side: self.side.clone(),
            count: self.all_playlists.clone(),
        };
        parts.more(youtube::browse(youtube::LIBRARY_PLAYLISTS), 0, Vec::new());
    }

    pub fn select(&self, route: &Route) {
        for (r, n) in &self.items {
            let on = match (r, route) {
                (Route::Search(_), Route::Search(_) | Route::SearchFilter { .. }) => true,
                (Route::Home, Route::Mood { id, .. }) => id == youtube::HOME,
                (a, b) => a == b,
            };
            n.set_active(on);
        }
        let id = match route {
            Route::Browse { id, .. } => Some(id.as_str()),
            _ => None,
        };
        for (pid, b) in self.playlist_items.borrow().iter() {
            if Some(pid.as_str()) == id {
                b.add_css_class("active");
            } else {
                b.remove_css_class("active");
            }
        }
    }

    /// For the checks: open the channel menu.
    pub fn open_account_menu(&self) {
        self.foot.root.popup();
    }

    /// Signed out, the foot offers Sign in and the library's rows are left empty.
    pub fn set_signed_in(&self, signed_in: bool) {
        if !signed_in {
            self.foot.show(None);
            clear(&self.playlists);
            self.playlist_items.borrow_mut().clear();
            self.all_playlists.set_count(None);
            if let Some((_, n)) = self.items.iter().find(|(r, _)| *r == Route::Liked) {
                n.set_count(None);
            }
        }
    }
}

/// What the playlist pages fill in, cloned into the requests.
#[derive(Clone)]
struct SideParts {
    playlists: gtk::Box,
    rows: Rc<RefCell<Vec<(String, gtk::Button)>>>,
    side: Rc<Sidebar>,
    count: Rc<NavItem>,
}

impl SideParts {
    fn show(&self, list: &[(String, String, Option<String>)]) {
        clear(&self.playlists);
        self.rows.borrow_mut().clear();
        for (id, title, thumb) in list {
            let b = gtk::Button::new();
            b.add_css_class("nav-item");
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let a = nyx_ui::media::Artwork::new(20, 20, "lyra-list-symbolic");
            a.root.add_css_class("nav-art");
            art::into(&a, thumb.as_deref());
            content.append(&a.root);
            let l = nyx_ui::widgets::label(title, "nav-label");
            l.add_css_class("compact-hide");
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            l.set_xalign(0.0);
            l.set_hexpand(true);
            content.append(&l);
            b.set_child(Some(&content));
            b.set_tooltip_text(Some(title));
            let route = Route::Browse { id: id.clone(), title: title.clone() };
            b.connect_clicked(move |_| navigate(route.clone()));
            self.playlists.append(&b);
            self.rows.borrow_mut().push((id.clone(), b));
        }
        self.count.set_count(Some(list.len() as u64));
        self.side.refresh();
    }

    fn more(&self, req: Option<youtube::Request>, pages: usize, mut so_far: Vec<(String, String, Option<String>)>) {
        let (Some(req), Some(p)) = (req, player()) else { return };
        let me = self.clone();
        p.request(
            &req,
            Box::new(move |r| {
                let Some(page): Option<Page> = r.ok().and_then(|v| youtube::page(&v).ok()) else { return };
                for item in page.items().filter(|i| i.kind == Kind::Playlist) {
                    // Liked Music has its own row above.
                    if let Some(id) = item.browse_id.clone().filter(|id| id != youtube::LIKED_MUSIC) {
                        so_far.push((id, item.title.clone(), item.thumbnail(40).map(|t| t.url.clone())));
                    }
                }
                me.show(&so_far);
                if pages + 1 < MAX_PLAYLIST_PAGES
                    && let Some(token) = page.continuation.as_deref()
                {
                    me.more(youtube::more(token), pages + 1, so_far);
                }
            }),
        );
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// "1,284 songs" → 1284.
fn song_count(text: &str) -> Option<u64> {
    let words: Vec<&str> = text.split(|c: char| c == '•' || c.is_whitespace()).filter(|w| !w.is_empty()).collect();
    words.windows(2).find(|w| w[1].starts_with("song")).and_then(|w| w[0].replace(',', "").parse().ok())
}

/// A group heading with a chevron that folds the group away.
fn collapsible_group(text: &str) -> (gtk::Box, gtk::Button) {
    let row = sidebar::group_label(text, false);
    let chev = nyx_ui::widgets::icon_button("lyra-chev-d-symbolic", "Show or hide");
    chev.add_css_class("nav-group-add");
    chev.set_valign(gtk::Align::End);
    row.append(&chev);
    (row, chev)
}

/* ── The channel at the foot ───────────────────────────────────────────────────── */

#[derive(Clone)]
struct Foot {
    root: gtk::MenuButton,
    photo: nyx_ui::media::Artwork,
    name: gtk::Label,
    sub: gtk::Label,
    chev: gtk::Image,
    signed_in: Rc<std::cell::Cell<bool>>,
}

impl Foot {
    fn new() -> Self {
        let root = gtk::MenuButton::new();
        root.set_always_show_arrow(false);
        root.add_css_class("acct");
        root.add_css_class("flat");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let photo = nyx_ui::media::Artwork::new(26, 26, "lyra-user-symbolic");
        photo.root.add_css_class("avatar");
        row.append(&photo.root);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.add_css_class("compact-hide");
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let name = nyx_ui::widgets::label("Sign in", "acct-name");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let sub = nyx_ui::widgets::label("To see your library", "acct-sub");
        sub.set_xalign(0.0);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&name);
        text.append(&sub);
        row.append(&text);
        let chev = gtk::Image::from_icon_name("lyra-chev-d-symbolic");
        chev.add_css_class("acct-chev");
        chev.add_css_class("compact-hide");
        chev.set_visible(false);
        row.append(&chev);
        root.set_child(Some(&row));
        let foot = Self { root, photo, name, sub, chev, signed_in: Rc::default() };
        let me = foot.clone();
        foot.root.set_create_popup_func(move |button| {
            if me.signed_in.get() {
                button.set_popover(Some(&channels_popover()));
            } else {
                button.set_popover(None::<&gtk::Popover>);
                if let Some(ui) = super::ui() {
                    ui.show_signin(true, None);
                }
            }
        });
        foot
    }

    fn show(&self, channel: Option<&Channel>) {
        self.signed_in.set(channel.is_some());
        self.chev.set_visible(channel.is_some());
        match channel {
            Some(c) => {
                self.name.set_text(&c.name);
                self.sub.set_text("YouTube Music");
                self.root.set_tooltip_text(Some(&format!("{} {} · switch channel or sign out", c.name, c.byline)));
                art::into(&self.photo, c.photo.iter().max_by_key(|t| t.width).map(|t| t.url.as_str()));
            }
            None => {
                self.name.set_text("Sign in");
                self.sub.set_text("To see your library");
                self.root.set_tooltip_text(Some("Sign in to YouTube Music"));
                art::into(&self.photo, None);
            }
        }
    }
}

/// The channels this Google account can use, the active one checked, and Sign out.
fn channels_popover() -> gtk::Popover {
    let pop = gtk::Popover::new();
    pop.add_css_class("acct-menu");
    let body = gtk::Box::new(gtk::Orientation::Vertical, 2);
    body.append(&nyx_ui::widgets::label("SWITCH CHANNEL", "nav-group"));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(360)
        .child(&list)
        .build();
    body.append(&scroll);
    let loading = gtk::Spinner::new();
    loading.start();
    list.append(&loading);
    body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let out = gtk::Button::with_label("Sign out of YouTube Music");
    out.add_css_class("flat");
    out.add_css_class("menu-row");
    let p2 = pop.clone();
    out.connect_clicked(move |_| {
        p2.popdown();
        crate::cache::clear();
        if let Some(p) = player() {
            p.sign_out();
        }
    });
    body.append(&out);
    pop.set_child(Some(&body));
    if let Some(p) = player() {
        let (list, pop) = (list.clone(), pop.clone());
        p.request(
            &youtube::accounts_list(),
            Box::new(move |r| {
                clear(&list);
                let channels = r.ok().map(|v| youtube::channels(&v)).unwrap_or_default();
                if channels.is_empty() {
                    list.append(&nyx_ui::widgets::label("YouTube Music didn't list your channels", "dim-label"));
                }
                for c in channels {
                    list.append(&channel_row(&c, &pop));
                }
            }),
        );
    }
    pop
}

fn channel_row(c: &Channel, pop: &gtk::Popover) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("flat");
    b.add_css_class("menu-row");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let photo = nyx_ui::media::Artwork::new(22, 22, "lyra-user-symbolic");
    photo.root.add_css_class("avatar");
    art::into(&photo, c.photo.iter().min_by_key(|t| t.width).map(|t| t.url.as_str()));
    row.append(&photo.root);
    let name = nyx_ui::widgets::label(&c.name, "");
    name.set_xalign(0.0);
    name.set_hexpand(true);
    row.append(&name);
    if c.selected {
        row.append(&gtk::Image::from_icon_name("lyra-check-symbolic"));
    }
    b.set_child(Some(&row));
    b.set_sensitive(c.switch.is_some() && !c.selected);
    if let Some(path) = c.switch.clone() {
        let pop = pop.clone();
        b.connect_clicked(move |_| {
            pop.popdown();
            super::switch_channel(path.clone());
        });
    }
    b
}

#[cfg(test)]
mod tests {
    use super::song_count;

    #[test]
    fn liked_music_counts() {
        assert_eq!(song_count("1,284 songs • 3+ hours"), Some(1284));
        assert_eq!(song_count("Auto playlist • 1 song"), Some(1));
        assert_eq!(song_count("Playlist • 2026"), None);
    }
}
