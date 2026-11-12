//! The pages: a route's YouTube Music page drawn as Lyra's own (direction A of the
//! mockups): Home's moods, shelves of tiles or songs with More and arrows, the featured
//! card, headers for albums, playlists and artists, track lists, the library with its tabs,
//! sort and view. Long pages load more as they scroll. A page opens from the cache and is
//! replaced by YouTube Music's answer.

use super::grid::{self, Spec, Strip};
use super::{Route, Tab, activate, navigate, play};
use crate::{art, cache, player, prefs};
use gtk::{glib, prelude::*};
use nyx_lyra_core::model::{Chip, Item, Kind, Page, Shelf};
use nyx_lyra_core::youtube::{self, Request};
use nyx_ui::{media::Artwork, widgets};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn request(route: &Route) -> Option<Request> {
    match route {
        Route::Home => youtube::browse(youtube::HOME),
        Route::Mood { id, params, .. } => youtube::browse_with(id, params),
        Route::Explore => youtube::browse(youtube::EXPLORE),
        Route::Search(q) => youtube::search(q),
        Route::SearchFilter { query, params, .. } => youtube::search_with(query, params),
        Route::Liked => youtube::browse(youtube::LIKED_MUSIC),
        Route::History => youtube::browse(youtube::HISTORY),
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
        Route::SearchFilter { query, title, .. } => format!("search:{}:{title}", query.to_lowercase()),
        r => request(r).map(|r| r.body.to_string()).unwrap_or_default(),
    }
}

/// Home, Explore and their moods: shelves a page at a time, as the mockup's.
fn browsing(route: &Route) -> bool {
    matches!(route, Route::Home | Route::Mood { .. } | Route::Explore)
}

/// Home (or Home with a mood), where the first shelf shows two rows.
fn home(route: &Route) -> bool {
    matches!(route, Route::Home) || matches!(route, Route::Mood { id, .. } if id == youtube::HOME)
}

/// What's drawn so far and how to go on: the continuation, and where more goes.
struct Pager {
    content: gtk::Box,
    next: RefCell<Option<String>>,
    loading: Cell<bool>,
    route: Route,
    detail: Cell<bool>,
    /// Rows numbered so far, so a long list's next part goes on counting.
    counted: Cell<usize>,
}

pub fn open(route: &Route) -> gtk::Widget {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("page-body");
    let scroll =
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).hexpand(true).child(&body).build();
    if let Route::Search(q) = route
        && q.is_empty()
    {
        recent_searches(&body);
        return scroll.upcast();
    }
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    if let Route::Library(tab) = route {
        body.append(&library_bar(*tab));
    }
    body.append(&content);
    let pager = Rc::new(Pager {
        content: content.clone(),
        next: RefCell::default(),
        loading: Cell::new(false),
        route: route.clone(),
        detail: Cell::new(false),
        counted: Cell::new(0),
    });
    // More as the page reaches its end.
    let p = pager.clone();
    scroll.connect_edge_reached(move |_, edge| {
        if edge == gtk::PositionType::Bottom {
            load_next(&p);
        }
    });
    let key = cache_key(route);
    let cached = cache::get(&key);
    match &cached {
        Some(page) => render(&pager, page, true),
        None => content.append(&loading()),
    }
    let Some(req) = request(route) else {
        content.append(&widgets::empty_state("dialog-warning-symbolic", "This page can't be opened", "", None));
        return scroll.upcast();
    };
    let had = cached.is_some();
    // Signed out, YouTube Music still has Home, Explore and search; the library needs an account.
    let needs_account = matches!(route, Route::Library(_) | Route::Liked | Route::History);
    let route = route.clone();
    fetch(&req, needs_account, move |r| match r {
        Ok(page) => {
            cache::put(&key, &page);
            clear(&pager.content);
            pager.counted.set(0);
            render(&pager, &page, true);
        }
        Err(why) if why == SIGNED_OUT => {
            clear(&pager.content);
            pager.content.append(&widgets::empty_state(
                "lyra-library-symbolic",
                "Sign in to see your library",
                "Your playlists, albums, artists and liked songs come from your YouTube Music account.",
                Some((
                    "Sign in",
                    Box::new(|| {
                        if let Some(ui) = crate::ui::ui() {
                            ui.show_signin(true, None);
                        }
                    }),
                )),
            ));
        }
        Err(why) if !had => {
            clear(&pager.content);
            let retry = route.clone();
            pager.content.append(&widgets::empty_state(
                "lyra-wifioff-symbolic",
                "YouTube Music didn't answer",
                &why,
                Some(("Try again", Box::new(move || navigate(retry.clone())))),
            ));
        }
        Err(why) => eprintln!("nyx-lyra: kept the cached page: {why}"),
    });
    scroll.upcast()
}

const SIGNED_OUT: &str = "Sign in to YouTube Music to see this";

/// A request, read into a page.
fn fetch(req: &Request, needs_account: bool, done: impl FnOnce(Result<Page, String>) + 'static) {
    let Some(p) = player() else { return done(Err("Lyra isn't ready".into())) };
    p.request(
        req,
        Box::new(move |r| {
            done(match r {
                Ok(v) if needs_account && youtube::logged_in(&v) == Some(false) => Err(SIGNED_OUT.into()),
                Err(crate::runtime::RequestError::SignedOut) => Err(SIGNED_OUT.into()),
                Ok(v) => youtube::page(&v).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            })
        }),
    );
}

fn load_next(pager: &Rc<Pager>) {
    if pager.loading.get() {
        return;
    }
    let Some(token) = pager.next.borrow_mut().take() else { return };
    let Some(req) = youtube::more(&token) else { return };
    pager.loading.set(true);
    let spinner = loading();
    pager.content.append(&spinner);
    let p = pager.clone();
    fetch(&req, false, move |r| {
        p.loading.set(false);
        p.content.remove(&spinner);
        match r {
            Ok(page) => render(&p, &page, false),
            Err(why) => p.content.append(&widgets::banner(&format!("Couldn't load more: {why}"), true)),
        }
    });
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
    s.set_margin_top(40);
    s.upcast()
}

