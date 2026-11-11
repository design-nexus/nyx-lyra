//! The runtime on WebKitGTK 6.0: one view on music.youtube.com with a persistent session and
//! the bridge script. Only YouTube Music and Google's sign-in pages load in it; other links
//! open in the browser. Popups and permission requests are refused.

use super::{Event, Load, Reply, RequestError, Runtime};
use gtk::{gio, prelude::*};
use nyx_lyra_core::bridge;
use nyx_lyra_core::command::CommandId;
use nyx_lyra_core::youtube::{Request, valid_id};
use serde_json::Value;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};
use webkit6::prelude::*;

pub const HOME: &str = "https://music.youtube.com/";
/// Pages that may load in the view: YouTube Music, and the hosts Google's sign-in passes through.
const HOSTS: &[&str] = &["music.youtube.com", "www.youtube.com", "accounts.youtube.com", "consent.youtube.com", "accounts.google.com", "gds.google.com"];
const MAX_ENDS: usize = 3;
const SIGN_IN: &str = "https://accounts.google.com/ServiceLogin?ltmpl=music&service=youtube&passive=true&continue=https%3A%2F%2Fwww.youtube.com%2Fsignin%3Faction_handle_signin%3Dtrue%26app%3Ddesktop%26next%3Dhttps%253A%252F%252Fmusic.youtube.com%252F";

pub struct WebRuntime {
    view: webkit6::WebView,
    on_event: Rc<dyn Fn(Event)>,
}

fn allowed(uri: &str) -> bool {
    let Some(rest) = uri.strip_prefix("https://") else { return uri == "about:blank" };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    HOSTS.contains(&host) || host.ends_with(".google.com") && host.starts_with("accounts.")
}

impl WebRuntime {
    pub fn new(data: &Path, cache: &Path, on_event: Rc<dyn Fn(Event)>) -> Rc<Self> {
        for d in [data, cache] {
            let _ = std::fs::create_dir_all(d);
            let _ = std::fs::set_permissions(d, <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700));
        }
        let session = webkit6::NetworkSession::new(data.to_str(), cache.to_str());
        if let Some(cookies) = session.cookie_manager() {
            cookies.set_persistent_storage(&data.join("cookies.sqlite").to_string_lossy(), webkit6::CookiePersistentStorage::Sqlite);
        }

        let settings = webkit6::Settings::new();
        settings.set_enable_javascript(true);
        settings.set_enable_media(true);
        settings.set_enable_mediasource(true);
        // Without Encrypted Media, YouTube picks streams that fail (MediaError 4) here.
        settings.set_enable_encrypted_media(true);
        settings.set_enable_webaudio(true);
        settings.set_media_playback_requires_user_gesture(false);
        // Old pages kept for Back each hold a web process; Lyra never goes back in the page.
        settings.set_enable_page_cache(false);
        settings.set_enable_developer_extras(std::env::var_os("NYX_LYRA_INSPECTOR").is_some());

        let content = webkit6::UserContentManager::new();
        content.register_script_message_handler("lyra", None);
        content.add_script(&webkit6::UserScript::new(
            nyx_lyra_core::BRIDGE_JS,
            webkit6::UserContentInjectedFrames::TopFrame,
            webkit6::UserScriptInjectionTime::Start,
            &["https://music.youtube.com/*"],
            &[],
        ));
        let ev = on_event.clone();
        content.connect_script_message_received(Some("lyra"), move |_, value| {
            ev(match bridge::parse(&value.to_str()) {
                Ok(bridge::Message::Hello { .. }) => Event::Hello,
                Ok(bridge::Message::State(s)) => Event::State(s),
                Err(e) => Event::Rejected(e),
            })
        });

        // WebKit's default autoplay is "without sound"; Lyra plays only when asked to.
        let policies = webkit6::WebsitePolicies::builder().autoplay(webkit6::AutoplayPolicy::Allow).build();
        let view = webkit6::WebView::builder()
            .network_session(&session)
            .user_content_manager(&content)
            .settings(&settings)
            .website_policies(&policies)
            .build();

