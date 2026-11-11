//! Phase 1 probe: YouTube Music in a WebKitGTK 6.0 view, a persistent web session, and the
//! bridge in bridge.js. Commands arrive as JSON lines on stdin; events leave as JSON lines on
//! stdout, each with `t`, the milliseconds since start.
//!
//! Installed, it is Lyra Preview: one instance, and closing the window while music plays hides
//! it until the music stops. Environment: LYRA_PROBE_APP_ID (a test copy: its own application
//! ID, more than one instance, hidden rather than closed), LYRA_PREVIEW_APP_ID (another ID with
//! the installed behaviour), LYRA_PROBE_MUTE=1 (mute the view),
//! LYRA_PROBE_INSPECTOR=1 (developer tools). The first argument is the page to open.

use gtk::{gio, glib, prelude::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};
use webkit6::prelude::*;

const BRIDGE: &str = include_str!("bridge.js");
const HOME: &str = "https://music.youtube.com/";
const MAX_MESSAGE: usize = 16 * 1024;
const MAX_RESTARTS: usize = 3;
static START: OnceLock<Instant> = OnceLock::new();

fn emit(mut event: Value) {
    let t = START.get().map_or(0, |s| s.elapsed().as_millis() as u64);
    if let Some(o) = event.as_object_mut() {
        o.insert("t".into(), t.into());
    }
    println!("{event}");
}

fn dir(var: &str, fallback: &str) -> PathBuf {
    let base = std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback));
    base.join("nyx-lyra-probe")
}

// Messages from the page are untrusted: only a JSON object of a known kind, of bounded size.
fn bridge_message(raw: &str) -> Option<Value> {
    if raw.len() > MAX_MESSAGE {
        return None;
    }
    let v: Value = serde_json::from_str(raw).ok()?;
    matches!(v.get("kind")?.as_str()?, "state" | "hello").then_some(v)
}

fn main() -> glib::ExitCode {
    START.get_or_init(Instant::now);
    let test = std::env::var("LYRA_PROBE_APP_ID").ok();
    let uri = std::env::args().nth(1).unwrap_or_else(|| HOME.into());
    glib::set_application_name("Lyra Preview");
    let flags = if test.is_some() { gio::ApplicationFlags::NON_UNIQUE } else { gio::ApplicationFlags::empty() };
    let id = test.clone().or_else(|| std::env::var("LYRA_PREVIEW_APP_ID").ok()).unwrap_or_else(|| APP_ID.into());
    let app = gtk::Application::builder().application_id(id).flags(flags).build();
    app.connect_activate(move |app| match app.windows().first() {
        Some(w) => w.present(),
        None => build(app, &uri, test.is_some()),
    });
    app.run_with_args(&["nyx-lyra-probe"])
}

const APP_ID: &str = "io.github.design_nexus.nyx.LyraPreview";

