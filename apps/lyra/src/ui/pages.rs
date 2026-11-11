//! The pages: a route's YouTube Music page drawn as Lyra's own (direction A): a header for
//! albums, playlists and artists, tile grids for albums, playlists and artists, song grids
//! for Home's songs, track lists for an album's or playlist's songs, and "Load more" for
//! long lists. A page opens from the cache and is replaced by YouTube Music's answer.

use super::{Route, Tab, activate, navigate, play};
use crate::{art, cache, player, prefs};
use gtk::{glib, prelude::*};
use nyx_lyra_core::model::{Item, Kind, Page, Shelf};
use nyx_lyra_core::youtube::{self, Request};
use nyx_ui::{media::Artwork, widgets};
use std::rc::Rc;

const TILE: i32 = 156;

fn request(route: &Route) -> Option<Request> {
    match route {
        Route::Home => youtube::browse(youtube::HOME),
        Route::Explore => youtube::browse(youtube::EXPLORE),
        Route::Search(q) => youtube::search(q),
        Route::Liked => youtube::browse(youtube::LIKED_MUSIC),
        Route::Library(t) => youtube::browse(match t {
            Tab::Playlists => youtube::LIBRARY_PLAYLISTS,
            Tab::Albums => youtube::LIBRARY_ALBUMS,
            Tab::Artists => youtube::LIBRARY_ARTISTS,
            Tab::Songs => youtube::LIBRARY_SONGS,
        }),
        Route::Browse { id, .. } => youtube::browse(id),
    }
}

fn cache_key(route: &Route) -> String {
    match route {
        Route::Search(q) => format!("search:{}", q.to_lowercase()),
        r => request(r).map(|r| r.body.to_string()).unwrap_or_default(),
    }
}

pub fn open(route: &Route) -> gtk::Widget {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("page-body");
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).hexpand(true).child(&body).build();
    if let Route::Library(tab) = route {
        body.append(&library_tabs(*tab));
    }
    if let Route::Search(q) = route {
        if q.is_empty() {
            recent_searches(&body);
            return scroll.upcast();
        }
    }
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&content);
    let key = cache_key(route);
    let cached = cache::get(&key);
    match &cached {
        Some(p) => render(&content, p, route),
        None => content.append(&loading()),
    }
    let Some(req) = request(route) else {
        content.append(&widgets::empty_state("dialog-warning-symbolic", "This page can't be opened", "", None));
        return scroll.upcast();
    };
    let route = route.clone();
    let had = cached.is_some();
    fetch(&req, move |r| {
        match r {
            Ok(page) => {
                cache::put(&key, &page);
                clear(&content);
                render(&content, &page, &route);
            }
            Err(why) if !had => {
                clear(&content);
                let retry = route.clone();
                let state = widgets::empty_state(
                    "network-offline-symbolic",
                    "YouTube Music didn't answer",
                    &why,
                    Some(("Try again", Box::new(move || navigate(retry.clone())))),
                );
                content.append(&state);
            }
            Err(why) => eprintln!("nyx-lyra: kept the cached page: {why}"),
        }
    });
    scroll.upcast()
}

