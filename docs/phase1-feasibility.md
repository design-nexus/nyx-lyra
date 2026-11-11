# Phase 1: can WebKitGTK run YouTube Music?

**Yes, on the laptop.** The probe (`apps/probe`) signed in to Google, played, took commands,
kept playing hidden, survived a web-process crash, a restart and network stalls, and ran 30
minutes without growing. Chromium isn't needed. The NyxOS VM run is still to do; the open
design questions are at the end.

Tested 11 November 2026 on the laptop (Arch, Hyprland): WebKitGTK 6.0 2.54.1, GStreamer 1.28.8
(base, good, libav; no bad), the `webkit6 0.6.1` crate, a release build of the probe, and a
real Google account (Ken's). Tools: `tools/headless.sh` (start the probe on a headless output,
muted), `tools/drive.py` (send commands and check what the page reports), `tools/measure.py`
(processes, PSS, CPU), `tools/longrun.py` (one sample a minute).

## The probe

A GTK4 window holding a `webkit6::WebView` on `https://music.youtube.com/`:

- **Session:** a persistent `NetworkSession` with cookies in SQLite, in its own `0700` folder,
  apart from anything else Lyra stores.
- **Bridge** (`bridge.js`): a user script injected at document start, top frame,
  `music.youtube.com` only. It wraps `navigator.mediaSession.setActionHandler` to keep the
  page's own handlers (what media keys call) and reads `navigator.mediaSession.metadata` and
  the playing `<video>`/`<audio>` element. State goes to a `lyra` script message handler as
  JSON; commands come back through `evaluate_javascript` as a fixed set (play, pause, toggle,
  next, previous, seek, volume, state). No class names or page layout are read.
- **Trust:** the native side treats every message as untrusted: JSON objects of a known kind,
  at most 16 KB, else counted and dropped. The page can't run anything native; the only
  commands are the fixed set, with arguments checked (volume outside 0–100 is refused).
- **Popups and permissions** are refused and logged. A crashed web process reloads after 2 s,
  at most three times a minute.

## Results

| Test | Result |
| --- | --- |
| YouTube Music loads | Yes, with WebKit's own user agent: no "unsupported browser", no workaround. |
| Google sign-in | **Works** in the embedded view, Google's own page, no user-agent change (Ken, by hand). |
| Session after restart | Kept: a new process opened signed in (avatar and library shown). |
| Signed out | Browsing works; **playback doesn't**: YouTube shows "Please sign in. This helps protect our community." Lyra needs an account. |
| Audio | Plays on the system's default output through PipeWire (Ken heard it). |
| Play, pause, seek | Through the page's Media Session handlers; confirmed by the reported state in ~0.4 s. |
| Next, previous | Media Session handlers; the new song reported in 0.8–1 s. |
| Volume | On the media element (`volume`), confirmed. |
| Metadata | Title, artist, artwork URL, duration, position, playing/paused. **Album is empty** in the Media Session. |
| Window hidden | Keeps playing (position advanced 20 s in 20 s). |
| Continuous playback | Autoplay went through 9 songs in 30 minutes unattended. |
| Web process crash | Reported; the page reloaded in ~2 s and the bridge came back; playback stopped (paused). |
| Network stall, 60 s | Played through on the buffer. |
| Network stall, 80 s just after a song change | Stopped when the ~30 s buffer ran out, reported as buffering (`readyState` 2, not paused, no error), and resumed by itself when the network came back. |
| Fresh start, then play | Failed until the view's autoplay policy was set to Allow (WebKit's default is "without sound", which left YouTube Music on its "start playback" overlay). With Allow, play works 1.6 s after a cold start. |

## Resources (laptop, release build, muted, window hidden)

PSS counts each shared page once, across every process under the probe (WebKit's web and
network processes, its bubblewrap sandbox and D-Bus proxies). The songs were mostly music
videos, which decode video even while hidden.

| Moment | Processes | PSS | CPU (one core) |
| --- | --- | --- | --- |
| Window shown | | | 76–95 ms after launch |
| Home page loaded, signed in | | | 1.4–2.2 s after launch |
| Usable playback | | | 1.6 s after a play command on a fresh start |
| Idle on Home, signed in | 14 | 777 MB | 9.5% for 10 s after load, settling |
| Playing (fresh) | 14 | 764 MB | 15.5% |
| 30-minute run | 14 | 754 MB at start, 450–730 MB after, 476 MB at the end | 15.5% mean |

