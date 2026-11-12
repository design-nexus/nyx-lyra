//! Lyra's actions on the control interface (`nyx_core::control`, API 2), for the `nyx`
//! command, Glimpse and Nyx Assistant: play, pause and skip (answered once YouTube Music
//! confirms), volume, what's playing, search, play something by name, open the window, and
//! the live `playback` topic Glimpse's Lyra panel shows.

use crate::{art, player};
use gtk::prelude::*;
use nyx_core::control::text;
use nyx_lyra_core::command::{Command, Failure};
use nyx_lyra_core::model::{Item, Kind};
use nyx_lyra_core::playback::{Connection, Playback, Status};
use nyx_lyra_core::youtube;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

type Reply = Box<dyn FnOnce(Result<Value, String>)>;

/// Lyra's description: its actions, topic and Glimpse panel (`app_id` is the running ID,
/// which a test copy changes).
pub fn description(app_id: &str) -> Value {
    let alias = |word: &str, action: &str| json!({"word": word, "arguments": {"action": action}});
    json!({
        "tools": [
            {"name": "lyra.play", "description": "Play something on YouTube Music in Lyra: a song, album, artist or playlist by name (the best match), or one of your playlists by its ID from lyra.playlists. With neither, resume.",
             "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}, "playlist": {"type": "string"}}, "additionalProperties": false},
             "annotations": {"title": "Play on YouTube Music"}, "x-nyx": {"risk": "writes", "confirmation": "never", "positional": ["query"], "result": "object"}},
            {"name": "lyra.control", "description": "Play, pause, toggle, next or previous in Lyra. Answers once YouTube Music confirms it.",
             "inputSchema": {"type": "object", "properties": {"action": {"enum": ["play", "pause", "toggle", "next", "previous", "like", "unlike", "shuffle", "repeat", "repeat-one", "repeat-off"]}}, "required": ["action"], "additionalProperties": false},
             "annotations": {"title": "Playback control"},
             "x-nyx": {"risk": "writes", "confirmation": "never", "positional": ["action"], "result": "object",
                       "aliases": [alias("pause", "pause"), alias("resume", "play"), alias("toggle", "toggle"), alias("next", "next"), alias("skip", "next"), alias("previous", "previous"), alias("prev", "previous"), alias("like", "like"), alias("unlike", "unlike"), alias("shuffle", "shuffle"), alias("repeat", "repeat")]}},
            {"name": "lyra.volume", "description": "Set Lyra's volume, 0 to 100.",
             "inputSchema": {"type": "object", "properties": {"level": {"type": "number", "minimum": 0, "maximum": 100}}, "required": ["level"], "additionalProperties": false},
             "annotations": {"title": "Volume"}, "x-nyx": {"risk": "writes", "confirmation": "never", "positional": ["level"], "aliases": ["vol"], "result": "object"}},
            {"name": "lyra.status", "description": "What Lyra is playing: song, artist, album, playing or paused, position, volume.",
             "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
             "annotations": {"title": "Now playing", "readOnlyHint": true}, "x-nyx": {"icon": "audio-x-generic-symbolic", "risk": "read_only", "aliases": ["now"], "result": "object"}},
            {"name": "lyra.search", "description": "Search YouTube Music: songs, albums, artists and playlists matching the words.",
             "inputSchema": {"type": "object", "properties": {"query": {"type": "string", "minLength": 1}}, "required": ["query"], "additionalProperties": false},
             "annotations": {"title": "Search YouTube Music", "readOnlyHint": true}, "x-nyx": {"risk": "read_only", "positional": ["query"], "aliases": ["find"], "result": "list"}},
            {"name": "lyra.playlists", "description": "Your playlists on YouTube Music.",
             "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
             "annotations": {"title": "Playlists", "readOnlyHint": true}, "x-nyx": {"risk": "read_only", "result": "list"}},
            {"name": "lyra.open", "description": "Open Lyra's window, at a page when given.",
             "inputSchema": {"type": "object", "properties": {"page": {"enum": ["home", "explore", "search", "library", "liked", "now-playing"]}}, "additionalProperties": false},
             "annotations": {"title": "Open Lyra"}, "x-nyx": {"risk": "writes", "confirmation": "never", "positional": ["page"], "result": "none"}}
        ],
        "nyx": {
            "api": 2, "app": app_id, "name": "Lyra", "icon": crate::APP_ID, "namespaces": ["lyra"], "aliases": ["youtube-music", "ytm"],
            "bus_name": app_id, "activation": "dbus", "default": "lyra.search", "open": {"action": "lyra.open"},
            "topics": [{"name": "playback"}, {"name": "playlists"}],
            "panel": {"id": "lyra", "topics": ["playback", "playlists"], "size": {"min": [420, 300], "preferred": [500, 360]}}
        }
    })
}

