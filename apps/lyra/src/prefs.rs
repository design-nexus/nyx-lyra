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
    /// Closing the window while music plays hides it; the music keeps going.
    pub keep_playing: bool,
    /// The now-playing column beside the pages (direction A).
    pub show_column: bool,
    pub recent_searches: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Omarchy,
            theme: "tokyo-night".into(),
            reduce_motion: false,
            glow: true,
            sidebar_collapsed: false,
            keep_playing: true,
            show_column: true,
            recent_searches: Vec::new(),
        }
    }
}

nyx_core::prefs_store!(Prefs);
