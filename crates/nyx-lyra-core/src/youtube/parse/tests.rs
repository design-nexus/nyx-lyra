//! Small replies in the shape YouTube Music sends, and a check over real saved replies.

use super::*;
use serde_json::json;

fn browse_ep(id: &str, page: &str) -> Value {
    json!({ "browseEndpoint": { "browseId": id, "browseEndpointContextSupportedConfigs": { "browseEndpointContextMusicConfig": { "pageType": page } } } })
}

fn song_row() -> Value {
    json!({ "musicResponsiveListItemRenderer": {
        "thumbnail": { "musicThumbnailRenderer": { "thumbnail": { "thumbnails": [
            { "url": "https://lh3.googleusercontent.com/a=w60", "width": 60, "height": 60 },
            { "url": "https://lh3.googleusercontent.com/a=w120", "width": 120, "height": 120 } ] } } },
        "flexColumns": [
            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [ { "text": "Instant Crush",
                "navigationEndpoint": { "watchEndpoint": { "videoId": "a5uQMwRMHcs",
                    "watchEndpointMusicSupportedConfigs": { "watchEndpointMusicConfig": { "musicVideoType": "MUSIC_VIDEO_TYPE_ATV" } } } } } ] } } },
            { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [
                { "text": "Song" }, { "text": " • " },
                { "text": "Daft Punk", "navigationEndpoint": browse_ep("UC_daft", "MUSIC_PAGE_TYPE_ARTIST") },
                { "text": " & " },
                { "text": "Julian Casablancas", "navigationEndpoint": browse_ep("UC_jc", "MUSIC_PAGE_TYPE_ARTIST") },
                { "text": " • " },
                { "text": "Random Access Memories", "navigationEndpoint": browse_ep("MPREb_ram", "MUSIC_PAGE_TYPE_ALBUM") },
                { "text": " • " }, { "text": "5:38" } ] } } } ],
        "badges": [ { "musicInlineBadgeRenderer": { "icon": { "iconType": "MUSIC_EXPLICIT_BADGE" } } } ]
    } })
}

fn album_tile() -> Value {
    json!({ "musicTwoRowItemRenderer": {
        "title": { "runs": [ { "text": "Discovery" } ] },
        "subtitle": { "runs": [ { "text": "Album" }, { "text": " • " },
            { "text": "Daft Punk", "navigationEndpoint": browse_ep("UC_daft", "MUSIC_PAGE_TYPE_ARTIST") }, { "text": " • " }, { "text": "2001" } ] },
        "navigationEndpoint": browse_ep("MPREb_disc", "MUSIC_PAGE_TYPE_ALBUM"),
        "thumbnailRenderer": { "musicThumbnailRenderer": { "thumbnail": { "thumbnails": [ { "url": "https://i.ytimg.com/d.jpg", "width": 226, "height": 226 } ] } } },
        "thumbnailOverlay": { "musicItemThumbnailOverlayRenderer": { "content": { "musicPlayButtonRenderer": {
            "playNavigationEndpoint": { "watchPlaylistEndpoint": { "playlistId": "OLAK5uy_disc" } } } } } }
    } })
}