- Five processes do the work: the app (~90 MB), the YouTube Music web process (400–600 MB),
  a second web process (~80 MB, falling to ~20 MB), and the network process (~41 MB). The
  other nine are the sandbox (bwrap, xdg-dbus-proxy) at well under 1 MB each.
- No growth over 30 minutes. The later drop may be the system reclaiming memory rather than
  WebKit freeing it, so read it as "no leak", not as the steady size.
- **After sign-in and many test navigations** the same probe held 56 processes and 1.9 GB:
  WebKit kept a process for each site visited (Google's sign-in hosts) and for pages in the
  back/forward history. Lyra should turn off the back/forward cache and return to
  `music.youtube.com` in one view, then measure again.
- No comparison with Chromium was run (no failure called for it), so this says nothing about
  which engine is lighter.

## What the page offers Lyra

| Capability | Through | Status |
| --- | --- | --- |
| Play, pause, toggle, seek, next, previous | Media Session handlers | Works |
| Volume | Media element | Works; whether YouTube Music's own volume follows it across songs is untested |
| Title, artist, art, duration, position, state | Media Session metadata, media element | Works |
| Buffering, errors | Media element (`readyState`, `error`) | Works |
| Album | — | Not in the Media Session |
| Shuffle, repeat, like, queue, lyrics | — | Not in the Media Session; only in the page itself |
| Search, home, library, playlists, artists, albums | — | Not tried; see below |

## Decided (11 November 2026)

- **MPRIS:** Lyra publishes its own player (`org.mpris.MediaPlayer2.lyra`, `DesktopEntry` the
  app ID, local covers, volume). The shell's three MPRIS consumers (nyx-plugin-sdk's service,
  Control Center's `nyx-providers::media`, the dock) skip a `<id>.Sandboxed.instance-*` player
  when `<id>` has a player of its own.
- **Browsing:** native screens everywhere, as in the mockups (direction A), fed by the page's
  own data. Each capability is reported as available or not, and a screen whose data fails
  says so rather than guessing.
- **Laptop first:** Lyra Preview (this probe, `./install.sh lyra`) is on the laptop; no NyxOS
  package yet.

## Open questions for Phase 2 and 3

1. **WebKit publishes its own MPRIS player.** While media plays, the web process owns
   `org.mpris.MediaPlayer2.<app id>.Sandboxed.instance-N` with title, artist, length,
   position, next/previous/seek, an `https://` cover, `Identity` = the program name, an empty
   `DesktopEntry`, and no volume, shuffle, loop or raise. WebKit has no public setting to stop
   it (`MediaSessionGLib.cpp` registers whenever there's a title; the switch,
   `setDBusNotificationsEnabled`, is for its tests). The shell's consumers need a `file://`
   cover and `DesktopEntry` = the app ID, so this player can't be Lyra's. If Lyra publishes
   its own as well, the bar would offer two players for one app. Options: the shell skips a
   `<id>.Sandboxed.instance-*` player when `<id>` has its own player (one rule in
   nyx-plugin-sdk, nyx-providers and the dock); and ask WebKit for a setting. Media keys work
   with either.
2. **Library and search data.** YouTube Music has no public API. The page's own data comes
   from undocumented endpoints; reading it is fragile and not a contract. Phase 3 has to
   decide between native screens fed by that data (with every capability reported and
   failing cleanly) and showing YouTube Music's own page inside Lyra for the views that can't
   be done reliably. This decides how much of the chosen design (direction A's native grids)
   is real.
3. **Shuffle, repeat, like, queue, album, lyrics** aren't in the Media Session; each needs the
   same decision.
4. **Signed-out mode** can't play, so the "Use without signing in" path in the sign-in
   mockups should become browsing only, or go.
5. **Memory:** turn off the back/forward cache and check process reuse after sign-in; try the
   Premium "audio only" setting to see what dropping video saves.
6. **NyxOS:** package the probe (WebKitGTK 6.0 and GStreamer good, bad, libav from Nyx Mail's
   and Nyx Music's recipes) and repeat sign-in, playback and measurements in the VM.