/// Draw `page` into the pager's content: the whole page when `first`, else the next part
/// of a long one.
fn render(pager: &Rc<Pager>, page: &Page, first: bool) {
    let b = &pager.content;
    let route = &pager.route;
    *pager.next.borrow_mut() = page.continuation.clone();
    if first {
        if let Some(h) = &page.header {
            b.append(&header(h, page));
        }
        if !page.chips.is_empty() && home(route) {
            b.append(&mood_chips(&page.chips));
        }
        if let Route::Search(q) | Route::SearchFilter { query: q, .. } = route
            && !page.chips.is_empty()
        {
            let on = match route {
                Route::SearchFilter { title, .. } => Some(title.as_str()),
                _ => None,
            };
            b.append(&search_chips(q, &page.chips, on));
        }
        if let Route::Library(_) = route
            && !page.sorts.is_empty()
            && let Some(slot) = library_sort_slot(b)
        {
            // The cached page drew one already; the fresh one replaces it.
            clear(&slot);
            slot.append(&sort_menu(&page.sorts, pager));
        }
        pager.detail.set(page.header.is_some() || matches!(route, Route::Liked | Route::Library(Tab::Songs) | Route::History));
    }
    let detail = pager.detail.get();
    let mut shown = 0;
    let mut tile_shelves = 0;
    let grouped;
    let shelves = if let Route::Search(_) = route {
        grouped = group_results(&page.shelves);
        &grouped
    } else {
        &page.shelves
    };
    for shelf in shelves {
        let items: Vec<&Item> = shelf.items.iter().filter(|i| i.kind != Kind::Other).collect();
        if items.is_empty() {
            continue;
        }
        shown += 1;
        let songs = !shelf.tiles && items.iter().filter(|i| matches!(i.kind, Kind::Song | Kind::Video)).count() * 2 > items.len();
        let links = items.iter().all(|i| i.kind == Kind::Link);
        if links {
            let colored = items.iter().any(|i| i.color.is_some());
            let strip = if colored {
                Strip::new(Spec { min: 150, max_cols: 4, gap_x: 10, gap_y: 10, rows: browsing(route).then_some(2) })
            } else {
                Strip::new(Spec { min: 170, max_cols: 3, gap_x: 10, gap_y: 10, rows: Some(1) })
            };
            for item in &items {
                strip.append(&if colored { mood_tile(item) } else { link_card(item) });
            }
            if !shelf.title.is_empty() {
                b.append(&shelf_head(shelf, None, None));
            } else {
                strip.root.add_css_class("first-strip");
            }
            b.append(&strip.root);
            continue;
        }
        // YouTube Music's card shelf: the search's top result, Home's featured album.
        if shelf.title == youtube::TOP_RESULT && matches!(route, Route::Search(_)) {
            b.append(&shelf_head(shelf, None, None));
            b.append(&top_result(&items));
        } else if shelf.title == youtube::TOP_RESULT {
            b.append(&feature_card(&items));
        } else if let (Route::Search(q), Some(filter)) = (route, search_group(&shelf.title)) {
            // The search's results, a section per kind; More shows that filter.
            let chip = page.chips.iter().find(|c| c.title.starts_with(filter)).cloned();
            if songs {
                b.append(&group_head(&shelf.title, q, chip, None));
                let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
                for item in items.iter().take(4) {
                    list.append(&song_row(item, None, false));
                }
                b.append(&list);
            } else {
                let strip = Strip::new(Spec { rows: Some(1), ..grid::TILES });
                for item in &items {
                    strip.append(&tile(item));
                }
                b.append(&group_head(&shelf.title, q, chip, Some(&strip)));
                b.append(&strip.root);
            }
        } else if songs && page.header.as_ref().is_some_and(|h| h.artist.is_some()) {
            // An artist's top songs: two columns, two rows, "Show all".
            let strip = Strip::new(Spec { min: 300, max_cols: 2, gap_x: 24, gap_y: 2, rows: Some(2) });
            for item in &items {
                strip.append(&artist_song_row(item));
            }
            b.append(&shelf_head(shelf, None, None));
            b.append(&strip.root);
        } else if songs && (detail || shelf.title == youtube::RESULTS) {
            if !shelf.title.is_empty() {
                b.append(&shelf_head(shelf, None, None));
            }
            let album = page.header.as_ref().is_some_and(is_album);
            if page.header.as_ref().is_some_and(|h| h.list.is_some()) || matches!(route, Route::Liked) {
                let playing = player().and_then(|p| p.state().track.and_then(|t| t.video_id));
                let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
                list.add_css_class("track-table");
                if first {
                    list.append(&table_head(album));
                }
                let offset = pager.counted.get();
                for (i, item) in items.iter().enumerate() {
                    list.append(&table_row(item, offset + i + 1, album, playing.as_deref()));
                }
                pager.counted.set(offset + items.len());
                b.append(&list);
            } else {
                b.append(&track_list(&items, album));
            }
        } else if songs && items.iter().any(|i| i.rank.is_some()) {
            // A chart: places and how they moved, two columns, a page of two rows.
            let strip = Strip::new(Spec { min: 300, max_cols: 2, gap_x: 24, gap_y: 2, rows: browsing(route).then_some(2) });
            for item in &items {
                strip.append(&ranked_row(item));
            }
            b.append(&shelf_head(shelf, browsing(route).then_some(&strip), None));
            b.append(&strip.root);
        } else if songs {
            // Quick picks and the like: three columns of songs, a page at a time.
            let strip = Strip::new(Spec { rows: browsing(route).then_some(3), ..grid::SONGS });
            for item in &items {
                strip.append(&song_row(item, None, false));
            }
            let play_all = (shelf.title == "Quick picks").then(|| items.first().map(|i| (*i).clone())).flatten();
            b.append(&shelf_head(shelf, browsing(route).then_some(&strip), play_all));
            b.append(&strip.root);
        } else if library_as_list(route) {
            if !shelf.title.is_empty() {
                b.append(&shelf_head(shelf, None, None));
            }
            let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
            if first && shown == 1 && *route == Route::Library(Tab::Playlists) {
                list.append(&new_playlist_row());
            }
            for item in &items {
                list.append(&song_row(item, None, false));
            }
            b.append(&list);
        } else {
            tile_shelves += 1;
            // On Home the first shelf shows two rows, the rest one; an artist's shelves one;
            // elsewhere every tile.
            let artist = page.header.as_ref().is_some_and(|h| h.artist.is_some());
            let rows = (browsing(route) || artist).then_some(if tile_shelves == 1 && first && home(route) { 2 } else { 1 });
            let strip = Strip::new(Spec { rows, ..grid::TILES });
            if first && shown == 1 && *route == Route::Library(Tab::Playlists) {
                strip.append(&new_playlist_tile());
            }
            for item in &items {
                strip.append(&tile(item));
            }
            if !shelf.title.is_empty() {
                b.append(&shelf_head(shelf, rows.is_some().then_some(&strip), None));
            }
            b.append(&strip.root);
        }
    }
    if first && shown == 0 && page.header.is_none() {
        let (icon, title, desc) = match route {
            Route::Liked => {
                ("lyra-heart-symbolic", "No liked songs yet", "Songs you like in Lyra or anywhere on YouTube Music show up here.")
            }
            Route::History => (
                "lyra-history-symbolic",
                "Nothing played yet",
                "What you play in Lyra or anywhere on YouTube Music shows up here.",
            ),
            Route::Library(Tab::Albums) => {
                ("lyra-disc-symbolic", "No saved albums", "Albums you save on YouTube Music show up here.")
            }
            Route::Library(Tab::Artists) => {
                ("lyra-user-symbolic", "No artists yet", "Artists you follow or play from your library show up here.")
            }
            Route::Search(_) => ("lyra-search-symbolic", "Nothing found", "Try other words."),
            _ => ("lyra-music-symbolic", "Nothing here yet", ""),
        };
        b.append(&widgets::empty_state(icon, title, desc, None));
    }
}