/// A request, read into a page.
fn fetch(req: &Request, done: impl FnOnce(Result<Page, String>) + 'static) {
    let Some(p) = player() else { return done(Err("Lyra isn't ready".into())) };
    p.request(
        req,
        Box::new(move |r| {
            done(match r {
                Ok(v) if youtube::logged_in(&v) == Some(false) => Err("Sign in to YouTube Music to see this".into()),
                Ok(v) => youtube::page(&v).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            })
        }),
    );
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn loading() -> gtk::Widget {
    let s = gtk::Spinner::new();
    s.start();
    s.set_size_request(32, 32);
    s.set_halign(gtk::Align::Center);
    s.set_margin_top(80);
    s.upcast()
}

fn render(b: &gtk::Box, page: &Page, route: &Route) {
    if let Some(h) = &page.header {
        b.append(&header(h, page));
    }
    let detail = page.header.is_some() || matches!(route, Route::Liked | Route::Library(Tab::Songs));
    let browsing = matches!(route, Route::Home | Route::Explore);
    let mut shown = 0;
    for shelf in &page.shelves {
        let items: Vec<&Item> = shelf.items.iter().filter(|i| i.kind != Kind::Other).collect();
        if items.is_empty() {
            continue;
        }
        shown += 1;
        if !shelf.title.is_empty() {
            b.append(&shelf_head(shelf));
        }
        let songs = items.iter().filter(|i| matches!(i.kind, Kind::Song | Kind::Video)).count() * 2 > items.len();
        if shelf.title == "Top result" {
            b.append(&top_result(&items));
        } else if songs && detail {
            b.append(&track_list(&items, page.header.as_ref().is_some_and(|h| h.subtitle.starts_with("Album") || h.subtitle.starts_with("Single") || h.subtitle.starts_with("EP"))));
        } else if songs || shelf.title == youtube::RESULTS {
            b.append(&song_grid(&items, if browsing { 12 } else { usize::MAX }, if shelf.title == youtube::RESULTS { 1 } else { 3 }));
        } else {
            b.append(&tile_grid(&items, if browsing { 12 } else { usize::MAX }));
        }
    }
    if let Some(token) = &page.continuation {
        b.append(&load_more(b, token.clone(), detail));
    }
    if shown == 0 && page.header.is_none() {
        let (title, desc) = match route {
            Route::Liked => ("No liked songs yet", "Songs you like in Lyra or anywhere on YouTube Music show up here."),
            Route::Library(Tab::Albums) => ("No saved albums", "Albums you save on YouTube Music show up here."),
            Route::Library(Tab::Artists) => ("No artists yet", "Artists you follow or play from your library show up here."),
            Route::Search(_) => ("Nothing found", "Try other words."),
            _ => ("Nothing here yet", ""),
        };
        b.append(&widgets::empty_state("folder-music-symbolic", title, desc, None));
    }
}

fn load_more(b: &gtk::Box, token: String, detail: bool) -> gtk::Widget {
    let button = gtk::Button::with_label("Load more");
    button.add_css_class("load-more");
    button.set_halign(gtk::Align::Center);
    let body = b.clone();
    button.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        btn.set_label("Loading…");
        let Some(req) = youtube::more(&token) else { return };
        let (body, btn) = (body.clone(), btn.clone());
        fetch(&req, move |r| {
            body.remove(&btn);
            match r {
                Ok(p) => {
                    let items: Vec<&Item> = p.items().filter(|i| i.kind != Kind::Other).collect();
                    body.append(&if detail { track_list(&items, false) } else { tile_grid(&items, usize::MAX) });
                    if let Some(t) = &p.continuation {
                        body.append(&load_more(&body, t.clone(), detail));
                    }
                }
                Err(why) => body.append(&widgets::banner(&format!("Couldn't load more: {why}"), true)),
            }
        });
    });
    button.upcast()
}

/* ── Headers ───────────────────────────────────────────────────────────────────── */

fn header(h: &nyx_lyra_core::model::Header, page: &Page) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    row.add_css_class("detail-head");
    let art = Artwork::new(200, 200, "folder-music-symbolic");
    art.root.add_css_class("detail-art");
    art::into(&art, h.thumbnails.iter().max_by_key(|t| t.width).map(|t| t.url.as_str()));
    row.append(&art.root);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 6);
    text.set_valign(gtk::Align::End);
    text.set_hexpand(true);
    if !h.subtitle.is_empty() {
        text.append(&left(&h.subtitle, "detail-kind"));
    }
    let title = left(&h.title, "detail-title");
    title.set_wrap(true);
    text.append(&title);
    if !h.second_subtitle.is_empty() {
        text.append(&left(&h.second_subtitle, "dim-label"));
    }
    if !h.description.is_empty() {
        let d = left(&h.description, "detail-desc");
        d.set_wrap(true);
        d.set_lines(3);
        d.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&d);
    }
    if let Some(first) = page.items().find(|i| i.playable() && matches!(i.kind, Kind::Song | Kind::Video)).cloned() {
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions.set_margin_top(10);
        let play_button = widgets::labeled_button("media-playback-start-symbolic", "Play");
        play_button.add_css_class("suggested-action");
        play_button.connect_clicked(move |_| play(&first));
        actions.append(&play_button);
        text.append(&actions);
    }
    row.append(&text);
    row.upcast()
}

