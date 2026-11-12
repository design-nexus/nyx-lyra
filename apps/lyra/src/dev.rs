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
            Event::Failed { id, command, failure } => {
                json!({ "event": "failed", "id": id, "command": command, "failure": failure })
            }
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
        "like" => Some(Command::Like { on: c.get("on").and_then(Value::as_bool).unwrap_or(true) }),
        "shuffle" => Some(Command::Shuffle),
        "eq" => {
            let mut bands = [0.0; 10];
            for (i, b) in c.get("bands").and_then(Value::as_array).into_iter().flatten().take(10).enumerate() {
                bands[i] = b.as_f64().unwrap_or(0.0);
            }
            Some(Command::Eq {
                on: c.get("on").and_then(Value::as_bool).unwrap_or(true),
                preamp: num("preamp").unwrap_or(0.0),
                bands,
            })
        }
        // Where the page is and what its media element is doing (for playback checks).
        "where" => {
            return player.dev_eval(
                "JSON.stringify({ url: location.href, media: (() => { const m = document.querySelector('video'); return m && { paused: m.paused, time: m.currentTime, ready: m.readyState, network: m.networkState, error: m.error && m.error.code, src: String(m.currentSrc).slice(0, 40), buffered: m.buffered.length ? [m.buffered.start(0), m.buffered.end(m.buffered.length - 1)] : null }; })() })",
                Box::new(move |r| out(json!({ "event": "where", "result": r }), start)),
            );
        }
        // The streams YouTube Music offers for the song playing.
        "formats" => {
            return player.dev_eval(
                "JSON.stringify((() => { const p = document.getElementById('movie_player'); const r = p && p.getPlayerResponse ? p.getPlayerResponse() : null; if (!r) return null; const f = (r.streamingData && r.streamingData.adaptiveFormats) || []; return { videoId: r.videoDetails && r.videoDetails.videoId, musicVideoType: r.videoDetails && r.videoDetails.musicVideoType, formats: f.map(x => x.itag + ':' + String(x.mimeType).split(';')[0] + (x.audioQuality ? ':' + x.audioQuality : '') + (x.qualityLabel ? ':' + x.qualityLabel : '')), playability: r.playabilityStatus && r.playabilityStatus.status }; })())",
                Box::new(move |r| out(json!({ "event": "formats", "result": r }), start)),
            );
        }
        // Any script in the page, for one-off checks.
        "js" => {
            return player.dev_eval(&s("code"), Box::new(move |r| out(json!({ "event": "js", "result": r }), start)));
        }
        "eq-level" => {
            return player.dev_eval(
                "JSON.stringify(window.__lyraEqLevel ? window.__lyraEqLevel() : 'no bridge')",
                Box::new(move |r| out(json!({ "event": "eq-level", "level": r }), start)),
            );
        }
        "autoplay" => Some(Command::Autoplay { on: c.get("on").and_then(Value::as_bool).unwrap_or(true) }),
        "repeat" => Some(Command::Repeat {
            mode: match s("mode").as_str() {
                "all" => nyx_lyra_core::playback::Repeat::All,
                "one" => nyx_lyra_core::playback::Repeat::One,
                _ => nyx_lyra_core::playback::Repeat::Off,
            },
        }),
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
                "history" => Route::History,
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
        "signin" => return crate::ui::dev_signin(),
        "not-now" => return crate::ui::dev_decline(),
        "settings" => return crate::ui::dev_settings(),
        // Scroll the scroller with this style class to the end, as the wheel would.
        "scroll" => {
            fn find(w: &gtk::Widget, class: &str) -> Option<gtk::ScrolledWindow> {
                if w.has_css_class(class) {
                    return w.clone().downcast().ok();
                }
                let mut c = w.first_child();
                while let Some(x) = c {
                    if let Some(f) = find(&x, class) {
                        return Some(f);
                    }
                    c = x.next_sibling();
                }
                None
            }
            let found = app.active_window().and_then(|w| find(w.upcast_ref(), &s("class")));
            if let Some(sw) = &found {
                let a = sw.vadjustment();
                a.set_value(a.upper() - a.page_size());
            }
            return out(json!({ "event": "scroll", "found": found.is_some() }), start);
        }
        "equalizer" => return crate::ui::equalizer::open(),
        "close-equalizer" => return crate::ui::equalizer::close(),
        "close-settings" => return nyx_ui::settings_card::close(),
        "settings-group" => return nyx_ui::settings_card::select(&s("name")),
        "account-menu" => return crate::ui::dev_account_menu(),
        "script-probe" => {
            return player
                .dev_eval_async(SCRIPT_PROBE, Box::new(move |r| out(json!({ "event": "script-probe", "result": r }), start)));
        }
        // Where YouTube Music keeps its audio quality (for the audio quality work).
        "quality-probe" => {
            return player
                .dev_eval(QUALITY_PROBE, Box::new(move |r| out(json!({ "event": "quality-probe", "result": r }), start)));
        }
        // The stream playing and what WebKit can play (for the audio quality checks).
        "audio" => {
            return player.dev_eval(AUDIO_PROBE, Box::new(move |r| out(json!({ "event": "audio", "result": r }), start)));
        }
        // What YouTube Music's own player bar has (for the bridge's selectors).
        "dom" => {
            return player.dev_eval(DOM_PROBE, Box::new(move |r| out(json!({ "event": "dom", "result": r }), start)));
        }
        // Switch to the channel already active: the whole switch, without changing it.
        "switch-same" => {
            return player.request(
                &youtube::accounts_list(),
                Box::new(move |r| {
                    let path = r.ok().and_then(|v| youtube::channels(&v).into_iter().find(|c| c.selected)).and_then(|c| c.switch);
                    let found = path.is_some();
                    if let Some(path) = path {
                        crate::ui::switch_channel(path);
                    }
                    out(json!({ "event": "switch-same", "found": found }), start)
                }),
            );
        }
        "sign-out" => return player.sign_out(),
        "accounts" => {
            return crate::accounts::google_email(move |email| {
                out(
                    json!({ "event": "accounts", "google": email.is_some(), "domain": email.as_deref().and_then(|e| e.split('@').nth(1)) }),
                    start,
                )
            });
        }
        "ui" => return out(json!({ "event": "ui", "ui": crate::ui::describe() }), start),
        "focus-web" => return out(json!({ "event": "focus-web", "focused": crate::ui::dev_focus_web() }), start),
        // A key as if pressed: the same path as the window's key handler.
        "key" => {
            let Some(key) = gtk::gdk::Key::from_name(s("key")) else {
                return out(json!({ "event": "key", "error": "unknown key" }), start);
            };
            let handled = crate::ui::on_key(key, gtk::gdk::ModifierType::empty()) == gtk::glib::Propagation::Stop;
            return out(json!({ "event": "key", "handled": handled, "ui": crate::ui::describe() }), start);
        }
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
        // As closing the window would (its close request).
        "close-window" => {
            if let Some(w) = app.windows().into_iter().find(|w| w.has_css_class("lyra-window") && !w.has_css_class("lyra-mini")) {
                w.close();
            }
            return;
        }
        "search" | "browse" | "browse-with" | "account" | "channels" | "yt-settings" | "list-start" | "up-next" | "more" => {
            let req = match s("cmd").as_str() {
                "search" => youtube::search(&s("query")),
                "browse" => youtube::browse(&s("id")),
                "browse-with" => youtube::browse_with(&s("id"), &s("params")),
                "account" => Some(youtube::account_menu()),
                "channels" => Some(youtube::accounts_list()),
                "yt-settings" => Some(youtube::settings()),
                "list-start" => youtube::list_start(&s("id")),
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
                        Ok(v) if kind == "up-next" || kind == "list-start" => match youtube::next(&v) {
                            Ok(n) => json!({ "event": "data", "kind": kind, "ms": ms, "queue": n.queue.len(), "lyrics": n.lyrics, "first": n.queue.first().map(|i| &i.title) }),
                            Err(e) => json!({ "event": "data", "kind": kind, "ms": ms, "error": e.to_string() }),
                        },
                        Ok(v) => match youtube::page(&v) {
                            Ok(p) => json!({ "event": "data", "kind": kind, "ms": ms, "header": p.header.as_ref().map(|h| &h.title), "continuation": p.continuation,
                                "shelves": p.shelves.iter().map(|s| json!({ "title": s.title, "items": s.items.len(),
                                    "first": s.items.first().map(|i| json!({ "title": i.title, "kind": i.kind, "playable": i.playable() })),
                                    "lists": s.items.iter().filter_map(|i| i.playlist_id.as_ref()).take(6).collect::<Vec<_>>() })).collect::<Vec<_>>() }),
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
        None => out(
            json!({ "event": "error", "error": "unknown command", "line": line.chars().take(200).collect::<String>() }),
            start,
        ),
    }
}

const DOM_PROBE: &str = r#"(() => {
  const bar = document.querySelector("ytmusic-player-bar");
  const out = { bar: !!bar };
  if (bar) {
    out.attrs = [...bar.attributes].map((a) => a.name + "=" + a.value.slice(0, 40));
    const like = bar.querySelector("ytmusic-like-button-renderer");
    out.like = like ? [...like.attributes].map((a) => a.name + "=" + a.value.slice(0, 30)) : null;
    out.likeButtons = like ? [...like.querySelectorAll("button, [aria-pressed]")].map((e) => e.tagName + "|" + (e.closest("[id]") || {}).id + "|" + e.getAttribute("aria-label") + "|" + e.getAttribute("aria-pressed")) : null;
    out.buttons = [...bar.querySelectorAll("[aria-label],[title]")]
      .map((e) => e.tagName + "|" + String(e.className).slice(0, 50) + "|" + (e.getAttribute("aria-label") || e.getAttribute("title")) + "|" + (e.getAttribute("aria-pressed") || ""))
      .slice(0, 60);
  }
  out.toggles = [...document.querySelectorAll("tp-yt-paper-toggle-button, #automix, [role=switch]")]
    .map((e) => e.tagName + "|" + e.id + "|" + e.getAttribute("aria-pressed") + "|" + e.getAttribute("aria-label") + "|" + e.hasAttribute("checked"));
  return JSON.stringify(out);
})()"#;