/* ── Home's moods ──────────────────────────────────────────────────────────────── */

fn mood_chips(chips: &[Chip]) -> gtk::Widget {
    // They wrap, as the mockup's do.
    let row = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(6)
        .row_spacing(6)
        .max_children_per_line(30)
        .build();
    row.add_css_class("chips");
    let any = chips.iter().any(|c| c.selected);
    let all = gtk::Button::with_label("All");
    all.add_css_class("chip");
    if !any {
        all.add_css_class("selected");
    }
    all.connect_clicked(|_| navigate(Route::Home));
    row.insert(&all, -1);
    for c in chips {
        let b = gtk::Button::with_label(&c.title);
        b.add_css_class("chip");
        if c.selected {
            b.add_css_class("selected");
        }
        let (title, params, selected) = (c.title.clone(), c.params.clone(), c.selected);
        b.connect_clicked(move |_| match (&params, selected) {
            (Some(p), false) => navigate(Route::Mood { id: youtube::HOME.into(), title: title.clone(), params: p.clone() }),
            _ => navigate(Route::Home),
        });
        row.insert(&b, -1);
    }
    row.upcast()
}

/* ── Headers ───────────────────────────────────────────────────────────────────── */

fn header(h: &nyx_lyra_core::model::Header, page: &Page) -> gtk::Widget {
    if let Some(a) = &h.artist {
        return artist_header(h, a);
    }
    if let Some(l) = &h.list {
        return list_header(h, l, page);
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    row.add_css_class("detail-head");
    let art = Artwork::new(200, 200, "lyra-disc-symbolic");
    art.root.add_css_class("detail-art");
    art::into(&art, h.thumbnails.iter().max_by_key(|t| t.width).map(|t| t.url.as_str()));
    row.append(&art.root);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 6);
    text.set_valign(gtk::Align::End);
    text.set_hexpand(true);
    if !h.subtitle.is_empty() {
        text.append(&left(&dots(&h.subtitle), "detail-kind"));
    }
    let title = left(&h.title, "detail-title");
    title.set_wrap(true);
    text.append(&title);
    if !h.second_subtitle.is_empty() {
        text.append(&left(&dots(&h.second_subtitle), "dim-label"));
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
        let play_button = widgets::labeled_button("lyra-play-symbolic", "Play");
        play_button.add_css_class("suggested-action");
        play_button.connect_clicked(move |_| play(&first));
        actions.append(&play_button);
        text.append(&actions);
    }
    row.append(&text);
    row.upcast()
}

