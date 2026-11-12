//! Lyra's own preferences (`~/.config/nyx-lyra/settings.toml`).

pub use nyx_theme::ThemeMode;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub mode: ThemeMode,
    pub theme: String,
    pub reduce_motion: bool,
    pub glow: bool,
    /// The sidebar shows only icons, whatever the window width.
    pub sidebar_collapsed: bool,
    /// The now-playing column beside the pages (direction A).
    pub show_column: bool,
    pub recent_searches: Vec<String>,
    /// The library as a list instead of tiles.
    pub library_list: bool,
    /// Now playing opens on Lyrics when YouTube Music has them.
    pub show_lyrics: bool,
    /// A notification when the song changes while Lyra isn't in front.
    pub notify: bool,
    /// YouTube Music's audio quality option picked in Settings; none keeps the highest.
    pub audio_quality: Option<String>,
    /// The equalizer (Settings → Equalizer), as Nyx Music's.
    pub eq_enabled: bool,
    pub eq_preamp: f64,
    pub eq_bands: [f64; 10],
    pub eq_custom: Vec<EqPreset>,
}

/// A saved equalizer shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EqPreset {
    pub name: String,
    pub preamp: f64,
    pub bands: [f64; 10],
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Omarchy,
            theme: "tokyo-night".into(),
            reduce_motion: false,
            glow: true,
            sidebar_collapsed: false,
            show_column: true,
            recent_searches: Vec::new(),
            library_list: false,
            show_lyrics: true,
            notify: false,
            audio_quality: None,
            eq_enabled: false,
            eq_preamp: 0.0,
            eq_bands: [0.0; 10],
            eq_custom: Vec::new(),
        }
    }
}

nyx_core::prefs_store!(Prefs);
