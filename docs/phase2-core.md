# Phase 2: the core

Lyra's core runs end to end on the laptop: the app (`apps/lyra`) plays YouTube Music through
the WebKit runtime, every command is confirmed by what the page reports, YouTube Music's data
reads into Lyra's models from the page that's playing, and a crashed web process comes back
to the same song, playing, at the same place. `tools/check-app.py` checks all of it: 19 of 19
on two fresh runs (11 November 2026).

## Layout

| Part | Where | What |
| --- | --- | --- |
| Bridge script | `crates/nyx-lyra-core/bridge.js` | Injected at document start, top frame, `music.youtube.com` only. Reads the Media Session and the media element; runs a fixed set of playback commands; makes the web client's own `search`, `browse` and `next` requests. |
| Bridge protocol | `nyx-lyra-core::bridge` | Page messages are untrusted: JSON of a known kind, at most 16 KB, strings cut to 512 characters, numbers checked, IDs and artwork URLs validated. |
| Playback | `nyx-lyra-core::playback` | The normalized state (status, track, position, volume, controls, connection, error), built only from page reports. |
| Commands | `nyx-lyra-core::command` | Typed commands; the coordinator confirms one only when the reported state shows it, fails it on a refusal, a timeout or a lost page, and refuses unsupported ones before sending. |
| Capabilities | `nyx-lyra-core::capability` | Each capability is available, unofficial (works through YouTube Music's own requests), not yet built, or unavailable with the reason. |
| YouTube Music | `nyx-lyra-core::youtube` | The requests (checked IDs and queries; never streams) and the parsers for search, Home, the library, playlists, albums, artists, Up next and lyrics. |
| Runtime | `apps/lyra/src/runtime` | The `Runtime` trait (send, request, load, mute, widget) and its WebKitGTK implementation. Nothing else in Lyra touches WebKit. |
| Player | `apps/lyra/src/player.rs` | The coordinator fed by the runtime: timeouts, listeners, recovery and retries. |
| Dev channel | `apps/lyra/src/dev.rs` | JSON lines on stdin/stdout with `NYX_LYRA_DEV=1`, for the checks. Off otherwise. |

## The runtime

- One persistent session in `~/.local/share/nyx-lyra/web` (`0700`), cache in
  `~/.cache/nyx-lyra/web`.
- Autoplay allowed (Lyra plays only when asked), Encrypted Media on (without it YouTube picks
  streams that fail), the page cache off (old pages kept a web process each).
- Only YouTube Music and Google's sign-in hosts load in the view; other links open in the
  browser. Popups and permission requests are refused.
- A web process that ends is reported; after three in a minute Lyra stops reloading
  (`Connection::Lost`) until asked.

## Recovery and retries

- **Web process ended:** the player marks the connection Reconnecting (waiting commands fail
  as Disconnected), reloads the last song from its start, plays it if it was playing, and
  seeks back once it plays. Measured: back and playing 8.8–9.8 s after the crash.
- **Page didn't load (offline):** Reconnecting, then reloads after 5, 15 and 30 s.
- **First song of a new page:** see below.

## The first-song problem (WebKitGTK 2.54)

The first song a new web process plays usually fails a few times with MediaError 4 before it
plays. The stream types are the same each time (`audio/webm; codecs="opus"` and VP9 video,
both reported as supported); GStreamer's log shows the audio branch rejecting an event into
`streamsynchronizer` while the pipeline is being set up ("could not send sticky events",
then "Failed to push buffer"). YouTube Music retries a few times quickly and gives up; a
play a few seconds later works, and later songs in the same page change cleanly. Starting
mid-song (`&t=`) fails every time.

Lyra re-sends Play after 0.5, 1.5, 3 and 5 s while it means to play, and recovers from
the start and seeks afterwards. The first song is audible 5–6 s after Play on a fresh page;
later songs in about a second. This belongs upstream (a WebKitGTK bug report with the log).

Also found: GStreamer had no AV1 decoder on the laptop until `gst-plugins-bad` was installed
(now a dependency in `install.sh`, as in the NyxOS recipe).

## Measured (laptop, release, muted, window hidden)

| | Value |
| --- | --- |
| Commands (play, pause, toggle, seek, volume) | confirmed in 0.2–0.4 s |
| Next, previous | 0.4–1.2 s |
| Data (search, Home, library, liked, Up next) | 0.15–1.8 s each, while playing, without interrupting it |
| First song on a fresh page | audible after 5–6 s |
| Playing | 20 processes, 767 MB PSS, 13.7% of one core (a music video) |

## Next

- Phase 3: the data layer for every screen: continuations for long lists, album and artist
  pages, lyrics, the live queue, like and shuffle/repeat state, a SQLite cache for metadata
  and artwork.
- Phase 4: the native window in direction A.
- Phase 5: MPRIS (plus the shell rule for WebKit's own player), `nyx lyra …` actions, the
  Glimpse panel, Assistant tools.