#[test]
fn search_reply() {
    let reply = json!({ "contents": { "tabbedSearchResultsRenderer": { "tabs": [ { "tabRenderer": { "content": { "sectionListRenderer": { "contents": [
        { "musicCardShelfRenderer": {
            "header": { "musicCardShelfHeaderBasicRenderer": { "title": { "runs": [ { "text": "Top result" } ] } } },
            "title": { "runs": [ { "text": "Daft Punk", "navigationEndpoint": browse_ep("UC_daft", "MUSIC_PAGE_TYPE_ARTIST") } ] },
            "subtitle": { "runs": [ { "text": "Artist" }, { "text": " • " }, { "text": "66.6M monthly audience" } ] },
            "contents": [ song_row() ] } },
        { "musicShelfRenderer": { "title": { "runs": [ { "text": "Songs" } ] }, "contents": [ song_row() ],
            "bottomEndpoint": { "searchEndpoint": { "query": "daft punk" } } } }
    ] } } } } ] } } });
    let p = page(&reply).unwrap();
    assert_eq!(p.shelves.len(), 2);
    let top = &p.shelves[0];
    assert_eq!(top.title, "Top result");
    assert_eq!(top.items[0].kind, Kind::Artist);
    assert_eq!(top.items[0].browse_id.as_deref(), Some("UC_daft"));
    let song = &p.shelves[1].items[0];
    assert_eq!(song.kind, Kind::Song);
    assert_eq!(song.title, "Instant Crush");
    assert_eq!(song.video_id.as_deref(), Some("a5uQMwRMHcs"));
    assert_eq!(song.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Daft Punk", "Julian Casablancas"]);
    assert_eq!(song.album.as_ref().unwrap().browse_id.as_deref(), Some("MPREb_ram"));
    assert_eq!(song.duration, Some(338));
    assert!(song.explicit);
    assert_eq!(song.thumbnail(100).unwrap().width, 120);
    assert_eq!(song.subtitle, "Song • Daft Punk & Julian Casablancas • Random Access Memories • 5:38");
}

#[test]
fn search_one_result_per_section() {
    let section = || json!({ "itemSectionRenderer": { "contents": [ song_row() ] } });
    let reply = json!({ "contents": { "sectionListRenderer": { "contents": [
        { "musicCardShelfRenderer": { "title": { "runs": [ { "text": "Daft Punk" } ] } } }, section(), section(), section() ] } } });
    let p = page(&reply).unwrap();
    assert_eq!(p.shelves.iter().map(|s| (s.title.as_str(), s.items.len())).collect::<Vec<_>>(), [("Top result", 1), (RESULTS, 3)]);
}

#[test]
fn home_reply() {
    let reply = json!({ "contents": { "singleColumnBrowseResultsRenderer": { "tabs": [ { "tabRenderer": { "content": { "sectionListRenderer": { "contents": [
        { "musicCarouselShelfRenderer": {
            "header": { "musicCarouselShelfBasicHeaderRenderer": { "title": { "runs": [ { "text": "Listen again" } ] },
                "moreContentButton": { "buttonRenderer": { "navigationEndpoint": { "browseEndpoint": { "browseId": "FEmusic_listen_again" } } } } } },
            "contents": [ album_tile(), { "somethingNew": {} } ] } },
        { "musicCarouselShelfRenderer": { "header": {}, "contents": [] } }
    ] } } } } ] } } });
    let p = page(&reply).unwrap();
    assert_eq!(p.shelves.len(), 1, "empty shelves are dropped");
    let s = &p.shelves[0];
    assert_eq!((s.title.as_str(), s.more.as_deref()), ("Listen again", Some("FEmusic_listen_again")));
    assert_eq!(s.items.len(), 1, "unknown renderers are skipped");
    let a = &s.items[0];
    assert_eq!(a.kind, Kind::Album);
    assert_eq!(a.playlist_id.as_deref(), Some("OLAK5uy_disc"));
    assert_eq!(a.artists[0].name, "Daft Punk");
}

#[test]
fn album_header() {
    let reply = json!({ "contents": { "twoColumnBrowseResultsRenderer": {
        "tabs": [ { "tabRenderer": { "content": { "sectionListRenderer": { "contents": [ { "musicResponsiveHeaderRenderer": {
            "title": { "runs": [ { "text": "Hurry Up, We're Dreaming" } ] },
            "subtitle": { "runs": [ { "text": "Album" }, { "text": " • " }, { "text": "2011" } ] },
            "straplineTextOne": { "runs": [ { "text": "M83" } ] },
            "secondSubtitle": { "runs": [ { "text": "22 songs" }, { "text": " • " }, { "text": "1 hour, 13 minutes" } ] },
            "thumbnail": { "musicThumbnailRenderer": { "thumbnail": { "thumbnails": [ { "url": "https://lh3.googleusercontent.com/h", "width": 544, "height": 544 } ] } } }
        } } ] } } } } ],
        "secondaryContents": { "sectionListRenderer": { "contents": [ { "musicShelfRenderer": { "contents": [ song_row() ] } } ] } } } } });
    let p = page(&reply).unwrap();
    let h = p.header.unwrap();
    assert_eq!(h.title, "Hurry Up, We're Dreaming");
    assert_eq!(h.subtitle, "Album • 2011 • M83");
    assert_eq!(h.second_subtitle, "22 songs • 1 hour, 13 minutes");
    assert_eq!(p.shelves[0].items.len(), 1);
}

#[test]
fn continuations() {
    let first = json!({ "contents": { "twoColumnBrowseResultsRenderer": { "secondaryContents": { "sectionListRenderer": { "contents": [ { "musicPlaylistShelfRenderer": {
        "contents": [ song_row(), { "continuationItemRenderer": { "continuationEndpoint": { "continuationCommand": { "token": "4qmFsgK_abc%3D" } } } } ] } } ] } } } } });
    let p = page(&first).unwrap();
    assert_eq!(p.continuation.as_deref(), Some("4qmFsgK_abc%3D"));
    assert!(crate::youtube::more(p.continuation.as_deref().unwrap()).is_some());
    let next = json!({ "onResponseReceivedActions": [ { "appendContinuationItemsAction": { "continuationItems": [ song_row(), song_row() ] } } ] });
    let p = page(&next).unwrap();
    assert_eq!(p.items().count(), 2);
    assert_eq!(p.continuation, None);
}

#[test]
fn errors() {
    assert_eq!(page(&json!({ "error": { "code": 401, "message": "Unauthorized" } })), Err(ParseError::Server { code: 401, message: "Unauthorized".into() }));
    assert_eq!(page(&json!({ "contents": { "somethingElse": [] } })), Err(ParseError::Unrecognized));
    let signed_out = json!({ "responseContext": { "serviceTrackingParams": [ { "service": "GFEEDBACK", "params": [ { "key": "logged_in", "value": "0" } ] } ] } });
    assert_eq!(logged_in(&signed_out), Some(false));
}

#[test]
fn up_next_and_lyrics() {
    let reply = json!({ "contents": { "singleColumnMusicWatchNextResultsRenderer": { "tabbedRenderer": { "watchNextTabbedResultsRenderer": { "tabs": [
        { "tabRenderer": { "title": "Up next", "content": { "musicQueueRenderer": { "content": { "playlistPanelRenderer": { "contents": [
            { "playlistPanelVideoRenderer": { "title": { "runs": [ { "text": "Midnight City" } ] }, "videoId": "dX3k_QDnzHE", "selected": true,
                "lengthText": { "runs": [ { "text": "4:03" } ] },
                "longBylineText": { "runs": [ { "text": "M83", "navigationEndpoint": browse_ep("UC_m83", "MUSIC_PAGE_TYPE_ARTIST") } ] } } },
            { "playlistPanelVideoRenderer": { "title": { "runs": [ { "text": "Reunion" } ] }, "videoId": "r3un10n_M83",
                "lengthText": { "runs": [ { "text": "3:55" } ] } } } ] } } } } } },
        { "tabRenderer": { "title": "Lyrics", "endpoint": { "browseEndpoint": { "browseId": "MPLYt_x" } } } },
        { "tabRenderer": { "title": "Related", "endpoint": { "browseEndpoint": { "browseId": "MPTRt_x" } } } }
    ] } } } } });
    let n = next(&reply).unwrap();
    assert_eq!(n.queue.len(), 2);
    assert_eq!(n.current, Some(0));
    assert_eq!(n.queue[1].duration, Some(235));
    assert_eq!((n.lyrics.as_deref(), n.related.as_deref()), (Some("MPLYt_x"), Some("MPTRt_x")));
    let l = json!({ "contents": { "sectionListRenderer": { "contents": [ { "musicDescriptionShelfRenderer": {
        "description": { "runs": [ { "text": "line one\nline two" } ] }, "footer": { "runs": [ { "text": "Source: LyricFind" } ] } } } ] } } });
    assert_eq!(lyrics(&l).unwrap().unwrap().source, "Source: LyricFind");
    assert_eq!(lyrics(&json!({ "contents": { "sectionListRenderer": { "contents": [ { "messageRenderer": {} } ] } } })), Ok(None));
}

/// Real replies saved by the probe (`data-N.json` in `LYRA_DUMPS`); run with
/// `LYRA_DUMPS=/tmp/lyra-probe cargo test -p nyx-lyra-core -- --ignored`. They hold the
/// account's own library, so they stay out of the repository.
#[test]
#[ignore]
fn real_replies() {
    let dir = std::env::var("LYRA_DUMPS").expect("LYRA_DUMPS");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !((name.starts_with("data-") || name.starts_with("reply-")) && name.ends_with(".json")) {
            continue;
        }
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        seen += 1;
        if v.pointer("/contents/singleColumnMusicWatchNextResultsRenderer").is_some() {
            let n = next(&v).unwrap();
            println!("{name}: next, {} in the queue, lyrics {:?}", n.queue.len(), n.lyrics);
            assert!(!n.queue.is_empty());
            continue;
        }
        if logged_in(&v) == Some(false) {
            println!("{name}: signed out");
            continue;
        }
        if find(&v, "musicDescriptionShelfRenderer").is_some() && find(&v, "musicResponsiveListItemRenderer").is_none() {
            let l = lyrics(&v).unwrap_or_else(|e| panic!("{name}: {e}"));
            println!("{name}: lyrics, {} lines, {:?}", l.as_ref().map_or(0, |l| l.text.lines().count()), l.map(|l| l.source));
            continue;
        }
        let p = page(&v).unwrap_or_else(|e| panic!("{name}: {e}"));
        if p.shelves.is_empty() && p.header.is_none() {
            println!("{name}: empty");
            continue;
        }
        let n = p.items().count();
        let untitled = p.items().filter(|i| i.title.is_empty()).count();
        let unplayable = p.items().filter(|i| !i.playable() && i.browse_id.is_none()).count();
        let other = p.items().filter(|i| i.kind == Kind::Other).count();
        println!(
            "{name}: {} shelves {:?}, {n} items ({other} other), header {:?}, {unplayable} lead nowhere, more {}",
            p.shelves.len(),
            p.shelves.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
            p.header.as_ref().map(|h| &h.title),
            p.continuation.is_some()
        );
        assert!(n > 0 || p.header.is_some());
        assert_eq!(untitled, 0);
        assert_eq!(unplayable, 0, "{name}: every item should play or open");
    }
    assert!(seen > 0);
}