thread_local! {
    static SERVICE: RefCell<Option<nyx_core::control::Service>> = const { RefCell::new(None) };
    static WATCHED: Cell<bool> = const { Cell::new(false) };
    static LAST: RefCell<Value> = const { RefCell::new(Value::Null) };
}

/// Register the interface (from `connect_startup`) and keep `playback` current while watched.
pub fn serve(app: &gtk::Application) {
    let id = app.application_id().map(|s| s.to_string()).unwrap_or_else(|| crate::APP_ID.to_string());
    let app_for = app.clone();
    let Some(service) = nyx_core::control::serve_with(app, description(&id), move |request, answer| {
        handle(
            &app_for,
            &request.action,
            request.arguments,
            Box::new(move |r| answer(r.map_err(|e| nyx_actions::Error::new(nyx_actions::ErrorKind::Failed, e)))),
        );
    }) else {
        return;
    };
    service.on_watch(|topic, on| {
        if topic == "playback" {
            WATCHED.with(|w| w.set(on));
            if on {
                LAST.with(|l| *l.borrow_mut() = Value::Null);
                publish();
            }
        } else if topic == "playlists" && on {
            playlists(|r| {
                let items = r.unwrap_or_default();
                SERVICE.with(|s| {
                    if let Some(service) = s.borrow().as_ref() {
                        service.publish("playlists", json!({"items": items}));
                    }
                });
            });
        }
    });
    SERVICE.with(|s| *s.borrow_mut() = Some(service));
    if let Some(p) = player() {
        p.connect(|e| {
            if matches!(e, nyx_lyra_core::command::Event::State(_)) && WATCHED.with(|w| w.get()) {
                publish();
            }
        });
    }
}

fn publish() {
    let Some(p) = player() else { return };
    let v = playback(&p.state());
    // Only what changed in more than the position's fraction.
    if LAST.with(|l| *l.borrow() == v) {
        return;
    }
    LAST.with(|l| *l.borrow_mut() = v.clone());
    SERVICE.with(|s| {
        if let Some(service) = s.borrow().as_ref() {
            service.publish("playback", v);
        }
    });
}

/// What the panel shows: the song, its cover (a local file), where it is, the volume.
fn playback(s: &Playback) -> Value {
    let state = match s.status {
        Status::Playing => "playing",
        Status::Buffering => "buffering",
        Status::Paused => "paused",
        Status::Stopped => "stopped",
    };
    let art = s.track.as_ref().and_then(|t| t.artwork.as_deref()).and_then(art::file).map(|p| p.to_string_lossy().to_string());
    json!({
        "state": state,
        "connection": match s.connection { Connection::Ready => "ready", Connection::Starting => "starting", Connection::Reconnecting => "reconnecting", Connection::Lost => "lost" },
        "track": s.track.as_ref().map(|t| json!({"title": t.title, "artist": t.artist, "album": t.album.clone().unwrap_or_default()})),
        "position": s.position.map(|p| p.floor()),
        "duration": s.duration.map(|d| d.round()),
        "volume": s.volume,
        "art": art.unwrap_or_default(),
        "can_next": s.controls.next,
        "can_previous": s.controls.previous,
        "signed_in": s.signed_in,
        "liked": s.liked,
        "repeat": s.repeat,
        "autoplay": s.autoplay,
    })
}

