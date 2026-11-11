# Lyra

YouTube Music for NyxOS, in GTK4 and Nyx Design 2.0. Lyra plays through YouTube Music's own
web player in WebKitGTK 6.0 and is separate from Nyx Music (local music, radio, podcasts).

In progress. The native window (nyx-design's `mockups/lyra`, direction A) has Home, Explore,
Search, the library, albums, artists and playlists, the now-playing column, the player bar,
Now playing with Up next, Lyrics and Related, and the strip mini-player, all from YouTube
Music's own data. Media keys, the bar's media controls, `nyx lyra …` and Glimpse come next.

## Install

```sh
./install.sh
```

Installs `nyx-lyra` and Lyra in the app launcher, with WebKitGTK 6.0 and the GStreamer
plugins it needs (good, bad, libav). `./uninstall.sh` removes it. Sign in to YouTube Music in
the window the first time; YouTube Music doesn't play anything signed out.

## What's here

- `apps/lyra`: the app. `runtime/` is the web runtime (WebKitGTK behind the `Runtime`
  trait), `player.rs` the playback service, `dev.rs` a JSON-lines channel for the checks
  (`NYX_LYRA_DEV=1`).
- `crates/nyx-lyra-core`: the bridge script and its protocol, playback state, typed commands
  and their confirmation, capabilities, and YouTube Music's requests and parsers. No GTK.
- `apps/probe`: the Phase 1 feasibility probe.
- `tools/`: `headless.sh` starts a test copy on a headless output (`LYRA=app` for Lyra),
  `check-app.py` checks Lyra end to end, `drive.py` the probe, `measure.py` and
  `longrun.py` measure them.
- `docs/`: [Phase 0 audit](docs/phase0-audit.md), [Phase 1 feasibility](docs/phase1-feasibility.md),
  [Phase 2 core](docs/phase2-core.md), [Phases 3 and 4](docs/phase3-4-window.md).

## License

MIT