fn left(text: &str, class: &str) -> gtk::Label {
    let l = widgets::label(text, class);
    l.set_xalign(0.0);
    l
}

fn shelf_head(s: &Shelf) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("shelf-head");
    let t = left(&s.title, "shelf-title");
    t.set_hexpand(true);
    row.append(&t);
    if let Some(id) = s.more.clone() {
        let more = gtk::Button::with_label("More");
        more.add_css_class("flat");
        let title = s.title.clone();
        more.connect_clicked(move |_| navigate(Route::Browse { id: id.clone(), title: title.clone() }));
        row.append(&more);
    }
    row.upcast()
}

/* ── Tiles, songs, track lists ─────────────────────────────────────────────────── */

fn thumb(item: &Item, size: i32) -> Artwork {
    let art = Artwork::new(size, size, if item.kind == Kind::Artist { "avatar-default-symbolic" } else { "folder-music-symbolic" });
    if item.kind == Kind::Artist {
        art.root.add_css_class("round");
    }
    art::into(&art, item.thumbnail(size as u32 * 2).map(|t| t.url.as_str()));
    art
}

fn kind_word(item: &Item) -> &'static str {
    match item.kind {
        Kind::Song => "Song",
        Kind::Video => "Video",
        Kind::Album => "Album",
        Kind::Single => "Single",
        Kind::Ep => "EP",
        Kind::Artist => "Artist",
        Kind::Playlist => "Playlist",
        Kind::Other => "",
    }
}

fn byline(item: &Item) -> String {
    let artists = item.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    if !artists.is_empty() { artists } else { item.subtitle.clone() }
}

fn tile(item: &Item) -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("tile");
    b.add_css_class("flat");
    let col = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let art = thumb(item, TILE);
    let hover = nyx_ui::media::hover_play();
    art.root.add_overlay(&hover);
    col.append(&art.root);
    let t = left(&item.title, "tile-title");
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    t.set_max_width_chars(18);
    col.append(&t);
    let sub = left(&item.subtitle, "tile-sub");
    sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    sub.set_max_width_chars(18);
    col.append(&sub);
    b.set_child(Some(&col));
    b.set_tooltip_text(Some(&item.title));
    let it = item.clone();
    b.connect_clicked(move |_| activate(&it));
    b.upcast()
}

fn tile_grid(items: &[&Item], max: usize) -> gtk::Widget {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .column_spacing(16)
        .row_spacing(18)
        .min_children_per_line(2)
        .max_children_per_line(8)
        .build();
    flow.add_css_class("tile-grid");
    for item in items.iter().take(max) {
        flow.insert(&tile(item), -1);
    }
    flow.upcast()
}

fn song_row(item: &Item, number: Option<usize>, album_column: bool) -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("song-row");
    b.add_css_class("flat");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    match number {
        Some(n) => {
            let l = widgets::label(&n.to_string(), "song-number");
            l.set_width_chars(3);
            row.append(&l);
        }
        None => row.append(&thumb(item, 40).root),
    }
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let t = left(&item.title, "song-title");
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.append(&t);
    if item.explicit {
        let e = widgets::label("E", "explicit");
        e.set_tooltip_text(Some("Explicit"));
        title.append(&e);
    }
    text.append(&title);
    let sub = match item.kind {
        Kind::Song | Kind::Video if number.is_some() => byline(item),
        Kind::Song | Kind::Video => format!("{} • {}", kind_word(item), byline(item)),
        _ => item.subtitle.clone(),
    };
    let s = left(&sub, "song-sub");
    s.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&s);
    row.append(&text);
    if album_column {
        let a = left(item.album.as_ref().map_or("", |a| a.name.as_str()), "song-album");
        a.set_ellipsize(gtk::pango::EllipsizeMode::End);
        a.set_width_chars(22);
        a.set_max_width_chars(22);
        row.append(&a);
    }
    if let Some(d) = item.duration {
        row.append(&widgets::label(&super::clock(d as f64), "song-duration"));
    }
    b.set_child(Some(&row));
    let it = item.clone();
    b.connect_clicked(move |_| activate(&it));
    b.upcast()
}