/// An artist's banner, as the mockup's: the photo faded behind, the photo round in the
/// middle, the name, the subscribers, then Shuffle, Radio, Subscribe, share and more.
fn artist_header(h: &nyx_lyra_core::model::Header, a: &nyx_lyra_core::model::ArtistActions) -> gtk::Widget {
    let banner = gtk::Overlay::new();
    banner.add_css_class("artist-banner");
    let url = h.thumbnails.iter().max_by_key(|t| t.width).map(|t| t.url.clone());
    let backdrop = gtk::Picture::new();
    backdrop.set_content_fit(gtk::ContentFit::Cover);
    backdrop.set_can_shrink(true);
    backdrop.add_css_class("artist-backdrop");
    banner.set_child(Some(&backdrop));
    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("artist-fade");
    banner.add_overlay(&fade);
    let col = gtk::Box::new(gtk::Orientation::Vertical, 6);
    col.add_css_class("artist-head");
    let photo = Artwork::new(108, 108, "lyra-user-symbolic");
    photo.root.add_css_class("round");
    photo.root.set_halign(gtk::Align::Center);
    col.append(&photo.root);
    if let Some(u) = &url {
        let (b, p2) = (backdrop.clone(), photo.clone());
        art::load(u, move |tex| {
            b.set_paintable(Some(tex));
            p2.set_texture(Some(tex));
        });
    }
    let name = left(&h.title, "artist-name");
    col.append(&name);
    let line = [a.subscribers.as_str(), h.second_subtitle.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if !line.is_empty() {
        col.append(&left(&dots(&line), "dim-label"));
    }
    let acts = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    acts.set_margin_top(8);
    let play_list = |list: Option<String>| {
        move |_: &gtk::Button| {
            if let (Some(l), Some(p)) = (list.clone(), player()) {
                p.play_ids(None, Some(l));
            }
        }
    };
    if a.shuffle.is_some() {
        let b = widgets::labeled_button("lyra-shuffle-symbolic", "Shuffle");
        b.add_css_class("suggested-action");
        b.connect_clicked(play_list(a.shuffle.clone()));
        acts.append(&b);
    }
    if a.radio.is_some() {
        let b = widgets::labeled_button("lyra-radio-symbolic", "Radio");
        b.connect_clicked(play_list(a.radio.clone()));
        acts.append(&b);
    }
    if let (Some(channel), Some(subscribed)) = (a.channel_id.clone(), a.subscribed) {
        let b = gtk::Button::new();
        let on = Rc::new(Cell::new(subscribed));
        let show = {
            let b = b.clone();
            move |on: bool| {
                let (icon, text) = if on { ("lyra-check-symbolic", "Subscribed") } else { ("lyra-plus-symbolic", "Subscribe") };
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                content.append(&gtk::Image::from_icon_name(icon));
                content.append(&gtk::Label::new(Some(text)));
                b.set_child(Some(&content));
            }
        };
        show(subscribed);
        b.connect_clicked(move |b| {
            let want = !on.get();
            let (Some(req), Some(p)) = (youtube::subscribe(&channel, want), player()) else { return };
            b.set_sensitive(false);
            let (b2, on2, show2) = (b.clone(), on.clone(), show.clone());
            p.request(
                &req,
                Box::new(move |r| {
                    b2.set_sensitive(true);
                    if r.is_ok() {
                        on2.set(want);
                        show2(want);
                    } else if let Some(ui) = super::ui() {
                        nyx_ui::toast::show(&ui.overlay, "YouTube Music didn't change the subscription");
                    }
                }),
            );
        });
        acts.append(&b);
    }
    if let Some(channel) = a.channel_id.clone() {
        let share = widgets::icon_button("lyra-share-symbolic", "Copy the link");
        share.connect_clicked(move |b| {
            b.clipboard().set_text(&format!("https://music.youtube.com/channel/{channel}"));
            if let Some(ui) = super::ui() {
                nyx_ui::toast::show(&ui.overlay, "Link copied");
            }
        });
        acts.append(&share);
    }
    col.append(&acts);
    banner.add_overlay(&col);
    banner.upcast()
}

/// An album's or playlist's header, as the mockup's: the cover; the kind, the title, who
/// made it (photo and name), the count and length, a playlist's description; then Play,
/// Shuffle, Save to library, share and more.
fn list_header(h: &nyx_lyra_core::model::Header, l: &nyx_lyra_core::model::ListActions, page: &Page) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    row.add_css_class("detail-head");
    let art = Artwork::new(200, 200, "lyra-disc-symbolic");
    art.root.add_css_class("detail-art");
    let liked = h.title == "Liked Music" && h.subtitle.starts_with("Auto playlist");
    if !liked {
        art::into(&art, h.thumbnails.iter().max_by_key(|t| t.width).map(|t| t.url.as_str()));
    }
    if liked {
        // YouTube Music gives Liked Music no cover; the mockup's is a heart on the accent.
        art.root.add_css_class("liked-art");
        art.placeholder.set_icon_name(Some("lyra-heartf-symbolic"));
        art.placeholder.set_pixel_size(72);
    }
    row.append(&art.root);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 6);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let album = is_album(h);
    // The kind line without the maker, who has their own line.
    let mut kind = dots(&h.subtitle);
    if let Some(by) = &l.by {
        kind = kind.trim_end_matches(&format!(" · {}", by.name)).to_string();
    }
    text.append(&left(&kind, "detail-kind"));
    let title = left(&h.title, "detail-title");
    title.set_wrap(true);
    text.append(&title);
    if let Some(by) = l.by.clone() {
        let who = gtk::Button::new();
        who.add_css_class("flat");
        who.add_css_class("by-link");
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let photo = Artwork::new(22, 22, "lyra-user-symbolic");
        photo.root.add_css_class("round");
        art::into(&photo, l.by_photo.iter().min_by_key(|t| t.width).map(|t| t.url.as_str()));
        r.append(&photo.root);
        r.append(&left(&by.name, "by-name"));
        who.set_child(Some(&r));
        who.set_halign(gtk::Align::Start);
        if let Some(id) = by.browse_id.clone() {
            who.connect_clicked(move |_| navigate(Route::Browse { id: id.clone(), title: by.name.clone() }));
        }
        text.append(&who);
    }
    if !h.second_subtitle.is_empty() {
        text.append(&left(&dots(&h.second_subtitle), "dim-label"));
    }
    if !album && !h.description.is_empty() {
        let d = left(h.description.lines().next().unwrap_or_default(), "detail-desc");
        d.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&d);
    }
    let acts = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    acts.set_margin_top(8);
    let first = page.items().find(|i| i.playable() && matches!(i.kind, Kind::Song | Kind::Video)).cloned();
    // A playlist's own list is its page's ID without "VL".
    let own = super::ui().and_then(|u| u.history.borrow().last().cloned()).and_then(|r| match r {
        Route::Browse { id, .. } => id.strip_prefix("VL").filter(|l| youtube::valid_id(l)).map(String::from),
        Route::Liked => Some("LM".to_string()),
        _ => None,
    });
    let play_list = l.play.clone().or(own);
    let play_b = widgets::labeled_button("lyra-play-symbolic", "Play");
    play_b.add_css_class("suggested-action");
    let play_list_c = play_list.clone();
    play_b.connect_clicked(move |_| match (&play_list_c, &first, player()) {
        (Some(list), _, Some(p)) => {
            p.play_ids(None, Some(list.clone()));
        }
        (None, Some(f), _) => play(f),
        _ => {}
    });
    acts.append(&play_b);
    if let Some(list) = l.shuffle.clone() {
        let b = widgets::labeled_button("lyra-shuffle-symbolic", "Shuffle");
        b.connect_clicked(move |_| {
            if let Some(p) = player() {
                p.play_ids(None, Some(list.clone()));
            }
        });
        acts.append(&b);
    } else if let Some(list) = play_list.clone() {
        // No shuffled list of its own: play it, then YouTube Music's Shuffle once it plays.
        let b = widgets::labeled_button("lyra-shuffle-symbolic", "Shuffle");
        b.connect_clicked(move |_| {
            if let Some(p) = player() {
                p.play_ids(None, Some(list.clone()));
                glib::timeout_add_local_once(std::time::Duration::from_secs(6), || {
                    if let Some(p) = player().filter(|p| p.state().track.is_some()) {
                        p.send(nyx_lyra_core::command::Command::Shuffle);
                    }
                });
            }
        });
        acts.append(&b);
    }
    if let (Some(target), Some(saved)) = (l.save_target.clone(), l.saved) {
        let b = widgets::icon_button("lyra-saved-symbolic", "");
        let on = Rc::new(Cell::new(saved));
        let show = {
            let b = b.clone();
            move |on: bool| {
                b.set_tooltip_text(Some(if on { "Remove from your library" } else { "Save to your library" }));
                if on {
                    b.add_css_class("pressed");
                } else {
                    b.remove_css_class("pressed");
                }
            }
        };
        show(saved);
        b.connect_clicked(move |b| {
            let want = !on.get();
            let (Some(req), Some(p)) = (youtube::save(&target, want), player()) else { return };
            b.set_sensitive(false);
            let (b2, on2, show2) = (b.clone(), on.clone(), show.clone());
            p.request(
                &req,
                Box::new(move |r| {
                    b2.set_sensitive(true);
                    if r.is_ok() {
                        on2.set(want);
                        show2(want);
                    } else if let Some(ui) = super::ui() {
                        nyx_ui::toast::show(&ui.overlay, "YouTube Music didn't change your library");
                    }
                }),
            );
        });
        acts.append(&b);
    }
    if let Some(list) = l.play.clone().or_else(|| l.save_target.clone()) {
        let share = widgets::icon_button("lyra-share-symbolic", "Copy the link");
        share.connect_clicked(move |b| {
            b.clipboard().set_text(&format!("https://music.youtube.com/playlist?list={list}"));
            if let Some(ui) = super::ui() {
                nyx_ui::toast::show(&ui.overlay, "Link copied");
            }
        });
        acts.append(&share);
    }
    text.append(&acts);
    row.append(&text);
    row.upcast()
}

