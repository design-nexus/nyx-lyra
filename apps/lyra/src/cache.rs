//! Pages as last read (`~/.cache/nyx-lyra/pages.db`, SQLite): a page opens at once from
//! here and is then refreshed from YouTube Music, which stays the source of truth. The
//! newest `KEEP` pages are kept.

use nyx_lyra_core::model::Page;
use rusqlite::{Connection, params};
use std::cell::RefCell;

const KEEP: i64 = 300;

thread_local! {
    static DB: RefCell<Option<Connection>> = const { RefCell::new(None) };
}

fn with<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    DB.with(|db| {
        let mut db = db.borrow_mut();
        if db.is_none() {
            let dir = nyx_core::paths::cache_dir();
            let _ = std::fs::create_dir_all(&dir);
            let c = Connection::open(dir.join("pages.db")).ok()?;
            c.execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=3000;
                 CREATE TABLE IF NOT EXISTS pages (key TEXT PRIMARY KEY, json TEXT NOT NULL, at INTEGER NOT NULL);",
            )
            .ok()?;
            *db = Some(c);
        }
        f(db.as_ref()?).map_err(|e| eprintln!("nyx-lyra: page cache: {e}")).ok()
    })
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub fn get(key: &str) -> Option<Page> {
    let json: String = with(|c| c.query_row("SELECT json FROM pages WHERE key = ?1", params![key], |r| r.get(0)))?;
    serde_json::from_str(&json).ok()
}

pub fn put(key: &str, page: &Page) {
    let Ok(json) = serde_json::to_string(page) else { return };
    with(|c| {
        c.execute("INSERT OR REPLACE INTO pages (key, json, at) VALUES (?1, ?2, ?3)", params![key, json, now()])?;
        c.execute("DELETE FROM pages WHERE key NOT IN (SELECT key FROM pages ORDER BY at DESC LIMIT ?1)", params![KEEP])
    });
}
