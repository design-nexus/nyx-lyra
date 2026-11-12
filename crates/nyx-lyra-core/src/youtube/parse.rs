//! Reading YouTube Music's replies into [`crate::model`]. The replies are trees of
//! "renderers"; these are the ones the web client uses for shelves, rows, tiles and headers.

use crate::model::{ArtistActions, Channel, Chip, Header, Item, Kind, Link, ListActions, Page, Shelf, Thumbnail, parse_duration};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// YouTube Music answered with an error.
    Server { code: i64, message: String },
    /// Nothing in the reply is something Lyra knows how to show.
    Unrecognized,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseError::Server { code, message } => write!(f, "YouTube Music answered {code}: {message}"),
            ParseError::Unrecognized => write!(f, "Lyra couldn't read YouTube Music's answer"),
        }
    }
}

/// Whether the reply was for a signed-in session, when it says.
pub fn logged_in(v: &Value) -> Option<bool> {
    v.pointer("/responseContext/serviceTrackingParams")?
        .as_array()?
        .iter()
        .flat_map(|s| s.get("params").and_then(Value::as_array).into_iter().flatten())
        .find(|p| p.get("key").and_then(Value::as_str) == Some("logged_in"))
        .and_then(|p| p.get("value").and_then(Value::as_str))
        .map(|v| v == "1")
}

fn server_error(v: &Value) -> Option<ParseError> {
    let e = v.get("error")?;
    Some(ParseError::Server {
        code: e.get("code").and_then(Value::as_i64).unwrap_or(0),
        message: e.get("message").and_then(Value::as_str).unwrap_or_default().chars().take(200).collect(),
    })
}

pub fn page(v: &Value) -> Result<Page, ParseError> {
    if let Some(e) = server_error(v) {
        return Err(e);
    }
    let mut shelves = Vec::new();
    collect_shelves(v, &mut shelves);
    shelves.retain(|s: &Shelf| !s.items.is_empty());
    let header = find_header(v);
    if shelves.is_empty() && header.is_none() {
        // An empty library page is only a message ("Albums you save will show up here").
        if find(v, "messageRenderer").is_some() {
            return Ok(Page::default());
        }
        return Err(ParseError::Unrecognized);
    }
    Ok(Page { header, shelves, continuation: continuation(v), chips: chips(v), sorts: sorts(v) })
}

/* ── Small readers ─────────────────────────────────────────────────────────────── */

fn text(v: Option<&Value>) -> String {
    let Some(v) = v else { return String::new() };
    if let Some(s) = v.get("simpleText").and_then(Value::as_str) {
        return s.to_string();
    }
    v.get("runs")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(|x| x.get("text").and_then(Value::as_str)).collect())
        .unwrap_or_default()
}

fn runs(v: Option<&Value>) -> &[Value] {
    v.and_then(|v| v.get("runs")).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

fn str_at<'a>(v: &'a Value, path: &str) -> Option<&'a str> {
    v.pointer(path).and_then(Value::as_str)
}

/// The first value under `key` anywhere in `v`, depth first.
fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Object(o) => o.get(key).or_else(|| o.values().find_map(|x| find(x, key))),
        Value::Array(a) => a.iter().find_map(|x| find(x, key)),
        _ => None,
    }
}

fn thumbnails(v: Option<&Value>) -> Vec<Thumbnail> {
    let Some(list) = v.and_then(|v| find(v, "thumbnails")).and_then(Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|t| {
            let url = t.get("url")?.as_str()?;
            url.starts_with("https://").then(|| Thumbnail {
                url: url.to_string(),
                width: t.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
                height: t.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
            })
        })
        .collect()
}