fn is_album(h: &nyx_lyra_core::model::Header) -> bool {
    ["Album", "Single", "EP"].iter().any(|k| h.subtitle.starts_with(k))
}

/// The track table's head: "#", TITLE (ARTIST, ALBUM for a playlist) and the clock.
fn table_head(album: bool) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("table-head");
    let n = left("#", "col-head");
    n.set_size_request(NUM_W, -1);
    n.set_xalign(1.0);
    row.append(&n);
    if !album {
        let gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        gap.set_width_request(26);
        row.append(&gap);
    }
    let t = left("TITLE", "col-head");
    t.set_hexpand(true);
    row.append(&t);
    if !album {
        for name in ["ARTIST", "ALBUM"] {
            let c = left(name, "col-head");
            c.set_size_request(COL_W, -1);
            row.append(&c);
        }
    }
    let clock_icon = gtk::Image::from_icon_name("lyra-clock-symbolic");
    clock_icon.add_css_class("col-head");
    let heart_gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    heart_gap.set_width_request(20);
    row.append(&heart_gap);
    clock_icon.set_size_request(TIME_W, -1);
    clock_icon.set_halign(gtk::Align::End);
    row.append(&clock_icon);
    let pad = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pad.set_width_request(MENU_W);
    row.append(&pad);
    row.upcast()
}

/// The track table's column widths, the same in its head and its rows.
const NUM_W: i32 = 30;
const COL_W: i32 = 160;
const TIME_W: i32 = 44;
const MENU_W: i32 = 28;

/// A row of the track table; the song playing is marked as the mockup's (bars, accent).
fn table_row(item: &Item, n: usize, album: bool, playing: Option<&str>) -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("track-row");
    b.add_css_class("flat");
    let on = playing.is_some() && item.video_id.as_deref() == playing;
    if on {
        b.add_css_class("on");
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    if on {
        let eq = gtk::Image::from_icon_name("lyra-eq-symbolic");
        eq.add_css_class("eq");
        eq.set_size_request(NUM_W, -1);
        eq.set_halign(gtk::Align::End);
        row.append(&eq);
    } else {
        let num = widgets::label(&n.to_string(), "track-number");
        num.set_size_request(NUM_W, -1);
        num.set_xalign(1.0);
        row.append(&num);
    }
    if !album {
        row.append(&thumb(item, 26).root);
    }
    let title = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    title.set_hexpand(true);
    let t = left(&item.title, "track-name");
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.append(&t);
    if item.explicit {
        let e = widgets::label("E", "explicit");
        e.set_valign(gtk::Align::Center);
        title.append(&e);
    }
    row.append(&title);
    if !album {
        let artists = item.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
        for text in [artists.as_str(), item.album.as_ref().map_or("", |a| a.name.as_str())] {
            let c = left(text, "song-album");
            c.set_ellipsize(gtk::pango::EllipsizeMode::End);
            c.set_size_request(COL_W, -1);
            c.set_max_width_chars(1);
            row.append(&c);
        }
    }
    // Liked songs show their heart, as the mockup's.
    let heart = gtk::Image::from_icon_name("lyra-heartf-symbolic");
    heart.add_css_class("row-heart");
    heart.set_size_request(20, -1);
    heart.set_opacity(if item.liked == Some(true) { 1.0 } else { 0.0 });
    row.append(&heart);
    let d = widgets::label(&item.duration.map(|d| super::clock(d as f64)).unwrap_or_default(), "song-duration");
    d.set_size_request(TIME_W, -1);
    d.set_xalign(1.0);
    row.append(&d);
    let menu = tile_menu(item);
    menu.remove_css_class("tile-more");
    menu.add_css_class("row-more");
    menu.set_has_frame(false);
    menu.set_valign(gtk::Align::Center);
    menu.set_size_request(MENU_W, -1);
    row.append(&menu);
    b.set_child(Some(&row));
    let it = item.clone();
    b.connect_clicked(move |_| activate(&it));
    b.upcast()
}

/// An artist's song: its album and how often it's played ("Discovery · 980M plays").
fn artist_song_line(item: &Item) -> String {
    let plays = item.subtitle.split(" • ").find(|p| p.ends_with("plays") || p.ends_with("views"));
    [item.album.as_ref().map(|a| a.name.as_str()), plays].into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

fn left(text: &str, class: &str) -> gtk::Label {
    let l = widgets::label(text, class);
    l.set_xalign(0.0);
    l
}

/// YouTube Music writes " • " between parts; the mockups write " · ".
fn dots(text: &str) -> String {
    text.replace(" • ", " · ")
}

/// A shelf's title, then at its right: Play all (song shelves), More (when YouTube Music
/// has the whole shelf as a page), and the arrows that page through it.
fn shelf_head(s: &Shelf, strip: Option<&Strip>, play_all: Option<Item>) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("shelf-head");
    let t = left(&s.title, "shelf-title");
    row.append(&t);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    row.append(&spacer);
    if let Some(first) = play_all {
        let b = widgets::labeled_button("lyra-play-symbolic", "Play all");
        b.connect_clicked(move |_| play(&first));
        row.append(&b);
    }
    if let Some(id) = s.more.clone() {
        // The mockup's charts say "See all", an artist's songs "Show all", the rest "More".
        let ranked = s.items.iter().any(|i| i.rank.is_some());
        let songs = s.items.iter().all(|i| matches!(i.kind, Kind::Song | Kind::Video));
        let more = gtk::Button::with_label(if ranked {
            "See all"
        } else if songs {
            "Show all"
        } else {
            "More"
        });
        more.add_css_class("flat");
        more.add_css_class("ghost");
        let title = s.title.clone();
        more.connect_clicked(move |_| navigate(Route::Browse { id: id.clone(), title: title.clone() }));
        row.append(&more);
    }
    if let Some(strip) = strip {
        row.append(&grid::arrows(strip));
    }
    row.upcast()
}