fn status(s: &Playback) -> Value {
    let mut v = playback(s);
    v.as_object_mut().map(|o| o.remove("art"));
    v
}

fn why(f: &Failure) -> String {
    match f {
        Failure::Unsupported => "YouTube Music doesn't offer that here".into(),
        Failure::BadArgument => "that isn't possible".into(),
        Failure::NoMedia => "nothing is loaded in Lyra".into(),
        Failure::TimedOut => "YouTube Music didn't confirm it in time".into(),
        Failure::Disconnected => "YouTube Music isn't loaded in Lyra right now".into(),
        Failure::Refused(r) => r.clone(),
    }
}

/// Playing needs YouTube Music's page in a shown window: show it if Lyra was started
/// without one (by D-Bus, for an action).
fn ensure_window(app: &gtk::Application) {
    if app.windows().is_empty() {
        app.activate();
    }
}

fn command(app: &gtk::Application, c: Command, reply: Reply) {
    let Some(p) = player() else { return reply(Err("Lyra isn't ready".into())) };
    if matches!(c, Command::Play | Command::Toggle) {
        ensure_window(app);
    }
    p.send_then(c, move |r| {
        reply(match r {
            Ok(_) => Ok(player().map(|p| status(&p.state())).unwrap_or(Value::Null)),
            Err(f) => Err(why(&f)),
        })
    });
}

fn summary(i: &Item) -> Value {
    let kind = match i.kind {
        Kind::Song => "song",
        Kind::Video => "video",
        Kind::Album => "album",
        Kind::Single => "single",
        Kind::Ep => "ep",
        Kind::Artist => "artist",
        Kind::Playlist => "playlist",
        Kind::Link => "page",
        Kind::Other => "other",
    };
    json!({"title": i.title, "subtitle": i.subtitle, "kind": kind, "value": i.title})
}

fn search(query: &str, done: impl FnOnce(Result<Vec<Item>, String>) + 'static) {
    let (Some(p), Some(req)) = (player(), youtube::search(query)) else { return done(Err("search needs some words".into())) };
    p.request(
        &req,
        Box::new(move |r| {
            done(match r {
                Ok(v) => youtube::page(&v)
                    .map(|pg| pg.items().filter(|i| i.kind != Kind::Other).cloned().collect())
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            })
        }),
    );
}

/// The playlists in the YouTube Music library, as `{name, id}` (the ID plays with `lyra.play`).
fn playlists(done: impl FnOnce(Result<Vec<Value>, String>) + 'static) {
    let (Some(p), Some(req)) = (player(), youtube::browse(youtube::LIBRARY_PLAYLISTS)) else {
        return done(Err("Lyra isn't ready".into()));
    };
    p.request(
        &req,
        Box::new(move |r| {
            done(match r {
                Ok(v) => youtube::page(&v)
                    .map(|pg| {
                        pg.items()
                            .filter(|i| i.kind == Kind::Playlist)
                            .filter_map(|i| {
                                let id = i
                                    .playlist_id
                                    .clone()
                                    .or_else(|| i.browse_id.as_deref().and_then(|b| b.strip_prefix("VL")).map(String::from))?;
                                Some(json!({"name": i.title, "id": id, "title": i.title, "value": id}))
                            })
                            .collect()
                    })
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            })
        }),
    );
}