const AUDIO_PROBE: &str = r#"(() => {
  const p = document.getElementById("movie_player");
  const stats = p && p.getStatsForNerds ? p.getStatsForNerds() : null;
  const pick = (o) => o && Object.fromEntries(Object.entries(o).filter(([k]) => /codec|bitrate|format|audio|quality|bandwidth|loudness|volume/i.test(k)));
  const types = ['audio/webm; codecs="opus"', 'audio/mp4; codecs="mp4a.40.2"', 'video/webm; codecs="vp9"', 'video/mp4; codecs="avc1.4d401f"'];
  return JSON.stringify({
    stats: pick(stats),
    quality: p && p.getPlaybackQuality ? p.getPlaybackQuality() : null,
    audioTrack: p && p.getAudioTrack ? String(p.getAudioTrack()) : null,
    supported: Object.fromEntries(types.map((t) => [t, window.MediaSource ? MediaSource.isTypeSupported(t) : null])),
  });
})()"#;

const SCRIPT_PROBE: &str = r#"(async () => {
  const urls = performance.getEntriesByType("resource").map((e) => e.name).filter((u) => /\.js(\?|$)/.test(u) && /youtube|ytimg|gstatic/.test(u));
  const out = { scripts: urls.length, values: new Set(), contexts: [] };
  for (const u of urls) {
    try {
      const s = await (await fetch(u)).text();
      for (const m of s.matchAll(/"[A-Z_]*(AUDIO|QUALITY)[A-Z_]*"/g)) out.values.add(m[0]);
      for (const m of s.matchAll(/.{0,200}(applicationSettingsEndpoint|account\/get_setting|get_setting_values|settings_dialog|SETTING_ITEM_ID).{0,200}/g)) if (out.contexts.length < 14) out.contexts.push(m[0]);
    } catch (_) {}
  }
  return JSON.stringify({ scripts: out.scripts, values: [...out.values], contexts: out.contexts });
})()"#;

const QUALITY_PROBE: &str = r#"(() => {
  const cfg = (window.ytcfg && window.ytcfg.data_) || {};
  const keys = Object.keys(cfg).filter((k) => /AUDIO|QUALITY|SETTING/i.test(k));
  const ls = Object.keys(localStorage).filter((k) => /quality|audio|setting|pref/i.test(k));
  const vals = Object.fromEntries(ls.map((k) => [k, String(localStorage.getItem(k)).slice(0, 300)]));
  return JSON.stringify({ cfg: Object.fromEntries(keys.map((k) => [k, JSON.stringify(cfg[k]).slice(0, 300)])), localStorage: vals });
})()"#;