fn page_type(endpoint: &Value) -> Option<&str> {
    str_at(endpoint, "/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
}

/// A watch endpoint's music type; one without (a plain YouTube video) counts as a video.
fn video_type(endpoint: &Value) -> Option<&str> {
    let w = endpoint.get("watchEndpoint")?;
    Some(
        str_at(w, "/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType")
            .unwrap_or("MUSIC_VIDEO_TYPE_OMV"),
    )
}

fn kind_of(page: Option<&str>, video: Option<&str>, first_label: &str) -> Kind {
    match (page, video) {
        (Some("MUSIC_PAGE_TYPE_ALBUM"), _) | (Some("MUSIC_PAGE_TYPE_AUDIOBOOK"), _) => match first_label {
            "Single" => Kind::Single,
            "EP" => Kind::Ep,
            _ => Kind::Album,
        },
        (Some("MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL" | "MUSIC_PAGE_TYPE_LIBRARY_ARTIST"), _) => Kind::Artist,
        (Some("MUSIC_PAGE_TYPE_PLAYLIST"), _) => Kind::Playlist,
        (Some(_), _) => Kind::Other,
        (None, Some("MUSIC_VIDEO_TYPE_ATV")) => Kind::Song,
        (None, Some("MUSIC_VIDEO_TYPE_PODCAST_EPISODE")) => Kind::Other,
        (None, Some(_)) => Kind::Video,
        (None, None) => match first_label {
            "Song" => Kind::Song,
            "Video" => Kind::Video,
            "Album" => Kind::Album,
            "Single" => Kind::Single,
            "EP" => Kind::Ep,
            "Artist" => Kind::Artist,
            "Playlist" => Kind::Playlist,
            _ => Kind::Other,
        },
    }
}

const LABELS: &[&str] = &["Song", "Video", "Album", "Single", "EP", "Artist", "Playlist", "Episode", "Podcast", "Profile"];

/// Artists and the album from runs that link to them; plain names when none link.
fn people(rs: &[Value]) -> (Vec<Link>, Option<Link>) {
    let mut artists = Vec::new();
    let mut album = None;
    for r in rs {
        let name = r.get("text").and_then(Value::as_str).unwrap_or_default().trim();
        if name.is_empty() || name == "•" || name == "&" || name == "," {
            continue;
        }
        let Some(ep) = r.get("navigationEndpoint") else { continue };
        let id = str_at(ep, "/browseEndpoint/browseId").map(String::from);
        match page_type(ep) {
            Some("MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL") => {
                artists.push(Link { name: name.into(), browse_id: id })
            }
            Some("MUSIC_PAGE_TYPE_ALBUM") => album = Some(Link { name: name.into(), browse_id: id }),
            _ => {}
        }
    }
    (artists, album)
}

fn play_endpoint(v: &Value) -> Option<&Value> {
    find(v, "musicPlayButtonRenderer").and_then(|b| b.get("playNavigationEndpoint"))
}

fn ids(endpoints: &[Option<&Value>]) -> (Option<String>, Option<String>) {
    let mut video = None;
    let mut playlist = None;
    for e in endpoints.iter().flatten() {
        video = video.or_else(|| str_at(e, "/watchEndpoint/videoId").map(String::from));
        playlist = playlist
            .or_else(|| str_at(e, "/watchPlaylistEndpoint/playlistId").map(String::from))
            .or_else(|| str_at(e, "/watchEndpoint/playlistId").map(String::from));
    }
    (video, playlist)
}

fn explicit(v: &Value) -> bool {
    v.get("badges")
        .and_then(Value::as_array)
        .is_some_and(|b| b.iter().any(|x| str_at(x, "/musicInlineBadgeRenderer/icon/iconType") == Some("MUSIC_EXPLICIT_BADGE")))
}

/* ── Rows and tiles ────────────────────────────────────────────────────────────── */

fn list_item(r: &Value) -> Option<Item> {
    let cols: Vec<&Value> = r
        .get("flexColumns")?
        .as_array()?
        .iter()
        .filter_map(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text"))
        .collect();
    let title = text(cols.first().copied());
    if title.is_empty() {
        return None;
    }
    let title_ep = runs(cols.first().copied()).first().and_then(|x| x.get("navigationEndpoint"));
    let row_ep = r.get("navigationEndpoint");
    // "Shuffle all" heading a list is an action, not a song; Lyra has its own.
    if cols.len() == 1 && title_ep.is_none() && row_ep.is_some_and(|e| e.get("watchPlaylistEndpoint").is_some()) {
        return None;
    }
    let rest: Vec<&Value> = cols.iter().skip(1).flat_map(|c| runs(Some(c))).collect();
    let rest_owned: Vec<Value> = rest.iter().map(|v| (*v).clone()).collect();
    let (mut artists, album) = people(&rest_owned);
    let subtitle = cols.iter().skip(1).map(|c| text(Some(c))).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" • ");
    let first_label = rest.first().and_then(|x| x.get("text")).and_then(Value::as_str).unwrap_or_default();
    let (mut video_id, playlist_id) = ids(&[title_ep, play_endpoint(r), row_ep]);
    video_id = str_at(r, "/playlistItemData/videoId").map(String::from).or(video_id);
    let page = row_ep.and_then(page_type).or_else(|| title_ep.and_then(page_type));
    let video = title_ep.and_then(video_type).or_else(|| play_endpoint(r).and_then(video_type));
    let kind = kind_of(page, video, first_label);
    let duration = r
        .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text")
        .map(|t| text(Some(t)))
        .and_then(|t| parse_duration(&t))
        .or_else(|| rest.iter().filter_map(|x| x.get("text").and_then(Value::as_str)).find_map(parse_duration));
    if artists.is_empty() && matches!(kind, Kind::Song | Kind::Video) {
        // Plain names (no link), e.g. "Various Artists": the first run that isn't a label.
        if let Some(name) = rest
            .iter()
            .filter_map(|x| x.get("text").and_then(Value::as_str))
            .map(str::trim)
            .find(|t| !t.is_empty() && *t != "•" && !LABELS.contains(t) && parse_duration(t).is_none())
        {
            artists.push(Link { name: name.into(), browse_id: None });
        }
    }
    Some(Item {
        kind,
        title,
        subtitle,
        artists,
        album,
        duration,
        video_id,
        playlist_id,
        browse_id: row_ep
            .and_then(|e| str_at(e, "/browseEndpoint/browseId"))
            .or_else(|| title_ep.and_then(|e| str_at(e, "/browseEndpoint/browseId")))
            .map(String::from),
        thumbnails: thumbnails(r.get("thumbnail")),
        explicit: explicit(r),
        rank: r
            .pointer("/customIndexColumn/musicCustomIndexColumnRenderer/text")
            .map(|t| text(Some(t)))
            .filter(|t| !t.is_empty()),
        liked: {
            let mut likes = Vec::new();
            all(r, "likeButtonRenderer", &mut likes);
            likes.first().and_then(|l| str_at(l, "/likeStatus")).map(|s| s == "LIKE")
        },
        trend: str_at(r, "/customIndexColumn/musicCustomIndexColumnRenderer/icon/iconType").map(|i| match i {
            "ARROW_DROP_UP" => 1,
            "ARROW_DROP_DOWN" => -1,
            _ => 0,
        }),
        ..Item::default()
    })
}

/// Explore's buttons (New releases, Charts, Moods & genres) and the moods and genres.
fn nav_button(r: &Value) -> Option<Item> {
    let title = text(r.get("buttonText"));
    let ep = r.pointer("/clickCommand/browseEndpoint")?;
    let id = str_at(ep, "/browseId")?;
    // Podcasts are Nyx Music's.
    if title.is_empty() || id == "FEmusic_non_music_audio" {
        return None;
    }
    Some(Item {
        kind: Kind::Link,
        title,
        browse_id: Some(id.to_string()),
        params: str_at(ep, "/params").map(String::from),
        color: r.pointer("/solid/leftStripeColor").and_then(Value::as_u64).map(|c| (c & 0xFF_FFFF) as u32),
        ..Item::default()
    })
}

fn tile(r: &Value) -> Option<Item> {
    let title = text(r.get("title"));
    if title.is_empty() {
        return None;
    }
    let ep = r.get("navigationEndpoint").or_else(|| runs(r.get("title")).first().and_then(|x| x.get("navigationEndpoint")));
    // The library's "New playlist" tile is an action, not something to show; Lyra has its own.
    if ep.is_some_and(|e| e.get("createPlaylistEndpoint").is_some()) {
        return None;
    }
    let sub = runs(r.get("subtitle"));
    let first_label = sub.first().and_then(|x| x.get("text")).and_then(Value::as_str).unwrap_or_default();
    let (artists, album) = people(sub);
    let (video_id, playlist_id) = ids(&[ep, play_endpoint(r)]);
    let kind = kind_of(ep.and_then(page_type), ep.and_then(video_type), first_label);
    Some(Item {
        kind,
        title,
        subtitle: text(r.get("subtitle")),
        artists,
        album,
        duration: None,
        video_id,
        playlist_id,
        browse_id: ep.and_then(|e| str_at(e, "/browseEndpoint/browseId")).map(String::from),
        thumbnails: thumbnails(r.get("thumbnailRenderer")),
        explicit: r.get("subtitleBadges").is_some_and(|b| explicit(&serde_json::json!({ "badges": b }))),
        ..Item::default()
    })
}

fn card(r: &Value) -> Option<Item> {
    let title_ep = runs(r.get("title")).first().and_then(|x| x.get("navigationEndpoint"));
    let sub = runs(r.get("subtitle"));
    let first_label = sub.first().and_then(|x| x.get("text")).and_then(Value::as_str).unwrap_or_default();
    let (artists, album) = people(sub);
    let (video_id, playlist_id) = ids(&[title_ep, play_endpoint(r)]);
    let title = text(r.get("title"));
    (!title.is_empty()).then(|| Item {
        kind: kind_of(title_ep.and_then(page_type), title_ep.and_then(video_type), first_label),
        title,
        subtitle: text(r.get("subtitle")),
        artists,
        album,
        duration: sub.iter().filter_map(|x| x.get("text").and_then(Value::as_str)).find_map(parse_duration),
        video_id,
        playlist_id,
        browse_id: title_ep.and_then(|e| str_at(e, "/browseEndpoint/browseId")).map(String::from),
        thumbnails: thumbnails(r.get("thumbnail")),
        explicit: false,
        shuffle: card_list(r, "Shuffle"),
        radio: card_list(r, "Mix").or_else(|| card_list(r, "Radio")),
        ..Item::default()
    })
}

/// The list a top result's button plays ("Shuffle", "Mix").
fn card_list(r: &Value, label: &str) -> Option<String> {
    let mut buttons = Vec::new();
    all(r, "buttonRenderer", &mut buttons);
    buttons
        .into_iter()
        .find(|b| text(b.get("text")) == label)
        .and_then(|b| {
            b.pointer("/command/watchPlaylistEndpoint/playlistId")
                .or_else(|| b.pointer("/navigationEndpoint/watchPlaylistEndpoint/playlistId"))
        })
        .and_then(Value::as_str)
        .filter(|id| super::valid_id(id))
        .map(String::from)
}

fn row(v: &Value) -> Option<Item> {
    if let Some(r) = v.get("musicResponsiveListItemRenderer") {
        list_item(r)
    } else if let Some(r) = v.get("musicTwoRowItemRenderer") {
        tile(r)
    } else if let Some(r) = v.get("musicNavigationButtonRenderer") {
        nav_button(r)
    } else {
        None
    }
}

fn rows(list: Option<&Value>) -> Vec<Item> {
    list.and_then(Value::as_array).map(|a| a.iter().filter_map(row).collect()).unwrap_or_default()
}

/* ── Shelves and headers ───────────────────────────────────────────────────────── */

/// The title of the shelf that gathers search results listed one per section.
pub const RESULTS: &str = "Results";
/// The title of a card shelf without one of its own (the search's top result, Home's
/// featured album).
pub const TOP_RESULT: &str = "Top result";

fn more(ep: Option<&Value>) -> Option<String> {
    ep.and_then(|e| str_at(e, "/browseEndpoint/browseId")).map(String::from)
}

fn collect_shelves(v: &Value, out: &mut Vec<Shelf>) {
    match v {
        Value::Object(o) => {
            for (k, r) in o {
                match k.as_str() {
                    "musicCarouselShelfRenderer" => {
                        let h = r.pointer("/header/musicCarouselShelfBasicHeaderRenderer");
                        out.push(Shelf {
                            title: text(h.and_then(|h| h.get("title"))),
                            items: rows(r.get("contents")),
                            tiles: has_tiles(r.get("contents")),
                            more: more(h.and_then(|h| h.pointer("/moreContentButton/buttonRenderer/navigationEndpoint")))
                                .or_else(|| {
                                    more(h.and_then(|h| runs(h.get("title")).first()).and_then(|x| x.get("navigationEndpoint")))
                                }),
                        });
                    }
                    "musicShelfRenderer" | "musicPlaylistShelfRenderer" => out.push(Shelf {
                        title: text(r.get("title")),
                        items: rows(r.get("contents")),
                        more: more(r.get("bottomEndpoint")),
                        tiles: false,
                    }),
                    "musicCardShelfRenderer" => {
                        let mut items: Vec<Item> = card(r).into_iter().collect();
                        items.extend(rows(r.get("contents")));
                        let title = text(r.pointer("/header/musicCardShelfHeaderBasicRenderer/title"));
                        out.push(Shelf {
                            title: if title.is_empty() { TOP_RESULT.into() } else { title },
                            items,
                            more: None,
                            tiles: false,
                        });
                    }
                    // The next part of a long list.
                    "appendContinuationItemsAction" => out.push(Shelf {
                        title: String::new(),
                        items: rows(r.get("continuationItems")),
                        more: None,
                        tiles: has_tiles(r.get("continuationItems")),
                    }),
                    "musicShelfContinuation" | "musicPlaylistShelfContinuation" => {
                        out.push(Shelf { title: String::new(), items: rows(r.get("contents")), more: None, tiles: false })
                    }
                    "gridContinuation" => {
                        out.push(Shelf { title: String::new(), items: rows(r.get("items")), more: None, tiles: true })
                    }
                    // Search lists each result in its own section; consecutive ones make one shelf.
                    "itemSectionRenderer" => {
                        let items = rows(r.get("contents"));
                        if items.is_empty() {
                            collect_shelves(r, out);
                        } else {
                            match out.last_mut() {
                                Some(last) if last.title == RESULTS => last.items.extend(items),
                                _ => out.push(Shelf { title: RESULTS.into(), items, more: None, tiles: false }),
                            }
                        }
                    }
                    "gridRenderer" => out.push(Shelf {
                        title: text(r.pointer("/header/gridHeaderRenderer/title")),
                        items: rows(r.get("items")),
                        more: None,
                        tiles: true,
                    }),
                    _ => collect_shelves(r, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_shelves(x, out)),
        _ => {}
    }
}

fn has_tiles(list: Option<&Value>) -> bool {
    list.and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| x.get("musicTwoRowItemRenderer").is_some()))
}

fn all<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                if k == key {
                    out.push(x);
                } else {
                    all(x, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| all(x, key, out)),
        _ => {}
    }
}

/// Chips Lyra doesn't offer: podcasts are Nyx Music's; profiles and featured playlists
/// aren't in the mockups' filters.
const SKIPPED_CHIPS: &[&str] = &["Podcasts", "Episodes", "Profiles", "Featured playlists"];

/// Home's moods and the search's filters (their chip clouds).
fn chips(v: &Value) -> Vec<Chip> {
    let mut found = Vec::new();
    all(v, "chipCloudChipRenderer", &mut found);
    found
        .into_iter()
        .filter_map(|c| {
            let title = text(c.get("text"));
            // Home's moods browse; the search's filters search again.
            let ep = c.pointer("/navigationEndpoint/browseEndpoint").or_else(|| c.pointer("/navigationEndpoint/searchEndpoint"));
            (!title.is_empty() && !SKIPPED_CHIPS.contains(&title.as_str())).then(|| Chip {
                title,
                browse_id: ep.and_then(|e| str_at(e, "/browseId")).map(String::from),
                params: ep.and_then(|e| str_at(e, "/params")).map(String::from),
                selected: c.get("isSelected").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

/// The library's sort orders (its sort menu). Each reloads the list from a continuation
/// token, kept in `params`; the button's label names the one in use.
fn sorts(v: &Value) -> Vec<Chip> {
    let mut found = Vec::new();
    all(v, "musicSortFilterButtonRenderer", &mut found);
    let Some(button) = found.first() else { return Vec::new() };
    let label = str_at(button, "/accessibility/accessibilityData/label").unwrap_or_default();
    let mut items = Vec::new();
    all(button, "musicMultiSelectMenuItemRenderer", &mut items);
    items
        .into_iter()
        .filter_map(|i| {
            let title = text(i.get("title"));
            let mut reload = Vec::new();
            all(i.get("selectedCommand")?, "reloadContinuationData", &mut reload);
            let token = reload.first().and_then(|r| str_at(r, "/continuation")).map(String::from);
            (!title.is_empty() && token.is_some()).then(|| Chip {
                selected: label.contains(&format!("{title} selected")),
                title,
                browse_id: None,
                params: token,
            })
        })
        .collect()
}

fn description(r: &Value) -> String {
    text(r.get("description").and_then(|d| d.pointer("/musicDescriptionShelfRenderer/description")).or(r.get("description")))
}

fn find_header(v: &Value) -> Option<Header> {
    for key in [
        "musicResponsiveHeaderRenderer",
        "musicImmersiveHeaderRenderer",
        "musicDetailHeaderRenderer",
        "musicVisualHeaderRenderer",
    ] {
        if let Some(r) = find(v, key) {
            let title = text(r.get("title"));
            if title.is_empty() {
                continue;
            }
            let mut subtitle = text(r.get("subtitle"));
            let strap = text(r.get("straplineTextOne"));
            if !strap.is_empty() {
                subtitle = if subtitle.is_empty() { strap } else { format!("{subtitle} • {strap}") };
            }
            if subtitle.is_empty() {
                subtitle = text(r.pointer("/subscriptionButton/subscribeButtonRenderer/subscriberCountText"));
            }
            return Some(Header {
                title,
                subtitle,
                second_subtitle: text(r.get("secondSubtitle")),
                description: description(r),
                thumbnails: thumbnails(r.get("thumbnail").or(r.get("foregroundThumbnail"))),
                artist: (key == "musicImmersiveHeaderRenderer" || r.get("subscriptionButton").is_some())
                    .then(|| artist_actions(r)),
                list: (key == "musicResponsiveHeaderRenderer").then(|| list_actions(r)),
            });
        }
    }
    None
}

fn list_actions(r: &Value) -> ListActions {
    let id = |v: Option<&Value>| v.and_then(Value::as_str).filter(|id| super::valid_id(id)).map(String::from);
    let mut play = Vec::new();
    all(r, "musicPlayButtonRenderer", &mut play);
    let play_list = play.first().and_then(|p| {
        id(p.pointer("/playNavigationEndpoint/watchEndpoint/playlistId"))
            .or_else(|| id(p.pointer("/playNavigationEndpoint/watchPlaylistEndpoint/playlistId")))
    });
    let mut items = Vec::new();
    all(r, "menuNavigationItemRenderer", &mut items);
    let shuffle = items
        .iter()
        .find(|i| str_at(i, "/icon/iconType") == Some("MUSIC_SHUFFLE"))
        .and_then(|i| id(i.pointer("/navigationEndpoint/watchPlaylistEndpoint/playlistId")));
    let mut toggles = Vec::new();
    all(r, "toggleButtonRenderer", &mut toggles);
    let save = toggles.into_iter().find(|t| {
        t.pointer("/defaultServiceEndpoint/likeEndpoint").is_some() || t.pointer("/toggledServiceEndpoint/likeEndpoint").is_some()
    });
    let by = runs(r.get("straplineTextOne")).first().map(|run| Link {
        name: run.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
        browse_id: run.pointer("/navigationEndpoint/browseEndpoint/browseId").and_then(Value::as_str).map(String::from),
    });
    // A playlist of yours names its owner in a "facepile" instead.
    let face = r.pointer("/facepile/avatarStackViewModel");
    let face_by = face.map(|f| Link {
        name: str_at(f, "/rendererContext/accessibilityContext/label").unwrap_or_default().to_string(),
        browse_id: f
            .pointer("/rendererContext/commandContext/onTap/innertubeCommand/browseEndpoint/browseId")
            .and_then(Value::as_str)
            .map(String::from),
    });
    let face_photo: Vec<Thumbnail> = face
        .and_then(|f| f.pointer("/avatars/0/avatarViewModel/image/sources/0/url"))
        .and_then(Value::as_str)
        .filter(|u| u.starts_with("https://"))
        .map(|u| vec![Thumbnail { url: u.to_string(), width: 48, height: 48 }])
        .unwrap_or_default();
    let mut shuffle_toggles = Vec::new();
    all(r, "toggleButtonRenderer", &mut shuffle_toggles);
    let shuffle_play = shuffle_toggles.iter().any(|t| str_at(t, "/defaultIcon/iconType") == Some("MUSIC_SHUFFLE"));
    ListActions {
        play: play_list.or_else(|| id(r.pointer("/playlistId"))),
        shuffle_play: shuffle.is_none() && shuffle_play,
        shuffle,
        saved: save.and_then(|t| t.get("isToggled")).and_then(Value::as_bool),
        save_target: save.and_then(|t| {
            id(t.pointer("/defaultServiceEndpoint/likeEndpoint/target/playlistId"))
                .or_else(|| id(t.pointer("/toggledServiceEndpoint/likeEndpoint/target/playlistId")))
        }),
        by_photo: if by.as_ref().is_some_and(|b| !b.name.is_empty()) {
            thumbnails(r.get("straplineThumbnail"))
        } else {
            face_photo
        },
        by: by.filter(|b| !b.name.is_empty()).or(face_by.filter(|b| !b.name.is_empty())),
    }
}

fn artist_actions(r: &Value) -> ArtistActions {
    let list = |button: &str| {
        let ep = r.pointer(&format!("/{button}/buttonRenderer/navigationEndpoint"))?;
        str_at(ep, "/watchEndpoint/playlistId")
            .or_else(|| str_at(ep, "/watchPlaylistEndpoint/playlistId"))
            .filter(|id| super::valid_id(id))
            .map(String::from)
    };
    let sub = r.pointer("/subscriptionButton/subscribeButtonRenderer");
    ArtistActions {
        shuffle: list("playButton"),
        radio: list("startRadioButton"),
        channel_id: sub.and_then(|s| str_at(s, "/channelId")).filter(|id| super::valid_id(id)).map(String::from),
        subscribed: sub.and_then(|s| s.get("subscribed")).and_then(Value::as_bool),
        subscribers: sub.map(|s| text(s.get("longSubscriberCountText").or(s.get("subscriberCountText")))).unwrap_or_default(),
    }
}

fn continuation(v: &Value) -> Option<String> {
    find(v, "continuationItemRenderer")
        .and_then(|c| str_at(c, "/continuationEndpoint/continuationCommand/token"))
        .or_else(|| find(v, "nextContinuationData").and_then(|c| str_at(c, "/continuation")))
        .map(String::from)
}

/* ── Up next and lyrics ────────────────────────────────────────────────────────── */

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Next {
    pub queue: Vec<Item>,
    /// The queue item that's playing.
    pub current: Option<usize>,
    pub lyrics: Option<String>,
    pub related: Option<String>,
    /// What the queue plays from ("Hurry Up, We're Dreaming", "Midnight City Mix").
    pub playing_from: Option<String>,
}

pub fn next(v: &Value) -> Result<Next, ParseError> {
    if let Some(e) = server_error(v) {
        return Err(e);
    }
    let tabs = v
        .pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs")
        .and_then(Value::as_array)
        .ok_or(ParseError::Unrecognized)?;
    let tab_id =
        |i: usize| tabs.get(i).and_then(|t| str_at(t, "/tabRenderer/endpoint/browseEndpoint/browseId")).map(String::from);
    let mut out = Next { lyrics: tab_id(1), related: tab_id(2).or_else(|| tab_id(3)), ..Default::default() };
    out.playing_from = find(v, "musicQueueHeaderRenderer").map(|h| text(h.get("subtitle"))).filter(|s| !s.is_empty());
    let mut panel = Vec::new();
    collect_panel(tabs.first().ok_or(ParseError::Unrecognized)?, &mut panel);
    for (i, (item, selected)) in panel.into_iter().enumerate() {
        if selected {
            out.current = Some(i);
        }
        out.queue.push(item);
    }
    Ok(out)
}

fn collect_panel(v: &Value, out: &mut Vec<(Item, bool)>) {
    match v {
        Value::Object(o) => {
            if let Some(r) = o.get("playlistPanelVideoRenderer") {
                let byline = runs(r.get("longBylineText"));
                let (artists, album) = people(byline);
                let ep = r.get("navigationEndpoint");
                let title = text(r.get("title"));
                if !title.is_empty() {
                    out.push((
                        Item {
                            kind: kind_of(None, ep.and_then(video_type), ""),
                            title,
                            subtitle: text(r.get("longBylineText")),
                            artists,
                            album,
                            duration: parse_duration(&text(r.get("lengthText"))),
                            video_id: r.get("videoId").and_then(Value::as_str).map(String::from),
                            playlist_id: ep.and_then(|e| str_at(e, "/watchEndpoint/playlistId")).map(String::from),
                            browse_id: None,
                            thumbnails: thumbnails(r.get("thumbnail")),
                            explicit: explicit(r),
                            ..Item::default()
                        },
                        r.get("selected").and_then(Value::as_bool).unwrap_or(false),
                    ));
                }
                return;
            }
            o.values().for_each(|x| collect_panel(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| collect_panel(x, out)),
        _ => {}
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lyrics {
    pub text: String,
    /// Who provides them ("Source: LyricFind").
    pub source: String,
}

pub fn lyrics(v: &Value) -> Result<Option<Lyrics>, ParseError> {
    if let Some(e) = server_error(v) {
        return Err(e);
    }
    let Some(r) = find(v, "musicDescriptionShelfRenderer") else {
        // A page with only a message ("Lyrics not available") has no shelf.
        return if find(v, "messageRenderer").is_some() || find(v, "sectionListRenderer").is_some() {
            Ok(None)
        } else {
            Err(ParseError::Unrecognized)
        };
    };
    let body = text(r.get("description"));
    Ok((!body.is_empty()).then(|| Lyrics { text: body, source: text(r.get("footer")) }))
}

#[cfg(test)]
mod tests;

/* ── The account ───────────────────────────────────────────────────────────────── */

/// The active channel, from the account menu.
pub fn account(v: &Value) -> Option<Channel> {
    let h = find(v, "activeAccountHeaderRenderer")?;
    let name = text(h.get("accountName"));
    (!name.is_empty()).then(|| Channel {
        name,
        byline: text(h.get("channelHandle")),
        photo: thumbnails(h.get("accountPhoto")),
        selected: true,
        switch: None,
    })
}

/// The channels to switch to, from the accounts list; each with YouTube Music's own
/// sign-in address for it, kept only when it is one (a path on the same site).
pub fn channels(v: &Value) -> Vec<Channel> {
    let mut out = Vec::new();
    collect_channels(v, &mut out);
    out
}

/// YouTube Music's channel sign-in addresses are relative paths into its own `/signin`.
pub fn switch_url(url: &str) -> bool {
    url.len() <= 2048
        && url.starts_with("/signin?action_handle_signin=true&")
        && url.bytes().all(|b| b.is_ascii_alphanumeric() || b"/?&=%_-.".contains(&b))
}

fn collect_channels(v: &Value, out: &mut Vec<Channel>) {
    match v {
        Value::Object(o) => {
            for (k, r) in o {
                if k == "accountItem" {
                    let name = text(r.get("accountName"));
                    let switch = r
                        .pointer("/serviceEndpoint/selectActiveIdentityEndpoint/supportedTokens")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find_map(|t| str_at(t, "/accountSigninToken/signinUrl"))
                        .filter(|u| switch_url(u))
                        .map(String::from);
                    if !name.is_empty() {
                        out.push(Channel {
                            name,
                            byline: text(r.get("accountByline")),
                            photo: thumbnails(r.get("accountPhoto")),
                            selected: r.get("isSelected").and_then(Value::as_bool).unwrap_or(false),
                            switch,
                        });
                    }
                } else {
                    collect_channels(r, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_channels(x, out)),
        _ => {}
    }
}
