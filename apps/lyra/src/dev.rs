//! With NYX_LYRA_DEV=1: commands as JSON lines on stdin and events as JSON lines on stdout,
//! for tools/drive.py and the tests. Off otherwise.

use crate::player::Player;
use gtk::{glib, prelude::*};
use nyx_lyra_core::command::{Command, Event};
use nyx_lyra_core::youtube;
use serde_json::{Value, json};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub fn enabled() -> bool {
    std::env::var("NYX_LYRA_DEV").as_deref() == Ok("1")
}

/// NYX_LYRA_NO_WINDOW=1 (with the dev channel): the window is built but never shown.
pub fn windowless() -> bool {
    enabled() && std::env::var("NYX_LYRA_NO_WINDOW").as_deref() == Ok("1")
}

fn out(mut v: Value, start: Instant) {
    v["t"] = (start.elapsed().as_millis() as u64).into();
    println!("{v}");
}

pub fn start(player: &Rc<Player>, app: &gtk::Application) {
    let start = Instant::now();
    player.connect(move |e| {
        let v = match e {
            Event::State(p) => json!({ "event": "state", "state": p }),
            Event::Confirmed { id, command } => json!({ "event": "confirmed", "id": id, "command": command }),
            Event::Failed { id, command, failure } => json!({ "event": "failed", "id": id, "command": command, "failure": failure }),
        };
        out(v, start);
    });
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let player = player.clone();
    let app = app.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || {
        while let Ok(line) = rx.try_recv() {
            handle(&player, &app, &line, start);
        }
        glib::ControlFlow::Continue
    });
}

fn handle(player: &Rc<Player>, app: &gtk::Application, line: &str, start: Instant) {
    let Ok(c) = serde_json::from_str::<Value>(line) else { return out(json!({ "event": "error", "error": "not JSON" }), start) };
    let num = |k: &str| c.get(k).and_then(Value::as_f64);
    let s = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let command = match s("cmd").as_str() {
        "play" => Some(Command::Play),
        "pause" => Some(Command::Pause),
        "toggle" => Some(Command::Toggle),
        "next" => Some(Command::Next),
        "previous" => Some(Command::Previous),
        "seek" => num("position").map(|p| Command::Seek { position: p }),
        "volume" => num("level").map(|l| Command::Volume { level: l.clamp(0.0, 255.0) as u8 }),
        "state" => return out(json!({ "event": "state", "state": player.state() }), start),
        "mute" => return player.set_muted(true),
        "crash" => return player.terminate(),
        // Screens, for the screenshot checks.
        "nav" => {
            use crate::ui::{Route, Tab};
            let route = match s("to").as_str() {
                "explore" => Route::Explore,
                "search" => Route::Search(s("query")),
                "liked" => Route::Liked,
                "library" => Route::Library(match s("tab").as_str() {
                    "albums" => Tab::Albums,
                    "artists" => Tab::Artists,
                    "songs" => Tab::Songs,
                    _ => Tab::Playlists,
                }),
                "browse" => Route::Browse { id: s("id"), title: s("title") },
                _ => Route::Home,
            };
            crate::ui::navigate(route);
            return out(json!({ "event": "nav" }), start);
        }
        "now" => return crate::ui::open_now(&s("tab")),
        "close-now" => return crate::ui::close_now(),
        "mini" => return crate::ui::mini::toggle(),
        // Map the window the tests built hidden (tools/headless.sh checks the rules first).
        "show" => {
            if let Some(w) = app.windows().first() {
                w.present();
            }
            return out(json!({ "event": "shown" }), start);
        }
        "watch" => {
            let id = |k: &str| c.get(k).and_then(Value::as_str).map(String::from);
            let ok = player.play_ids(id("video"), id("playlist"));
            return out(json!({ "event": "watch", "loading": ok }), start);
        }
        "quit" => return app.quit(),
        "search" | "browse" | "up-next" | "more" => {
            let req = match s("cmd").as_str() {
                "search" => youtube::search(&s("query")),
                "browse" => youtube::browse(&s("id")),
                "more" => youtube::more(&s("token")),
                _ => youtube::up_next(&s("video"), None),
            };
            let Some(req) = req else { return out(json!({ "event": "data", "error": "bad-argument" }), start) };
            let kind = s("cmd");
            let t0 = Instant::now();
            return player.request(
                &req,
                Box::new(move |r| {
                    let ms = t0.elapsed().as_millis() as u64;
                    // NYX_LYRA_DUMP=DIR keeps each raw reply (they hold the account's own data).
                    if let (Ok(v), Some(dir)) = (&r, std::env::var_os("NYX_LYRA_DUMP")) {
                        let _ = std::fs::write(std::path::Path::new(&dir).join(format!("reply-{kind}-{ms}.json")), v.to_string());
                    }
                    let v = match r {
                        Err(e) => json!({ "event": "data", "kind": kind, "error": e.to_string(), "ms": ms }),
                        Ok(v) if kind == "up-next" => match youtube::next(&v) {
                            Ok(n) => json!({ "event": "data", "kind": kind, "ms": ms, "queue": n.queue.len(), "lyrics": n.lyrics, "first": n.queue.first().map(|i| &i.title) }),
                            Err(e) => json!({ "event": "data", "kind": kind, "ms": ms, "error": e.to_string() }),
                        },
                        Ok(v) => match youtube::page(&v) {
                            Ok(p) => json!({ "event": "data", "kind": kind, "ms": ms, "header": p.header.as_ref().map(|h| &h.title), "continuation": p.continuation,
                                "shelves": p.shelves.iter().map(|s| json!({ "title": s.title, "items": s.items.len(),
                                    "first": s.items.first().map(|i| json!({ "title": i.title, "kind": i.kind, "playable": i.playable() })) })).collect::<Vec<_>>() }),
                            Err(e) => json!({ "event": "data", "kind": kind, "ms": ms, "error": e.to_string() }),
                        },
                    };
                    out(v, start)
                }),
            );
        }
        _ => None,
    };
    match command {
        Some(cmd) => {
            let id = player.send(cmd.clone());
            out(json!({ "event": "sent", "id": id, "command": cmd }), start)
        }
        None => out(json!({ "event": "error", "error": "unknown command", "line": line.chars().take(200).collect::<String>() }), start),
    }
}
