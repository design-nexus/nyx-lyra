//! Covers: fetched from YouTube's image hosts (public, no cookies), kept in
//! `~/.cache/nyx-lyra/art` (at most `MAX_BYTES`, oldest dropped) and in memory while shown.

use gtk::{gdk, gio, glib, prelude::*};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;

const MAX_BYTES: u64 = 200 * 1024 * 1024;
const MAX_IN_MEMORY: usize = 400;
const HOSTS: &[&str] = &[".googleusercontent.com", ".ytimg.com", ".ggpht.com"];

type Waiting = Vec<Box<dyn Fn(&gdk::Texture)>>;

thread_local! {
    static MEMORY: RefCell<HashMap<String, gdk::Texture>> = RefCell::default();
    static LOADING: RefCell<HashMap<String, Waiting>> = RefCell::default();
    static WRITES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn dir() -> PathBuf {
    nyx_core::paths::cache_dir().join("art")
}

fn allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?']).next().unwrap_or_default();
    HOSTS.iter().any(|h| host.ends_with(h))
}

/// The cached file for `url`, when it has been fetched (MPRIS and notifications want files).
#[allow(dead_code)] // MPRIS and notifications (Phase 5)
pub fn file(url: &str) -> Option<PathBuf> {
    let p = path(url);
    p.exists().then_some(p)
}

fn path(url: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    url.hash(&mut h);
    dir().join(format!("{:016x}", h.finish()))
}

/// Call `done` with the cover at `url` once it's here (at once when it's in memory).
pub fn load(url: &str, done: impl Fn(&gdk::Texture) + 'static) {
    if !allowed(url) {
        return;
    }
    if let Some(t) = MEMORY.with(|m| m.borrow().get(url).cloned()) {
        return done(&t);
    }
    let first = LOADING.with(|l| {
        let mut l = l.borrow_mut();
        let waiting = l.entry(url.to_string()).or_default();
        waiting.push(Box::new(done));
        waiting.len() == 1
    });
    if !first {
        return;
    }
    let url = url.to_string();
    let target = path(&url);
    let fetch = url.clone();
    let task = gio::spawn_blocking(move || -> Option<Vec<u8>> {
        if let Ok(bytes) = std::fs::read(&target) {
            // Touch it so the eviction keeps what's in use.
            let _ = std::fs::File::options().append(true).open(&target).and_then(|f| f.set_modified(std::time::SystemTime::now()));
            return Some(bytes);
        }
        let mut body = ureq::get(&fetch).call().ok()?.into_body();
        let bytes = body.with_config().limit(8 * 1024 * 1024).read_to_vec().ok()?;
        let _ = std::fs::create_dir_all(target.parent()?);
        let tmp = target.with_extension("part");
        std::fs::write(&tmp, &bytes).ok()?;
        std::fs::rename(&tmp, &target).ok()?;
        Some(bytes)
    });
    glib::spawn_future_local(async move {
        let bytes = task.await.ok().flatten();
        let waiting = LOADING.with(|l| l.borrow_mut().remove(&url)).unwrap_or_default();
        let Some(texture) = bytes.and_then(|b| gdk::Texture::from_bytes(&glib::Bytes::from_owned(b)).ok()) else { return };
        MEMORY.with(|m| {
            let mut m = m.borrow_mut();
            if m.len() >= MAX_IN_MEMORY {
                m.clear();
            }
            m.insert(url, texture.clone());
        });
        for w in waiting {
            w(&texture);
        }
        if WRITES.with(|w| {
            w.set(w.get() + 1);
            w.get() % 50 == 1
        }) {
            gio::spawn_blocking(evict);
        }
    });
}

/// Show the cover at `url` in a picture (or leave the placeholder).
pub fn into(art: &nyx_ui::media::Artwork, url: Option<&str>) {
    art.set_texture(None);
    let Some(url) = url else { return };
    let art = art.clone();
    let wanted = url.to_string();
    // A reused picture may have been given another cover since.
    art.root.set_widget_name(&wanted);
    load(url, move |t| {
        if art.root.widget_name() == wanted {
            art.set_texture(Some(t));
        }
    });
}

fn evict() {
    let Ok(entries) = std::fs::read_dir(dir()) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= MAX_BYTES {
        return;
    }
    files.sort();
    for (_, len, p) in files {
        if total <= MAX_BYTES * 9 / 10 {
            break;
        }
        if std::fs::remove_file(p).is_ok() {
            total -= len;
        }
    }
}
