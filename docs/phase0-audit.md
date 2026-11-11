# Phase 0: what Lyra reuses

An audit of the Nyx repositories before Lyra is built (11 November 2026). Lyra is a new app,
`io.github.design_nexus.nyx.Lyra`, separate from Nyx Music. Everything below already exists;
Lyra adds no daemon, IPC, parser, theme engine or media service.

## The app itself

Lyra copies nyx-music's shape:

- **Repository:** `apps/lyra` in a Cargo workspace (edition 2024, `resolver = "3"`, release
  `lto = "thin"`, `strip = true`), `crates/nyx-icons` with the app icon, and nyx-weather's
  `AGENTS.md`, `rustfmt.toml`, `build-environment.json`, `packaging.json`, `install.sh` and
  `uninstall.sh` (an `app_info` line `lyra) ids="Lyra" deps="webkitgtk-6.0 gst-plugins-good
  gst-plugins-bad gst-libav" actions="nyx-lyra"`).
- **Shared crates,** pinned by git revision: `nyx-core` and `nyx-actions` (nyx-core),
  `nyx-theme` and `nyx-ui` (nyx-design), and `gtk4 0.11`. `webkit6 0.6.1` (feature `v2_50`) is
  what Nyx Mail uses.
- **Startup:** `nyx_core::app::init`, `describe_if_asked`, an `NYX_LYRA_APP_ID` override,
  `--gapplication-service` for D-Bus activation, `nyx_theme::use_design_2()` then
  `nyx_theme::install`, `nyx_ui::chrome` (header, settings button), `nyx_ui::sidebar` (with the
  icon rail below 980 px), `nyx_ui::settings_card` (Playback, Account, Background, then the
  standard groups), `nyx_ui::keys`, `nyx_ui::toast`, `nyx_ui::menu`, and `nyx_ui::media`
  (`Artwork`, `PosterCard`, `hover_play`) for covers.
- **Preferences:** `nyx_core::prefs_store!` (`~/.config/nyx-lyra/settings.toml`); paths from
  `nyx_core::paths`.
- **SQLite:** `rusqlite 0.40` (`bundled`), WAL, `PRAGMA user_version` migrations, and row-count
  eviction as nyx-weather's cache does. For cached metadata, recent items and artwork
  references only; YouTube Music stays the source of truth.
- **Errors and logs:** `anyhow` and `eprintln!("nyx-lyra: …")`, as the GTK apps do. Never log
  cookies, tokens or account details.
- **Background:** Torrents' `keep_running` close handler (hide instead of quit) plus Music's
  `hold_while_playing`; the dock's relaunch reaches the running instance's `command_line`,
  which shows the window again.

## Account and secrets

- YouTube Music's sign-in lives in WebKit's own storage: a persistent `NetworkSession` (cookies
  and site data) in `~/.local/share/nyx-lyra/web`, separate from Lyra's settings and database.
  The folder is `0700`. WebKit doesn't encrypt it; the same is true of every browser profile.
- If Lyra ever holds a secret of its own, it goes through the Secret Service with nyx-core's
  `nyx-sync::credentials` helper (`secret-tool`; GNOME Keyring on NyxOS). Nothing needs it today.
- Nyx Accounts' Google sign-in is OAuth for Google APIs; it can't sign a web view into
  YouTube Music, so Lyra doesn't use it.

## Commands, Glimpse and the Assistant

All three read the same registry, `nyx-actions` (nyx-core, contract API 2):

- **Actions:** a JSON description in Lyra's `control.rs`, served with
  `nyx_core::control::serve_with` on `io.github.design_nexus.nyx.Control` and installed as
  `nyx/actions/io.github.design_nexus.nyx.Lyra.json` plus a D-Bus service file (`install.sh`
  on the laptop, `installActions` in nyx-packages on NyxOS). Following `music.*`:
  `lyra.control` (`play|pause|toggle|next|previous`, positional, word aliases), `lyra.volume`
  (`level` 0–100), `lyra.status` (read-only), `lyra.search` (read-only, positional query),
  `lyra.open`. `nyx lyra pause`, `nyx lyra volume 50` and `nyx lyra search "Daft Punk"` then
  work through `/usr/bin/nyx` with no parser in Lyra.
- **Live state:** a `playback` topic published with `service.publish` while watched.
- **Glimpse:** a `lyra` panel (`quickshell/Glimpse/panels/lyra/Panel.qml`) and `"lyra"` in
  `commands::PANELS` in nyx-glimpse. Glimpse refuses a panel's calls to another app's actions,
  so Lyra can't reuse the `music` panel as is; its own panel is the music panel with `lyra.*`
  calls and a source line. Typing `music` already lists every app with a panel first.
- **Assistant:** AI Core links the same registry, so `actions.run` reaches Lyra at once. Named
  tools ("pause music", "play …") need `lyra.*` added to the `nyx-apps` ability's
  `plugin.toml`, `TOOLS` and `APPS`.

## Media

- **MPRIS consumers** (the bar's media plugin through `nyx-plugin-sdk`, Control Center's
  `nyx-providers::media`, the dock) each list `org.mpris.MediaPlayer2.*` and pick the player
  that's playing; none assumes one. The dock matches a player to its app by `DesktopEntry`,
  and Control Center and the dock show `mpris:artUrl` only as a `file://` URI.
- **Media keys:** Hyprland binds them to `playerctl` (`nyxos/modules/home/hyprland.nix`). Lyra
  binds nothing.
- **MPRIS server:** Music and Media Player each have a hand-written `zbus 5` server
  (`apps/music/src/mpris.rs`). Lyra copies it: `org.mpris.MediaPlayer2.lyra`, `Identity`
  "Lyra", `DesktopEntry` the app ID, covers cached to disk. A shared crate for the three would
  be better but doesn't exist yet.
- **WebKit's own MPRIS** (see Phase 1): WebKitGTK publishes the page's media itself, as
  `org.mpris.MediaPlayer2.<app id>.Sandboxed.instance-N`, and has no setting to turn that off.
- **Notifications:** `org.freedesktop.Notifications` (Nyx's notifyd). Use `replaces_id` so
  song changes replace each other, `desktop-entry`, a local `image-path`, and `transient`.

## Packaging

- **Laptop:** webkitgtk-6.0 2.54.1, GStreamer 1.28.8 with base, good and libav are installed;
  `gst-plugins-bad` isn't (Lyra's `install.sh` adds it).
- **NyxOS:** nyx-packages already builds `webkitgtk_6_0` and `glib-networking` (Nyx Mail) and
  the GStreamer sets (Nyx Music). `pkgs/nyx-lyra.nix` is `nyx.buildApp` with
  `actions = [ "nyx-lyra" ]` and `buildInputs = [ webkitgtk_6_0 glib-networking ] ++ gst
  (base, good, bad, libav)`; then a flake input, the `fromSource` list, `lyra = "Nyx Lyra"` in
  `modules/nixos/default.nix`, `lib/git-hashes.json`, and `lyra.enable` on the dev host.
- **Before any VM rebuild:** nyxos pins nyx-packages `1232b43` while nyx-packages `main` is
  `2398a09` (11 November, 12:30); bring the pin up first.

## Not reused

- **nyx-browser** is a Zen (Firefox) build, not an engine Lyra can embed.
- **nyx-playback** drives mpv for video; Lyra's playback is the web player's.
- **Nyx Music** stays as it is; Lyra shares no code with it beyond the patterns above.