fn handle(app: &gtk::Application, name: &str, args: Value, reply: Reply) {
    let s = player().map(|p| p.state()).unwrap_or_default();
    match name {
        "lyra.status" => reply(Ok(status(&s))),
        "lyra.control" => {
            let c = match args["action"].as_str() {
                Some("play") => Command::Play,
                Some("pause") => Command::Pause,
                Some("toggle") => Command::Toggle,
                Some("next") => Command::Next,
                Some("previous") => Command::Previous,
                Some("like") => Command::Like { on: true },
                Some("unlike") => Command::Like { on: false },
                Some("shuffle") => Command::Shuffle,
                Some("repeat") => Command::Repeat { mode: nyx_lyra_core::playback::Repeat::All },
                Some("repeat-one") => Command::Repeat { mode: nyx_lyra_core::playback::Repeat::One },
                Some("repeat-off") => Command::Repeat { mode: nyx_lyra_core::playback::Repeat::Off },
                other => return reply(Err(format!("unknown action {}", other.unwrap_or("(none)")))),
            };
            command(app, c, reply)
        }
        "lyra.volume" => match args["level"].as_f64() {
            Some(l) if (0.0..=100.0).contains(&l) => command(app, Command::Volume { level: l.round() as u8 }, reply),
            _ => reply(Err("level must be 0 to 100".into())),
        },
        "lyra.search" => match text(&args, "query") {
            Ok(q) => search(&q, move |r| {
                reply(r.map(|items| json!({"items": items.iter().take(20).map(summary).collect::<Vec<_>>()})))
            }),
            Err(e) => reply(Err(e)),
        },
        "lyra.playlists" => playlists(move |r| reply(r.map(|items| json!({"items": items})))),
        "lyra.play" => {
            if let Some(list) = args["playlist"].as_str().filter(|l| !l.is_empty()) {
                if !youtube::valid_id(list) {
                    return reply(Err("that isn't a playlist ID".into()));
                }
                ensure_window(app);
                return match player() {
                    Some(p) if p.play_ids(None, Some(list.to_string())) => reply(Ok(json!({"playing": {"playlist": list}}))),
                    _ => reply(Err("Lyra couldn't start it".into())),
                };
            }
            let q = args["query"].as_str().unwrap_or_default().trim().to_string();
            if q.is_empty() {
                return command(app, Command::Play, reply);
            }
            let app = app.clone();
            let words = q.clone();
            search(&words, move |r| {
                let items = match r {
                    Ok(i) => i,
                    Err(e) => return reply(Err(e)),
                };
                // The top result if it plays (a song, an album's or playlist's list), else the first song.
                let pick = items
                    .iter()
                    .find(|i| i.playable())
                    .or_else(|| items.iter().find(|i| matches!(i.kind, Kind::Song | Kind::Video)));
                let Some(item) = pick.cloned() else {
                    return reply(Err(format!("nothing on YouTube Music to play for “{q}”")));
                };
                ensure_window(&app);
                match player() {
                    Some(p) if p.play(&item) => reply(Ok(json!({"playing": summary(&item)}))),
                    _ => reply(Err("Lyra couldn't start it".into())),
                }
            });
        }
        "lyra.open" => {
            app.activate();
            use crate::ui::{Route, Tab};
            match args["page"].as_str() {
                Some("explore") => crate::ui::navigate(Route::Explore),
                Some("search") => crate::ui::navigate(Route::Search(String::new())),
                Some("library") => crate::ui::navigate(Route::Library(Tab::Playlists)),
                Some("liked") => crate::ui::navigate(Route::Liked),
                Some("now-playing") => crate::ui::open_now("queue"),
                Some("home") => crate::ui::navigate(Route::Home),
                _ => {}
            }
            reply(Ok(Value::Null))
        }
        _ => reply(Err(format!("unknown action {name}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_is_valid() {
        let d = nyx_actions::Description::from_value(&description(crate::APP_ID), None).expect("valid description");
        assert_eq!(d.app.panel.as_ref().map(|p| p.id.as_str()), Some("lyra"));
        assert_eq!(d.action("lyra.status").unwrap().risk, nyx_actions::Risk::ReadOnly);
        assert_eq!(d.action("lyra.search").unwrap().risk, nyx_actions::Risk::ReadOnly);
        assert!(!d.action("lyra.control").unwrap().asks(), "playback control never asks");
        assert!(!d.action("lyra.open").unwrap().asks());
    }
}