/* ── Search ────────────────────────────────────────────────────────────────────── */

/// The search's results (one per section in YouTube Music's reply) as a section per kind,
/// in the order the kinds first come; other shelves as they are.
fn group_results(shelves: &[Shelf]) -> Vec<Shelf> {
    let mut out: Vec<Shelf> = Vec::new();
    for s in shelves {
        if s.title != youtube::RESULTS {
            out.push(s.clone());
            continue;
        }
        for item in &s.items {
            let title = match item.kind {
                Kind::Song => "Songs",
                Kind::Video => "Videos",
                Kind::Album | Kind::Single | Kind::Ep => "Albums",
                Kind::Artist => "Artists",
                Kind::Playlist => "Community playlists",
                Kind::Link | Kind::Other => continue,
            };
            match out.iter_mut().find(|g| g.title == title) {
                Some(g) => g.items.push(item.clone()),
                None => out.push(Shelf {
                    title: title.into(),
                    items: vec![item.clone()],
                    more: None,
                    tiles: !matches!(item.kind, Kind::Song | Kind::Video),
                }),
            }
        }
    }
    out
}

/// The filter a result section's More opens.
fn search_group(title: &str) -> Option<&'static str> {
    ["Songs", "Videos", "Albums", "Artists", "Community playlists"].into_iter().find(|t| *t == title)
}

fn group_head(title: &str, query: &str, chip: Option<Chip>, strip: Option<&Strip>) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("shelf-head");
    let t = left(title, "shelf-title");
    t.set_hexpand(true);
    row.append(&t);
    if let Some(c) = chip.filter(|c| c.params.is_some()) {
        let more = gtk::Button::with_label("More");
        more.add_css_class("flat");
        more.add_css_class("ghost");
        let q = query.to_string();
        more.connect_clicked(move |_| {
            navigate(Route::SearchFilter {
                query: q.clone(),
                title: c.title.clone(),
                params: c.params.clone().unwrap_or_default(),
            })
        });
        row.append(&more);
    }
    if let Some(strip) = strip {
        row.append(&grid::arrows(strip));
    }
    row.upcast()
}

/// All, then YouTube Music's filters for this search.
fn search_chips(query: &str, chips: &[Chip], on: Option<&str>) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.add_css_class("chips");
    let all = gtk::Button::with_label("All");
    all.add_css_class("chip");
    if on.is_none() {
        all.add_css_class("selected");
    }
    let q = query.to_string();
    all.connect_clicked(move |_| navigate(Route::Search(q.clone())));
    row.append(&all);
    for c in chips.iter().filter(|c| c.params.is_some()) {
        let b = gtk::Button::with_label(&c.title);
        b.add_css_class("chip");
        if on == Some(c.title.as_str()) {
            b.add_css_class("selected");
        }
        let (q, c) = (query.to_string(), c.clone());
        b.connect_clicked(move |_| {
            navigate(Route::SearchFilter {
                query: q.clone(),
                title: c.title.clone(),
                params: c.params.clone().unwrap_or_default(),
            })
        });
        row.append(&b);
    }
    row.upcast()
}

/* ── Explore's links, moods and charts ─────────────────────────────────────────── */

fn open_link(item: &Item) {
    let (Some(id), title) = (item.browse_id.clone(), item.title.clone()) else { return };
    match item.params.clone() {
        Some(params) => navigate(Route::Mood { id, title, params }),
        None => navigate(Route::Browse { id, title }),
    }
}

/// New releases, Charts, Moods & genres: an icon, the name and a chevron, as the mockup's.
fn link_card(item: &Item) -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("link-card");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let icon = match item.browse_id.as_deref() {
        Some("FEmusic_new_releases") => "lyra-spark-symbolic",
        Some("FEmusic_charts") => "lyra-eq-symbolic",
        _ => "lyra-compass-symbolic",
    };
    row.append(&gtk::Image::from_icon_name(icon));
    let l = left(&item.title, "link-title");
    l.set_hexpand(true);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&l);
    row.append(&gtk::Image::from_icon_name("lyra-chev-r-symbolic"));
    b.set_child(Some(&row));
    let it = item.clone();
    b.connect_clicked(move |_| open_link(&it));
    b.upcast()
}

/// A mood or genre: its name on a card with its colour down the left edge.
fn mood_tile(item: &Item) -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("mood-tile");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let stripe = gtk::Box::new(gtk::Orientation::Vertical, 0);
    stripe.add_css_class("mood-stripe");
    if let Some(c) = item.color {
        widgets::paint(&stripe, &format!("#{c:06x}"));
    }
    row.append(&stripe);
    let l = left(&item.title, "mood-title");
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&l);
    b.set_child(Some(&row));
    let it = item.clone();
    b.connect_clicked(move |_| open_link(&it));
    b.upcast()
}

/// An artist's song: the cover, the song with its album and plays, the length, a menu.
fn artist_song_row(item: &Item) -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("song-row");
    b.add_css_class("flat");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&thumb(item, 40).root);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let t = left(&item.title, "song-title");
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&t);
    let s = left(&artist_song_line(item), "song-sub");
    s.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&s);
    row.append(&text);
    if let Some(d) = item.duration {
        row.append(&widgets::label(&super::clock(d as f64), "song-duration"));
    }
    b.set_child(Some(&row));
    let it = item.clone();
    b.connect_clicked(move |_| activate(&it));
    b.upcast()
}

/// A chart's row: the place, how it moved, the cover, the song, a menu.
fn ranked_row(item: &Item) -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("song-row");
    b.add_css_class("flat");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let rank = widgets::label(item.rank.as_deref().unwrap_or(""), "rank");
    rank.set_width_chars(2);
    rank.set_xalign(1.0);
    row.append(&rank);
    let (icon, class) = match item.trend {
        Some(1) => ("lyra-trendup-symbolic", "up"),
        Some(-1) => ("lyra-trenddown-symbolic", "down"),
        _ => ("lyra-dash-symbolic", "same"),
    };
    let trend = gtk::Image::from_icon_name(icon);
    trend.add_css_class("trend");
    trend.add_css_class(class);
    row.append(&trend);
    row.append(&thumb(item, 40).root);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let t = left(&item.title, "song-title");
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&t);
    let s = left(&byline(item), "song-sub");
    s.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&s);
    row.append(&text);
    let menu = tile_menu(item);
    menu.remove_css_class("tile-more");
    menu.set_has_frame(false);
    menu.set_valign(gtk::Align::Center);
    row.append(&menu);
    b.set_child(Some(&row));
    let it = item.clone();
    b.connect_clicked(move |_| activate(&it));
    b.upcast()
}

