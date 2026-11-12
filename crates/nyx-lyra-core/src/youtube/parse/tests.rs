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
    assert_eq!(
        p.shelves.iter().map(|s| (s.title.as_str(), s.items.len())).collect::<Vec<_>>(),
        [("Top result", 1), (RESULTS, 3)]
    );
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
    assert_eq!(
        page(&json!({ "error": { "code": 401, "message": "Unauthorized" } })),
        Err(ParseError::Server { code: 401, message: "Unauthorized".into() })
    );
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
            { "playlistPanelVideoWrapperRenderer": {
                "primaryRenderer": { "playlistPanelVideoRenderer": { "title": { "runs": [ { "text": "Reunion" } ] }, "videoId": "r3un10n_M83",
                    "lengthText": { "runs": [ { "text": "3:55" } ] } } },
                "counterpart": [ { "counterpartRenderer": { "playlistPanelVideoRenderer": {
                    "title": { "runs": [ { "text": "Reunion (Official Video)" } ] }, "videoId": "r3un10n_omv" } } } ] } } ] } } } } } },
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
    assert_eq!(
        lyrics(&json!({ "contents": { "sectionListRenderer": { "contents": [ { "messageRenderer": {} } ] } } })),
        Ok(None)
    );
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
        if let Some(q) = audio_quality(&v) {
            println!("{name}: settings, audio quality {:?} of {:?}", q.selected, q.options);
            assert!(q.highest().is_some());
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
        let links = p.items().filter(|i| i.kind == Kind::Link).count();
        let ranked = p.items().filter(|i| i.rank.is_some()).count();
        let liked = p.items().filter(|i| i.liked == Some(true)).count();
        println!(
            "    chips {}, sorts {} ({:?}), links {links}, ranked {ranked}, liked {liked}, artist {:?}, list {:?}",
            p.chips.len(),
            p.sorts.len(),
            p.sorts.iter().find(|s| s.selected).map(|s| &s.title),
            p.header.as_ref().and_then(|h| h.artist.as_ref()).map(|a| (a.shuffle.is_some(), a.radio.is_some(), a.subscribed)),
            p.header.as_ref().and_then(|h| h.list.as_ref()).map(|l| (
                l.play.is_some(),
                l.shuffle.is_some(),
                l.saved,
                l.by.as_ref().map(|b| &b.name)
            )),
        );
        for c in p.chips.iter().chain(&p.sorts) {
            assert!(!c.title.is_empty() && c.params.as_deref().is_none_or(|t| !t.is_empty()), "{name}: {c:?}");
        }
        for i in p.items().filter(|i| i.kind == Kind::Link) {
            assert!(i.browse_id.is_some(), "{name}: a link without a page: {i:?}");
        }
        assert!(n > 0 || p.header.is_some());
        assert_eq!(untitled, 0);
        assert_eq!(unplayable, 0, "{name}: every item should play or open");
    }
    assert!(seen > 0);
}

#[test]
fn the_active_channel_and_the_channels_to_switch_to() {
    let menu = json!({ "actions": [ { "openPopupAction": { "popup": { "multiPageMenuRenderer": { "header": {
        "activeAccountHeaderRenderer": {
            "accountName": { "runs": [ { "text": "Music" } ] },
            "channelHandle": { "runs": [ { "text": "@KenSmith-Music" } ] },
            "accountPhoto": { "thumbnails": [ { "url": "https://yt3.ggpht.com/p=s108", "width": 108, "height": 108 } ] } } } } } } } ] });
    let a = account(&menu).unwrap();
    assert_eq!((a.name.as_str(), a.byline.as_str(), a.photo.len()), ("Music", "@KenSmith-Music", 1));
    assert_eq!(account(&json!({})), None);

    let item = |name: &str, url: &str, selected: bool| {
        json!({ "accountItem": {
        "accountName": { "runs": [ { "text": name } ] },
        "accountByline": { "runs": [ { "text": "No subscribers" } ] },
        "isSelected": selected,
        "serviceEndpoint": { "selectActiveIdentityEndpoint": { "supportedTokens": [
            { "pageIdToken": { "pageId": "1" } }, { "accountSigninToken": { "signinUrl": url } } ] } } } })
    };
    let list = json!({ "actions": [ { "getMultiPageMenuAction": { "menu": { "multiPageMenuRenderer": { "sections": [
        { "accountSectionListRenderer": { "contents": [ { "accountItemSectionRenderer": { "contents": [
            item("Music", "/signin?action_handle_signin=true&authuser=0&pageid=104&next=%2F", true),
            item("Evil", "https://evil.example/signin?action_handle_signin=true&x", false),
            item("Quote", "/signin?action_handle_signin=true&next=\"x\"", false) ] } } ] } } ] } } } } ] });
    let c = channels(&list);
    assert_eq!(c.len(), 3);
    assert!(c[0].selected && c[0].switch.is_some());
    assert_eq!(c[1].switch, None, "another site's address");
    assert_eq!(c[2].switch, None, "characters a sign-in path doesn't have");
}

