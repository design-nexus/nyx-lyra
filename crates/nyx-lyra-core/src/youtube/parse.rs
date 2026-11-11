//! Reading YouTube Music's replies into [`crate::model`]. The replies are trees of
//! "renderers"; these are the ones the web client uses for shelves, rows, tiles and headers.

use crate::model::{Header, Item, Kind, Link, Page, Shelf, Thumbnail, parse_duration};
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
        return Err(ParseError::Unrecognized);
    }
    Ok(Page { header, shelves })
}

/* ── Small readers ─────────────────────────────────────────────────────────────── */

fn text(v: Option<&Value>) -> String {
    let Some(v) = v else { return String::new() };
    if let Some(s) = v.get("simpleText").and_then(Value::as_str) {
        return s.to_string();
    }
    v.get("runs").and_then(Value::as_array).map(|r| r.iter().filter_map(|x| x.get("text").and_then(Value::as_str)).collect()).unwrap_or_default()
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

fn video_type(endpoint: &Value) -> Option<&str> {
    str_at(endpoint, "/watchEndpoint/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType")
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
            Some("MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL") => artists.push(Link { name: name.into(), browse_id: id }),
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
        browse_id: row_ep.and_then(|e| str_at(e, "/browseEndpoint/browseId")).or_else(|| title_ep.and_then(|e| str_at(e, "/browseEndpoint/browseId"))).map(String::from),
        thumbnails: thumbnails(r.get("thumbnail")),
        explicit: explicit(r),
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
    })
}

fn row(v: &Value) -> Option<Item> {
    if let Some(r) = v.get("musicResponsiveListItemRenderer") {
        list_item(r)
    } else if let Some(r) = v.get("musicTwoRowItemRenderer") {
        tile(r)
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
                            more: more(h.and_then(|h| h.pointer("/moreContentButton/buttonRenderer/navigationEndpoint")))
                                .or_else(|| more(h.and_then(|h| runs(h.get("title")).first()).and_then(|x| x.get("navigationEndpoint")))),
                        });
                    }
                    "musicShelfRenderer" | "musicPlaylistShelfRenderer" => out.push(Shelf {
                        title: text(r.get("title")),
                        items: rows(r.get("contents")),
                        more: more(r.get("bottomEndpoint")),
                    }),
                    "musicCardShelfRenderer" => {
                        let mut items: Vec<Item> = card(r).into_iter().collect();
                        items.extend(rows(r.get("contents")));
                        let title = text(r.pointer("/header/musicCardShelfHeaderBasicRenderer/title"));
                        out.push(Shelf { title: if title.is_empty() { "Top result".into() } else { title }, items, more: None });
                    }
                    // Search lists each result in its own section; consecutive ones make one shelf.
                    "itemSectionRenderer" => {
                        let items = rows(r.get("contents"));
                        if items.is_empty() {
                            collect_shelves(r, out);
                        } else {
                            match out.last_mut() {
                                Some(last) if last.title == RESULTS => last.items.extend(items),
                                _ => out.push(Shelf { title: RESULTS.into(), items, more: None }),
                            }
                        }
                    }
                    "gridRenderer" => out.push(Shelf {
                        title: text(r.pointer("/header/gridHeaderRenderer/title")),
                        items: rows(r.get("items")),
                        more: None,
                    }),
                    _ => collect_shelves(r, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_shelves(x, out)),
        _ => {}
    }
}

fn description(r: &Value) -> String {
    text(r.get("description").and_then(|d| d.pointer("/musicDescriptionShelfRenderer/description")).or(r.get("description")))
}

fn find_header(v: &Value) -> Option<Header> {
    for key in ["musicResponsiveHeaderRenderer", "musicImmersiveHeaderRenderer", "musicDetailHeaderRenderer", "musicVisualHeaderRenderer"] {
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
            });
        }
    }
    None
}

/* ── Up next and lyrics ────────────────────────────────────────────────────────── */

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Next {
    pub queue: Vec<Item>,
    /// The queue item that's playing.
    pub current: Option<usize>,
    pub lyrics: Option<String>,
    pub related: Option<String>,
}

pub fn next(v: &Value) -> Result<Next, ParseError> {
    if let Some(e) = server_error(v) {
        return Err(e);
    }
    let tabs = v
        .pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs")
        .and_then(Value::as_array)
        .ok_or(ParseError::Unrecognized)?;
    let tab_id = |i: usize| {
        tabs.get(i).and_then(|t| str_at(t, "/tabRenderer/endpoint/browseEndpoint/browseId")).map(String::from)
    };
    let mut out = Next { lyrics: tab_id(1), related: tab_id(2).or_else(|| tab_id(3)), ..Default::default() };
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
        return if find(v, "messageRenderer").is_some() || find(v, "sectionListRenderer").is_some() { Ok(None) } else { Err(ParseError::Unrecognized) };
    };
    let body = text(r.get("description"));
    Ok((!body.is_empty()).then(|| Lyrics { text: body, source: text(r.get("footer")) }))
}

#[cfg(test)]
mod tests;
