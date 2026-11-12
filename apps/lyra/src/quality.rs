//! The account's audio quality, kept by YouTube Music (its Settings dialog's Audio
//! quality). Lyra keeps it at the highest option YouTube Music offers the account, or at
//! the one picked in Lyra's settings.

use crate::{player, prefs};
use nyx_lyra_core::youtube::{self, AudioQuality};

/// Read the setting, then `done` with it (`None` when YouTube Music doesn't say).
pub fn read(done: impl FnOnce(Option<AudioQuality>) + 'static) {
    let Some(p) = player() else { return done(None) };
    p.request(&youtube::settings(), Box::new(move |r| done(r.ok().and_then(|v| youtube::audio_quality(&v)))));
}

/// The option Lyra wants: the one picked in its settings if YouTube Music still offers
/// it, else the highest.
pub fn wanted(q: &AudioQuality) -> Option<String> {
    prefs::get().audio_quality.filter(|w| q.options.iter().any(|(_, v)| v == w)).or_else(|| q.highest().map(|(_, v)| v.clone()))
}

/// Once signed in: put the account on the option Lyra wants, if it isn't.
pub fn ensure() {
    read(|q| {
        let Some(q) = q else { return };
        if let Some(w) = wanted(&q)
            && q.selected.as_ref() != Some(&w)
        {
            set(&q.item, &w, |_| {});
        }
    });
}

pub fn set(item: &str, value: &str, done: impl FnOnce(bool) + 'static) {
    let (Some(req), Some(p)) = (youtube::set_audio_quality(item, value), player()) else { return done(false) };
    let value = value.to_string();
    p.request(
        &req,
        Box::new(move |r| {
            if r.is_err() {
                eprintln!("nyx-lyra: YouTube Music didn't take audio quality {value}");
            }
            done(r.is_ok())
        }),
    );
}
