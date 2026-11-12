//! The equalizer, as Nyx Music's: a preamp and ten bands with Music's presets, each
//! lowering the preamp by about its biggest boost so loud songs don't clip. It runs in the
//! page (Web Audio; see the bridge), so Lyra sends it again on every new page.

use crate::player;
use crate::prefs::{self, EqPreset};
use nyx_lyra_core::command::Command;

pub fn builtin() -> Vec<(&'static str, EqPreset)> {
    let p = |name: &str, preamp: f64, bands: [f64; 10]| EqPreset { name: name.into(), preamp, bands };
    vec![
        ("flat", p("Flat", 0.0, [0.0; 10])),
        ("bass", p("Bass boost", -5.0, [6.0, 5.0, 4.0, 2.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0])),
        ("treble", p("Treble boost", -5.0, [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.5, 4.0, 5.0, 6.0])),
        ("vocal", p("Vocal", -3.0, [-2.0, -1.5, 0.0, 2.0, 3.5, 3.5, 2.5, 1.0, 0.0, -1.0])),
        ("rock", p("Rock", -4.0, [4.5, 3.5, 2.0, 0.0, -1.0, -1.0, 0.5, 2.0, 3.0, 4.0])),
        ("electronic", p("Electronic", -5.0, [5.0, 4.0, 1.5, 0.0, -2.0, 1.0, 0.0, 1.5, 4.0, 5.0])),
        ("loudness", p("Loudness", -4.0, [5.0, 3.5, 0.0, 0.0, -1.0, 0.0, -1.0, 0.0, 3.0, 4.0])),
    ]
}

/// Every preset as (id, preset): built-ins, then the user's own as `custom:<name>`.
pub fn all(custom: &[EqPreset]) -> Vec<(String, EqPreset)> {
    let mut v: Vec<(String, EqPreset)> = builtin().into_iter().map(|(id, p)| (id.to_string(), p)).collect();
    v.extend(custom.iter().map(|p| (format!("custom:{}", p.name), p.clone())));
    v
}

/// The preset these settings match exactly, if any.
pub fn matching(custom: &[EqPreset], preamp: f64, bands: &[f64; 10]) -> Option<String> {
    let close = |a: f64, b: f64| (a - b).abs() < 0.05;
    all(custom)
        .into_iter()
        .find(|(_, p)| close(p.preamp, preamp) && p.bands.iter().zip(bands).all(|(a, b)| close(*a, *b)))
        .map(|(id, _)| id)
}

/// Send the equalizer to the page (off too, once it was on: the page keeps it built).
pub fn apply() {
    let p = prefs::get();
    if let Some(pl) = player() {
        pl.send(Command::Eq { on: p.eq_enabled, preamp: p.eq_preamp, bands: p.eq_bands });
    }
}

/// "+3.0" or "0", as Music writes dB.
pub fn db(v: f64) -> String {
    if v.abs() < 0.05 { "0".into() } else { format!("{v:+.1}") }
}

/// "31", "1k", "16k".
pub fn hz(f: f64) -> String {
    if f >= 1000.0 { format!("{}k", (f / 1000.0).round() as u64) } else { format!("{}", f.round() as u64) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_never_clip_and_ids_are_unique() {
        let all = all(&[]);
        let mut ids: Vec<_> = all.iter().map(|(i, _)| i.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        for (_, p) in all {
            let max = p.bands.iter().cloned().fold(f64::MIN, f64::max);
            assert!(max + p.preamp <= 1.0, "{} clips", p.name);
        }
        assert_eq!(matching(&[], 0.0, &[0.0; 10]).as_deref(), Some("flat"));
        assert_eq!((hz(1000.0), hz(16000.0), hz(62.0)), ("1k".into(), "16k".into(), "62".into()));
    }
}
