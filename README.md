# Lyra

YouTube Music for NyxOS, in GTK4 and Nyx Design 2.0. Lyra plays through YouTube Music's own
web player in WebKitGTK 6.0 and is separate from Nyx Music (local music, radio, podcasts).

Not built yet. What's here:

- `apps/probe`: the Phase 1 feasibility probe, a WebKitGTK window on YouTube Music with a
  narrow playback bridge, driven by JSON lines on stdin. `./install.sh` installs it as
  **Lyra Preview** (`~/.local/bin/nyx-lyra-probe`, in the app launcher): one window on YouTube
  Music that keeps playing when closed, until the music stops. `./uninstall.sh` removes it.
- `tools/`: start the probe on a headless output (`headless.sh`), drive and check it
  (`drive.py`), measure it (`measure.py`, `longrun.py`).
- `docs/phase0-audit.md`: what Lyra reuses from the Nyx repositories.
- `docs/phase1-feasibility.md`: the results: sign-in, playback, commands, recovery, resources,
  and the open questions.

The design is in nyx-design's `mockups/lyra` (direction A, the art-and-tabs expanded player,
the strip mini-player).

## License

MIT