        view.connect_decide_policy(|_, decision, kind| {
            use webkit6::PolicyDecisionType as T;
            if !matches!(kind, T::NavigationAction | T::NewWindowAction) {
                return false;
            }
            let Some(nav) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() else { return false };
            let Some(uri) = nav.navigation_action().and_then(|a| a.request()).and_then(|r| r.uri()) else { return false };
            if kind == T::NavigationAction && allowed(&uri) {
                return false;
            }
            if uri.starts_with("https://") {
                let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
            }
            decision.ignore();
            true
        });
        view.connect_create(|_, _| None);
        view.connect_permission_request(|_, request| {
            request.deny();
            true
        });
        let ev = on_event.clone();
        view.connect_load_failed(move |_, _, uri, error| {
            // A load replaced by another (Lyra loading a song while Home loads) isn't a failure.
            let replaced = error.matches(webkit6::NetworkError::Cancelled) || error.matches(webkit6::PolicyError::FrameLoadInterruptedByPolicyChange);
            if allowed(uri) && !replaced {
                ev(Event::LoadFailed(error.message().to_string()));
            }
            false
        });
        let ends: RefCell<Vec<Instant>> = RefCell::default();
        let ev = on_event.clone();
        view.connect_web_process_terminated(move |_, reason| {
            let mut e = ends.borrow_mut();
            e.retain(|t| t.elapsed() < Duration::from_secs(60));
            e.push(Instant::now());
            eprintln!("nyx-lyra: the web process ended ({reason:?})");
            ev(Event::Ended { again: e.len() <= MAX_ENDS });
        });
        Rc::new(Self { view, on_event })
    }
}

impl Runtime for WebRuntime {
    fn send(&self, id: CommandId, wire: Value) {
        let script = format!("window.__lyraCommand ? window.__lyraCommand({wire}) : '\"no-bridge\"'");
        let ev = self.on_event.clone();
        self.view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, move |r| {
            let answer = match r {
                Ok(v) => serde_json::from_str::<Value>(&v.to_str()).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "refused".into()),
                Err(_) => "no-bridge".into(),
            };
            ev(Event::Answer(id, answer))
        });
    }

    fn request(&self, request: &Request, reply: Reply) {
        let body = format!("return window.__lyraData ? await window.__lyraData({}) : JSON.stringify({{ error: 'no-bridge' }});", request.wire());
        self.view.call_async_javascript_function(&body, None, None, None, gio::Cancellable::NONE, move |r| {
            let out: Result<Value, RequestError> = (|| {
                let v = r.map_err(|e| RequestError::Failed(e.message().to_string()))?;
                let v: Value = serde_json::from_str(&v.to_str()).map_err(|_| RequestError::Failed("an unreadable answer".into()))?;
                match v.get("error").and_then(Value::as_str) {
                    Some("no-bridge" | "no-client-context") => return Err(RequestError::NotReady),
                    Some(e) => return Err(RequestError::Failed(e.chars().take(64).collect())),
                    None => {}
                }
                match v.get("status").and_then(Value::as_u64).unwrap_or(0) as u16 {
                    200 => {}
                    401 | 403 => return Err(RequestError::SignedOut),
                    code => return Err(RequestError::Http(code)),
                }
                let text = v.get("text").and_then(Value::as_str).unwrap_or_default();
                serde_json::from_str(text).map_err(|_| RequestError::Failed("YouTube Music's answer wasn't JSON".into()))
            })();
            reply(out)
        });
    }

    fn load(&self, target: Load) {
        let ok = match &target {
            Load::Home => true,
            Load::Watch { video_id, playlist_id, .. } => valid_id(video_id) && playlist_id.as_deref().is_none_or(valid_id),
            Load::List { playlist_id } => valid_id(playlist_id),
            Load::SignIn => true,
        };
        if !ok {
            eprintln!("nyx-lyra: refused to load {target:?}");
            return;
        }
        let uri = match target {
            Load::Home => HOME.to_string(),
            Load::Watch { video_id, playlist_id, at } => {
                let mut u = format!("{HOME}watch?v={video_id}");
                if let Some(p) = playlist_id {
                    u.push_str(&format!("&list={p}"));
                }
                if let Some(t) = at.filter(|t| *t > 0) {
                    u.push_str(&format!("&t={t}s"));
                }
                u
            }
            Load::List { playlist_id } => format!("{HOME}watch?list={playlist_id}"),
            Load::SignIn => SIGN_IN.to_string(),
        };
        self.view.load_uri(&uri);
    }

    fn set_muted(&self, muted: bool) {
        self.view.set_is_muted(muted);
    }

    fn widget(&self) -> gtk::Widget {
        self.view.clone().upcast()
    }

    fn terminate(&self) {
        self.view.terminate_web_process();
    }
}