/* ── Tiles, songs, track lists ─────────────────────────────────────────────────── */

fn placeholder_icon(item: &Item) -> &'static str {
    match item.kind {
        Kind::Artist => "lyra-user-symbolic",
        Kind::Song | Kind::Video => "lyra-note-symbolic",
        Kind::Playlist => "lyra-list-symbolic",
        _ => "lyra-disc-symbolic",
    }
}

fn thumb(item: &Item, size: i32) -> Artwork {
    let art = Artwork::new(size, size, placeholder_icon(item));
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
        Kind::Link | Kind::Other => "",
    }
}

fn byline(item: &Item) -> String {
    let artists = item.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    if !artists.is_empty() { artists } else { dots(&item.subtitle) }
}

/// A tile as the mockup's: the cover square at the column's width (round for artists), the
/// title and the line under it; on hover, play at the cover's corner and a menu at its top.
fn tile(item: &Item) -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("tile");
    b.add_css_class("flat");
    let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // Square covers at whatever width the column has: the art is placed by the tile's layout.
    let art = thumb(item, 48);
    art.root.set_halign(gtk::Align::Fill);
    art.root.set_valign(gtk::Align::Fill);
    if item.browse_id.as_deref() == Some(youtube::LIKED_MUSIC) {
        // YouTube Music gives Liked Music no cover; the mockup's is a heart on the accent.
        art.root.add_css_class("liked-art");
        art.placeholder.set_icon_name(Some("lyra-heartf-symbolic"));
        art.placeholder.set_pixel_size(44);
    }
    if item.playable() && item.kind != Kind::Artist {
        let play_mark = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        play_mark.add_css_class("hplay");
        play_mark.append(&gtk::Image::from_icon_name("lyra-play-symbolic"));
        play_mark.set_halign(gtk::Align::End);
        play_mark.set_valign(gtk::Align::End);
        play_mark.set_can_target(false);
        art.root.add_overlay(&play_mark);
    }
    let menu = tile_menu(item);
    art.root.add_overlay(&menu);
    col.append(&art.root);
    let t = left(&item.title, "tile-title");
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    col.append(&t);
    let sub = left(&tile_line(item), "tile-sub");
    sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    col.append(&sub);
    if item.kind == Kind::Artist {
        t.set_xalign(0.5);
        sub.set_xalign(0.5);
    }
    grid::tile_layout(&col, art.root.upcast_ref());
    b.set_child(Some(&col));
    b.set_tooltip_text(Some(&item.title));
    let it = item.clone();
    b.connect_clicked(move |_| activate(&it));
    b.upcast()
}

/// The line under a tile: the artist for songs and albums (as the mockup's), else YouTube
/// Music's own line.
fn tile_line(item: &Item) -> String {
    match item.kind {
        Kind::Song | Kind::Video | Kind::Album | Kind::Single | Kind::Ep if !item.artists.is_empty() => byline(item),
        _ => dots(&item.subtitle),
    }
}

/// "…" at a tile's top right: play it, or go to its artist or album.
fn tile_menu(item: &Item) -> gtk::MenuButton {
    let m = gtk::MenuButton::new();
    m.set_icon_name("lyra-more-symbolic");
    m.add_css_class("tile-more");
    m.set_halign(gtk::Align::End);
    m.set_valign(gtk::Align::Start);
    m.set_tooltip_text(Some("More"));
    let it = item.clone();
    m.set_create_popup_func(move |button| {
        let pop = gtk::Popover::new();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let add = |label: &str, act: Box<dyn Fn()>| {
            let b = gtk::Button::with_label(label);
            b.add_css_class("flat");
            b.add_css_class("menu-row");
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                act();
            });
            body.append(&b);
        };
        if it.playable() {
            let i = it.clone();
            add("Play", Box::new(move || play(&i)));
        }
        if let Some(a) = it.artists.iter().find(|a| a.browse_id.is_some()).cloned() {
            add(
                &format!("Go to {}", a.name),
                Box::new(move || navigate(Route::Browse { id: a.browse_id.clone().unwrap_or_default(), title: a.name.clone() })),
            );
        }
        if let Some(a) = it.album.clone().filter(|a| a.browse_id.is_some()) {
            add(
                "Go to the album",
                Box::new(move || navigate(Route::Browse { id: a.browse_id.clone().unwrap_or_default(), title: a.name.clone() })),
            );
        }
        pop.set_child(Some(&body));
        button.set_popover(Some(&pop));
    });
    m
}

pub fn song_row(item: &Item, number: Option<usize>, album_column: bool) -> gtk::Widget {
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
        Kind::Song | Kind::Video => format!("{} · {}", kind_word(item), byline(item)),
        _ => dots(&item.subtitle),
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
    let menu = tile_menu(item);
    menu.remove_css_class("tile-more");
    menu.add_css_class("row-more");
    menu.set_has_frame(false);
    menu.set_valign(gtk::Align::Center);
    row.append(&menu);
    b.set_child(Some(&row));
    let it = item.clone();
    b.connect_clicked(move |_| activate(&it));
    b.upcast()
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
    col.append(&left(&dots(&first.subtitle), "dim-label"));
    // Shuffle and Radio, as YouTube Music's card has them.
    let acts = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    if let Some(list) = first.shuffle.clone() {
        let b = widgets::labeled_button("lyra-shuffle-symbolic", "Shuffle");
        b.add_css_class("suggested-action");
        b.connect_clicked(move |_| {
            if let Some(p) = player() {
                p.play_ids(None, Some(list.clone()));
            }
        });
        acts.append(&b);
    }
    if let Some(list) = first.radio.clone() {
        let b = widgets::labeled_button("lyra-radio-symbolic", "Radio");
        b.connect_clicked(move |_| {
            if let Some(p) = player() {
                p.play_ids(None, Some(list.clone()));
            }
        });
        acts.append(&b);
    }
    if acts.first_child().is_some() {
        col.append(&acts);
    }
    card.set_child(Some(&col));
    let it = (*first).clone();
    card.connect_clicked(move |_| activate(&it));
    row.append(&card);
    let rest: Vec<&Item> = items.iter().skip(1).copied().collect();
    if !rest.is_empty() {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
        list.set_hexpand(true);
        for item in rest.iter().take(4) {
            list.append(&song_row(item, None, false));
        }
        row.append(&list);
    }
    row.upcast()
}