fn song_grid(items: &[&Item], max: usize, columns: u32) -> gtk::Widget {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .column_spacing(12)
        .row_spacing(2)
        .min_children_per_line(columns.min(2))
        .max_children_per_line(columns)
        .build();
    flow.add_css_class("song-grid");
    for item in items.iter().take(max) {
        flow.insert(&song_row(item, None, false), -1);
    }
    flow.upcast()
}

fn track_list(items: &[&Item], album: bool) -> gtk::Widget {
    let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    list.add_css_class("track-list");
    for (i, item) in items.iter().enumerate() {
        list.append(&song_row(item, album.then_some(i + 1), !album));
    }
    list.upcast()
}

fn top_result(items: &[&Item]) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    row.add_css_class("top-result-row");
    let Some(first) = items.first() else { return row.upcast() };
    let card = gtk::Button::new();
    card.add_css_class("top-result");
    card.add_css_class("card");
    let col = gtk::Box::new(gtk::Orientation::Vertical, 12);
    col.append(&thumb(first, 112).root);
    col.append(&left(&first.title, "top-title"));
    col.append(&left(&first.subtitle, "dim-label"));
    card.set_child(Some(&col));
    let it = (*first).clone();
    card.connect_clicked(move |_| activate(&it));
    row.append(&card);
    let rest: Vec<&Item> = items.iter().skip(1).copied().collect();
    if !rest.is_empty() {
        let g = song_grid(&rest, 4, 1);
        g.set_hexpand(true);
        row.append(&g);
    }
    row.upcast()
}

/* ── Library and search extras ─────────────────────────────────────────────────── */

fn library_tabs(current: Tab) -> gtk::Widget {
    let opts = widgets::opts(&[("playlists", "Playlists"), ("albums", "Albums"), ("artists", "Artists"), ("songs", "Songs")]);
    let now = match current {
        Tab::Playlists => "playlists",
        Tab::Albums => "albums",
        Tab::Artists => "artists",
        Tab::Songs => "songs",
    };
    let seg = widgets::segmented(&opts, now, |v| {
        let tab = match v.as_str() {
            "albums" => Tab::Albums,
            "artists" => Tab::Artists,
            "songs" => Tab::Songs,
            _ => Tab::Playlists,
        };
        glib::idle_add_local_once(move || navigate(Route::Library(tab)));
    });
    seg.set_halign(gtk::Align::Start);
    seg.set_margin_bottom(14);
    seg.upcast()
}

fn recent_searches(b: &gtk::Box) {
    let recent = prefs::get().recent_searches;
    if recent.is_empty() {
        b.append(&widgets::empty_state("system-search-symbolic", "Search YouTube Music", "Songs, albums, artists and playlists. Ctrl+F from anywhere.", None));
        return;
    }
    b.append(&left("Recent searches", "shelf-title"));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    for q in recent {
        let row = gtk::Button::new();
        row.add_css_class("flat");
        row.add_css_class("song-row");
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        content.append(&gtk::Image::from_icon_name("document-open-recent-symbolic"));
        content.append(&left(&q, ""));
        row.set_child(Some(&content));
        let query = Rc::new(q);
        row.connect_clicked(move |_| navigate(Route::Search((*query).clone())));
        list.append(&row);
    }
    b.append(&list);
}
