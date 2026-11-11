//! The Google account in Nyx Online Accounts, for signing in to YouTube Music with its
//! email filled in. Only the account list (metadata) is read: YouTube Music's web player
//! signs in with Google's own page and keeps its session in Lyra's web view, so no token
//! or credential is ever asked for.

use gtk::glib;
use serde_json::Value;
use std::time::Duration;

const BUS: &str = "io.github.design_nexus.nyx.Accounts.Preview";
const PATH: &str = "/io/github/design_nexus/nyx/Accounts";
const IFACE: &str = "io.github.design_nexus.nyx.Accounts1";

/// The Google account's email, or `None` without Online Accounts or a Google account.
pub fn google_email(done: impl FnOnce(Option<String>) + 'static) {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(list().ok().and_then(|text| pick(&text)));
    });
    glib::spawn_future_local(async move {
        done(rx.recv().await.ok().flatten());
    });
}

fn list() -> zbus::Result<String> {
    let conn = zbus::blocking::connection::Builder::session()?.method_timeout(Duration::from_secs(3)).build()?;
    let reply = conn.call_method(Some(BUS), PATH, Some(IFACE), "ListAccounts", &())?;
    reply.body().deserialize::<String>()
}

/// A connected Google account first, else any Google account (its email is the same).
fn pick(text: &str) -> Option<String> {
    let accounts: Vec<Value> = serde_json::from_str(text).ok()?;
    let google = |a: &&Value| a["provider"] == "google";
    let email = |a: &Value| a["identity"].as_str().filter(|e| plausible(e)).map(String::from);
    let connected = accounts.iter().filter(google).find(|a| a["status"] == "connected").and_then(email);
    connected.or_else(|| accounts.iter().filter(google).find_map(email))
}

fn plausible(email: &str) -> bool {
    email.len() <= 254
        && email.split('@').count() == 2
        && email.split('@').all(|p| !p.is_empty())
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_connected_google_account() {
        let text = r#"[
            {"provider":"microsoft_personal","identity":"a@outlook.com","status":"connected"},
            {"provider":"google","identity":"old@gmail.com","status":"needs_sign_in"},
            {"provider":"google","identity":"ken@gmail.com","status":"connected"}
        ]"#;
        assert_eq!(pick(text).as_deref(), Some("ken@gmail.com"));
    }

    #[test]
    fn falls_back_to_any_google_account() {
        let text = r#"[{"provider":"google","identity":"ken@gmail.com","status":"needs_sign_in"}]"#;
        assert_eq!(pick(text).as_deref(), Some("ken@gmail.com"));
    }

    #[test]
    fn refuses_what_isnt_an_email() {
        for bad in ["", "ken", "a@b@c", "ken @gmail.com", "@gmail.com", "ken@"] {
            let text = format!(r#"[{{"provider":"google","identity":"{bad}","status":"connected"}}]"#);
            assert_eq!(pick(&text), None, "{bad}");
        }
        assert_eq!(pick("not json"), None);
    }
}