/// The mockup's featured card: the cover, the title and kind, the line under it, its first
/// songs in two columns, and Play.
fn feature_card(items: &[&Item]) -> gtk::Widget {
    let Some(first) = items.first() else { return gtk::Box::new(gtk::Orientation::Vertical, 0).upcast() };
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 22);
    card.add_css_class("feature");
    card.add_css_class("card");
    card.append(&thumb(first, 150).root);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.set_hexpand(true);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let t = left(&first.title, "feature-title");
    t.set_hexpand(true);
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    top.append(&t);
    top.append(&widgets::label(&kind_word(first).to_uppercase(), "feature-kind"));
    body.append(&top);
    body.append(&left(&dots(&first.subtitle), "dim-label"));
    let songs = gtk::Grid::builder().column_spacing(24).row_spacing(2).column_homogeneous(true).build();
    for (i, item) in items.iter().skip(1).take(4).enumerate() {
        songs.attach(&song_row(item, Some(i + 1), false), (i % 2) as i32, (i / 2) as i32, 1, 1);
    }
    body.append(&songs);
    let acts = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let play_b = widgets::labeled_button("lyra-play-symbolic", "Play");
    play_b.add_css_class("suggested-action");
    let it = (*first).clone();
    play_b.connect_clicked(move |_| play(&it));
    acts.append(&play_b);
    body.append(&acts);
    card.append(&body);
    card.upcast()
}

/* ── The library ───────────────────────────────────────────────────────────────── */

fn library_as_list(route: &Route) -> bool {
    matches!(route, Route::Library(Tab::Playlists | Tab::Albums | Tab::Artists)) && prefs::get().library_list
}

/// The tabs (in the mockup's order), then at the right the sort menu (filled in once the
/// page says what it offers) and the grid/list switch.
fn library_bar(current: Tab) -> gtk::Widget {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("library-bar");
    let opts = widgets::opts(&[("playlists", "Playlists"), ("albums", "Albums"), ("songs", "Songs"), ("artists", "Artists")]);
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
    seg.set_hexpand(true);
    bar.append(&seg);
    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    slot.set_widget_name("sort-slot");
    bar.append(&slot);
    if current != Tab::Songs {
        let view = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        view.add_css_class("view-switch");
        let list = prefs::get().library_list;
        for (icon, tip, as_list) in [("lyra-grid-symbolic", "Tiles", false), ("lyra-list-symbolic", "List", true)] {
            let b = widgets::icon_button(icon, tip);
            if as_list == list {
                b.add_css_class("on");
            }
            b.connect_clicked(move |_| {
                prefs::update(|p| p.library_list = as_list);
                super::refresh();
            });
            view.append(&b);
        }
        bar.append(&view);
    }
    bar.upcast()
}

/// The library bar's slot for the sort menu, found from the page's content box.
fn library_sort_slot(content: &gtk::Box) -> Option<gtk::Box> {
    let bar = content.prev_sibling()?;
    let mut c = bar.first_child();
    while let Some(w) = c {
        if w.widget_name() == "sort-slot" {
            return w.downcast().ok();
        }
        c = w.next_sibling();
    }
    None
}

/// The sort orders YouTube Music offers; picking one draws the list again in that order.
fn sort_menu(sorts: &[Chip], pager: &Rc<Pager>) -> gtk::Widget {
    let current = sorts.iter().find(|s| s.selected).or(sorts.first()).map(|s| s.title.clone()).unwrap_or_default();
    let m = gtk::MenuButton::new();
    m.set_label(&current);
    m.add_css_class("sort-menu");
    let pop = gtk::Popover::new();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 1);
    for s in sorts {
        let b = gtk::Button::with_label(&s.title);
        b.add_css_class("flat");
        b.add_css_class("menu-row");
        let (token, title, pager, pop2, m2) = (s.params.clone(), s.title.clone(), pager.clone(), pop.clone(), m.clone());
        b.connect_clicked(move |_| {
            pop2.popdown();
            let Some(req) = token.as_deref().and_then(youtube::more) else { return };
            m2.set_label(&title);
            let p = pager.clone();
            fetch(&req, false, move |r| {
                if let Ok(page) = r {
                    // Keep the bar's slot; draw the list again.
                    clear(&p.content);
                    p.counted.set(0);
                    render(&p, &Page { sorts: Vec::new(), ..page }, true);
                }
            });
        });
        body.append(&b);
    }
    pop.set_child(Some(&body));
    m.set_popover(Some(&pop));
    m.upcast()
}

fn new_playlist_tile() -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("tile");
    b.add_css_class("flat");
    let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let art = Artwork::new(48, 48, "lyra-plus-symbolic");
    art.root.add_css_class("new-tile");
    art.root.set_halign(gtk::Align::Fill);
    art.root.set_valign(gtk::Align::Fill);
    col.append(&art.root);
    col.append(&left("New playlist", "tile-title"));
    grid::tile_layout(&col, art.root.upcast_ref());
    b.set_child(Some(&col));
    b.connect_clicked(|_| super::new_playlist());
    b.upcast()
}

fn new_playlist_row() -> gtk::Widget {
    let b = gtk::Button::new();
    b.add_css_class("song-row");
    b.add_css_class("flat");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let art = Artwork::new(40, 40, "lyra-plus-symbolic");
    art.root.add_css_class("new-tile");
    row.append(&art.root);
    row.append(&left("New playlist", "song-title"));
    b.set_child(Some(&row));
    b.connect_clicked(|_| super::new_playlist());
    b.upcast()
}

/* ── Search ────────────────────────────────────────────────────────────────────── */

fn recent_searches(b: &gtk::Box) {
    let recent = prefs::get().recent_searches;
    if recent.is_empty() {
        b.append(&widgets::empty_state(
            "lyra-search-symbolic",
            "Search YouTube Music",
            "Songs, albums, artists and playlists. Ctrl+F from anywhere.",
            None,
        ));
        return;
    }
    b.append(&left("Recent searches", "shelf-title"));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    for q in recent {
        let row = gtk::Button::new();
        row.add_css_class("flat");
        row.add_css_class("song-row");
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        content.append(&gtk::Image::from_icon_name("lyra-history-symbolic"));
        content.append(&left(&q, ""));
        row.set_child(Some(&content));
        let query = Rc::new(q);
        row.connect_clicked(move |_| navigate(Route::Search((*query).clone())));
        list.append(&row);
    }
    b.append(&list);
}
