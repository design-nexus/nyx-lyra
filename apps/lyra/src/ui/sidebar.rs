//! The sidebar: Home, Explore and Search; the library; the playlists from YouTube Music.

use super::{Route, Tab, navigate};
use crate::{art, player};
use gtk::{glib, prelude::*};
use nyx_lyra_core::model::Kind;
use nyx_lyra_core::youtube;
use nyx_ui::sidebar::{self, NavItem, Sidebar};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Side {
    pub side: Rc<Sidebar>,
    items: Vec<(Route, NavItem)>,
    playlists: gtk::Box,
    playlist_items: Rc<RefCell<Vec<(String, gtk::Button)>>>,
}

impl Side {
    pub fn new(_window: &gtk::ApplicationWindow) -> Self {
        let side = Sidebar::new();
        let mut items = Vec::new();
        let mut add = |route: Route, icon: &str, title: &str, tip: &str| {
            let n = sidebar::nav_item(icon, title, tip);
            let r = route.clone();
            n.button.connect_clicked(move |_| navigate(r.clone()));
            side.append(&n.button);
            items.push((route, n));
        };
        add(Route::Home, "go-home-symbolic", "Home", "Home");
        add(Route::Explore, "find-location-symbolic", "Explore", "New releases, charts, moods and genres");
        add(Route::Search(String::new()), "system-search-symbolic", "Search", "Search (Ctrl+F)");
        side.append(&sidebar::group_label("Your library", false));
        add(Route::Library(Tab::Playlists), "folder-music-symbolic", "Library", "Your playlists, albums, artists and songs");
        add(Route::Liked, "emblem-favorite-symbolic", "Liked Music", "Songs you liked");
        add(Route::Library(Tab::Artists), "avatar-default-symbolic", "Artists", "Artists in your library");
        add(Route::Library(Tab::Albums), "media-optical-symbolic", "Albums", "Albums you saved");
        add(Route::Library(Tab::Songs), "audio-x-generic-symbolic", "Songs", "Songs in your library");
        side.append(&sidebar::group_label("Playlists", false));
        let playlists = gtk::Box::new(gtk::Orientation::Vertical, 1);
        side.append(&playlists);
        let s = Self { side, items, playlists, playlist_items: Rc::default() };
        s.reload();
        s
    }

    /// The playlists, from YouTube Music's library (once the page is ready).
    pub fn reload(&self) {
        let playlists = self.playlists.clone();
        let rows = self.playlist_items.clone();
        let side = self.side.clone();
        let try_load = Rc::new(RefCell::new(None::<Box<dyn Fn()>>));
        let again = try_load.clone();
        *try_load.borrow_mut() = Some(Box::new(move || {
            let Some(p) = player() else { return };
            let Some(req) = youtube::browse(youtube::LIBRARY_PLAYLISTS) else { return };
            let (playlists, rows, side, again) = (playlists.clone(), rows.clone(), side.clone(), again.clone());
            p.request(
                &req,
                Box::new(move |r| {
                    let page = match r.ok().map(|v| youtube::page(&v)) {
                        Some(Ok(p)) => p,
                        _ => {
                            // Not ready yet, or offline: look again in a while.
                            glib::timeout_add_local_once(std::time::Duration::from_secs(4), move || {
                                if let Some(f) = again.borrow().as_ref() {
                                    f()
                                }
                            });
                            return;
                        }
                    };
                    while let Some(c) = playlists.first_child() {
                        playlists.remove(&c);
                    }
                    rows.borrow_mut().clear();
                    for item in page.items().filter(|i| i.kind == Kind::Playlist && i.browse_id.is_some()) {
                        let id = item.browse_id.clone().unwrap_or_default();
                        let b = gtk::Button::new();
                        b.add_css_class("nav-item");
                        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                        let a = nyx_ui::media::Artwork::new(20, 20, "view-list-symbolic");
                        a.root.add_css_class("nav-art");
                        art::into(&a, item.thumbnail(40).map(|t| t.url.as_str()));
                        content.append(&a.root);
                        let l = nyx_ui::widgets::label(&item.title, "nav-label");
                        l.add_css_class("compact-hide");
                        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                        l.set_xalign(0.0);
                        l.set_hexpand(true);
                        content.append(&l);
                        b.set_child(Some(&content));
                        b.set_tooltip_text(Some(&item.title));
                        let route = Route::Browse { id: id.clone(), title: item.title.clone() };
                        b.connect_clicked(move |_| navigate(route.clone()));
                        playlists.append(&b);
                        rows.borrow_mut().push((id, b));
                    }
                    side.refresh();
                }),
            );
        }));
        // The page may still be loading: start a moment after.
        glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
            if let Some(f) = try_load.borrow().as_ref() {
                f()
            }
        });
    }

    pub fn select(&self, route: &Route) {
        for (r, n) in &self.items {
            let on = match (r, route) {
                (Route::Search(_), Route::Search(_)) => true,
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
}