// Closing the window while music plays hides it; it closes once the music has stopped for a
// while (a short gap between songs isn't stopping).
fn keep_playing_when_closed(window: &gtk::ApplicationWindow, view: &webkit6::WebView) {
    let v = view.clone();
    window.connect_close_request(move |w| {
        if v.is_playing_audio() {
            w.set_visible(false);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    let w = window.downgrade();
    view.connect_is_playing_audio_notify(move |v| {
        if v.is_playing_audio() {
            return;
        }
        let (w, v) = (w.clone(), v.clone());
        glib::timeout_add_local_once(Duration::from_secs(15), move || {
            if let Some(w) = w.upgrade()
                && !w.is_visible()
                && !v.is_playing_audio()
            {
                w.destroy();
            }
        });
    });
}

fn build(app: &gtk::Application, uri: &str, test: bool) {
    let data = dir("XDG_DATA_HOME", ".local/share");
    let cache = dir("XDG_CACHE_HOME", ".cache");
    for d in [&data, &cache] {
        std::fs::create_dir_all(d).expect("create the probe's directories");
        let _ = std::fs::set_permissions(d, <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700));
    }
    let session = webkit6::NetworkSession::new(data.join("web").to_str(), cache.join("web").to_str());
    if let Some(cookies) = session.cookie_manager() {
        cookies.set_persistent_storage(data.join("cookies.sqlite").to_str().unwrap(), webkit6::CookiePersistentStorage::Sqlite);
    }

    let settings = webkit6::Settings::new();
    settings.set_enable_javascript(true);
    settings.set_enable_media(true);
    settings.set_enable_mediasource(true);
    settings.set_enable_encrypted_media(true);
    settings.set_enable_webaudio(true);
    settings.set_media_playback_requires_user_gesture(false);
    settings.set_enable_developer_extras(std::env::var_os("LYRA_PROBE_INSPECTOR").is_some());

    let content = webkit6::UserContentManager::new();
    content.register_script_message_handler("lyra", None);
    content.add_script(&webkit6::UserScript::new(
        BRIDGE,
        webkit6::UserContentInjectedFrames::TopFrame,
        webkit6::UserScriptInjectionTime::Start,
        &["https://music.youtube.com/*"],
        &[],
    ));
    content.connect_script_message_received(Some("lyra"), |_, value| {
        let raw = value.to_str();
        match bridge_message(&raw) {
            Some(v) => emit(json!({ "event": "bridge", "message": v })),
            None => emit(json!({ "event": "rejected", "bytes": raw.len() })),
        }
    });

    // WebKit's default lets pages autoplay only without sound; Lyra plays when asked to.
    let policies = webkit6::WebsitePolicies::builder().autoplay(webkit6::AutoplayPolicy::Allow).build();
    let view = webkit6::WebView::builder()
        .network_session(&session)
        .user_content_manager(&content)
        .settings(&settings)
        .website_policies(&policies)
        .build();
    if std::env::var("LYRA_PROBE_MUTE").as_deref() == Ok("1") {
        view.set_is_muted(true);
    }
    view.connect_load_changed(|v, e| {
        emit(json!({ "event": "load", "stage": format!("{e:?}"), "uri": v.uri().map(|u| u.to_string()) }))
    });
    view.connect_is_playing_audio_notify(|v| emit(json!({ "event": "audio", "playing": v.is_playing_audio() })));
    view.connect_create(|_, action| {
        let uri = action.request().and_then(|r| r.uri()).map(|u| u.to_string());
        emit(json!({ "event": "popup-refused", "uri": uri }));
        None
    });
    view.connect_permission_request(|_, request| {
        emit(json!({ "event": "permission-denied", "type": request.type_().name() }));
        request.deny();
        true
    });

    // A crashed web process reloads, at most MAX_RESTARTS times a minute.
    let restarts: Rc<RefCell<Vec<Instant>>> = Rc::default();
    view.connect_web_process_terminated(move |v, reason| {
        let mut r = restarts.borrow_mut();
        r.retain(|t| t.elapsed() < Duration::from_secs(60));
        let again = r.len() < MAX_RESTARTS;
        emit(json!({ "event": "web-process-terminated", "reason": format!("{reason:?}"), "reloading": again }));
        if again {
            r.push(Instant::now());
            let v = v.clone();
            glib::timeout_add_local_once(Duration::from_secs(2), move || v.reload());
        }
    });

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(if test { "Lyra probe" } else { "Lyra Preview" })
        .default_width(1280)
        .default_height(800)
        .child(&view)
        .build();
    if test {
        window.set_hide_on_close(true);
        std::mem::forget(app.hold());
    } else {
        keep_playing_when_closed(&window, &view);
    }
    window.present();
    emit(json!({ "event": "window" }));
    view.load_uri(uri);

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let seq = Cell::new(0u64);
    let app = app.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || {
        while let Ok(line) = rx.try_recv() {
            seq.set(seq.get() + 1);
            command(&app, &window, &view, &line, seq.get());
        }
        glib::ControlFlow::Continue
    });
}

fn command(app: &gtk::Application, window: &gtk::ApplicationWindow, view: &webkit6::WebView, line: &str, id: u64) {
    let Ok(c) = serde_json::from_str::<Value>(line) else {
        return emit(json!({ "event": "ack", "id": id, "error": "not JSON" }));
    };
    let name = c.get("cmd").and_then(Value::as_str).unwrap_or_default();
    let done = |result: Value| emit(json!({ "event": "ack", "id": id, "cmd": name, "result": result }));
    match name {
        "load" => match c.get("uri").and_then(Value::as_str) {
            Some(u) if u.starts_with("https://") => {
                view.load_uri(u);
                done("loading".into())
            }
            _ => done("bad-argument".into()),
        },
        "reload" => {
            view.reload();
            done("reloading".into())
        }
        "hide" => {
            window.set_visible(false);
            done("hidden".into())
        }
        "close" => {
            window.close();
            done(window.is_visible().into())
        }
        "show" => {
            window.set_visible(true);
            done("shown".into())
        }
        "mute" | "unmute" => {
            view.set_is_muted(name == "mute");
            done(view.is_muted().into())
        }
        "crash" => {
            view.terminate_web_process();
            done("terminated".into())
        }
        "features" => {
            let list = webkit6::Settings::all_features().map_or_else(Vec::new, |l| {
                (0..l.length())
                    .filter_map(|i| l.get(i))
                    .filter(|f| {
                        let id = f.identifier().unwrap_or_default().to_lowercase();
                        ["media", "session", "mse", "audio", "encrypted", "webcodecs"].iter().any(|k| id.contains(k))
                    })
                    .map(|f| json!({ "id": f.identifier().map(|s| s.to_string()), "on": f.is_default_value(), "status": format!("{:?}", f.status()) }))
                    .collect()
            });
            done(list.into())
        }
        "quit" => {
            done("quitting".into());
            app.quit()
        }
        "" => done("no-command".into()),
        _ => {
            let script = format!("window.__lyraCommand ? window.__lyraCommand({c}) : '\"no-bridge\"'");
            let name = name.to_string();
            view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, move |r| {
                let result = match r {
                    Ok(v) => serde_json::from_str::<Value>(&v.to_str()).unwrap_or(Value::Null),
                    Err(e) => json!({ "error": e.message() }),
                };
                emit(json!({ "event": "ack", "id": id, "cmd": name, "result": result }))
            });
        }
    }
}