#[test]
fn moods_sorts_and_tile_shelves() {
    let chip = |t: &str, params: &str, selected: bool| {
        json!({ "chipCloudChipRenderer": {
        "text": { "runs": [ { "text": t } ] }, "isSelected": selected,
        "navigationEndpoint": { "browseEndpoint": { "browseId": "FEmusic_home", "params": params } } } })
    };
    let sort = |t: &str, token: &str| {
        json!({ "musicMultiSelectMenuItemRenderer": {
        "title": { "runs": [ { "text": t } ] },
        "selectedCommand": { "commandExecutorCommand": { "commands": [ { "musicCheckboxFormItemMutatedCommand": {} },
            { "browseSectionListReloadEndpoint": { "continuation": { "reloadContinuationData": { "continuation": token } } } } ] } } } })
    };
    let v = json!({ "contents": { "sectionListRenderer": {
        "header": { "chipCloudRenderer": { "chips": [ chip("Podcasts", "p0", false), chip("Relax", "p1", false), chip("Focus", "p2", true) ] },
            "musicSideAlignedItemRenderer": { "endItems": [ { "musicSortFilterButtonRenderer": {
                "accessibility": { "accessibilityData": { "label": "Sort by - A to Z selected, dropdown" } },
                "menu": { "musicMultiSelectMenuRenderer": { "options": [ sort("Recently saved", "t1"), sort("A to Z", "t2") ] } } } } ] } },
        "contents": [ { "musicCarouselShelfRenderer": {
            "header": { "musicCarouselShelfBasicHeaderRenderer": { "title": { "runs": [ { "text": "Listen again" } ] } } },
            "contents": [ album_tile() ] } },
          { "musicCarouselShelfRenderer": {
            "header": { "musicCarouselShelfBasicHeaderRenderer": { "title": { "runs": [ { "text": "Quick picks" } ] } } },
            "contents": [ song_row() ] } } ] } } });
    let p = page(&v).unwrap();
    assert_eq!(p.chips.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["Relax", "Focus"], "no podcasts");
    assert!(p.chips[1].selected && p.chips[1].params.as_deref() == Some("p2"));
    assert_eq!(p.sorts.len(), 2);
    assert!(!p.sorts[0].selected && p.sorts[1].selected);
    assert_eq!(p.sorts[1].params.as_deref(), Some("t2"));
    assert!(p.shelves[0].tiles && !p.shelves[1].tiles);
}

#[test]
fn explore_buttons_moods_and_ranks() {
    let button = |t: &str, id: &str, color: Option<u64>| {
        let mut b = json!({ "musicNavigationButtonRenderer": { "buttonText": { "runs": [ { "text": t } ] },
            "clickCommand": { "browseEndpoint": { "browseId": id, "params": "p1" } } } });
        if let Some(c) = color {
            b["musicNavigationButtonRenderer"]["solid"] = json!({ "leftStripeColor": c });
        }
        b
    };
    let mut ranked = song_row();
    ranked["musicResponsiveListItemRenderer"]["customIndexColumn"] = json!({ "musicCustomIndexColumnRenderer": {
        "text": { "runs": [ { "text": "1" } ] }, "icon": { "iconType": "ARROW_DROP_UP" } } });
    let v = json!({ "contents": [
        { "gridRenderer": { "items": [ button("New releases", "FEmusic_new_releases", None), button("Podcasts", "FEmusic_non_music_audio", None) ] } },
        { "musicCarouselShelfRenderer": { "header": { "musicCarouselShelfBasicHeaderRenderer": { "title": { "runs": [ { "text": "Moods & genres" } ] } } },
            "contents": [ button("Chill", "FEmusic_moods_and_genres_category", Some(0xFF2DD4BF)) ] } },
        { "musicCarouselShelfRenderer": { "header": { "musicCarouselShelfBasicHeaderRenderer": { "title": { "runs": [ { "text": "Top songs" } ] } } },
            "contents": [ ranked ] } } ] });
    let p = page(&v).unwrap();
    assert_eq!(p.shelves[0].items.len(), 1, "no podcasts");
    assert_eq!(p.shelves[0].items[0].kind, Kind::Link);
    assert_eq!(p.shelves[1].items[0].color, Some(0x2DD4BF));
    assert_eq!(p.shelves[1].items[0].params.as_deref(), Some("p1"));
    let song = &p.shelves[2].items[0];
    assert_eq!((song.rank.as_deref(), song.trend), (Some("1"), Some(1)));
}

#[test]
fn the_accounts_audio_quality() {
    let option = |name: &str, value: &str, selected: bool| {
        json!({ "settingMenuItemRenderer": { "name": name, "selected": selected,
        "updateServiceEndpoint": { "setSettingEndpoint": { "intValue": value, "settingItemId": "304", "settingItemIdForClient": "MUSIC_WEB_AUDIO_QUALITY" } } } })
    };
    let v = json!({ "items": [ { "settingCategoryCollectionRenderer": { "items": [ { "settingSingleOptionMenuRenderer": {
        "itemId": "MUSIC_WEB_AUDIO_QUALITY",
        "items": [ option("Normal", "2", true), option("Low", "1", false), option("High", "3", false) ] } } ] } } ] });
    let q = audio_quality(&v).unwrap();
    assert_eq!(q.item, "304");
    assert_eq!(q.selected.as_deref(), Some("2"));
    assert_eq!(q.highest().map(|(n, v)| (n.as_str(), v.as_str())), Some(("High", "3")));
    let r = super::super::set_audio_quality(&q.item, "3").unwrap();
    assert_eq!(r.wire()["body"]["newValue"]["intValue"], "3");
    assert!(super::super::set_audio_quality("304", "3; drop").is_none());
}
