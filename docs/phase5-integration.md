# Phase 5: NyxOS integration

Lyra works with the rest of NyxOS through its actions (`nyx lyra …`, Glimpse, the
Assistant) and its own MPRIS player (media keys, the bar, Control Center, the dock).
Checked on the laptop, 11 November 2026.

## Actions

Lyra serves `nyx_core::control` (API 2) under its app ID; `--describe-actions` prints the
manifest the installer puts in `~/.local/share/nyx/actions/`, with a D-Bus service file so
any action starts Lyra in the background (`--gapplication-service`, no window; it quits
after 10 s with nothing to do).

| Action | `nyx lyra …` | What it does |
| --- | --- | --- |
| `lyra.play` | `play [words]`, `play --playlist ID` | Search and play the top song, artist or album, or a playlist from its first song; resumes with no words |
| `lyra.control` | `pause`, `resume`, `toggle`, `next`, `previous` | The page's own controls |
| `lyra.volume` | `volume 50` | 0–100 |
| `lyra.status` | `status` | Song, artist, album, position, volume, signed in or not |
| `lyra.search` | `search "Daft Punk"` | YouTube Music's results, Top result first |
| `lyra.playlists` | `playlists` | The library's playlists |
| `lyra.open` | `open [page]` | Shows the window at Home, Explore, Search, Library, Liked Music or Now playing |

The controls and volume answer only after the page confirms them (the state it reports
changes), or with why not; `play` answers once Lyra has the song loading. Requests made
while a page Lyra loaded is still loading wait for its bridge (up to 30 s), and one cut
off by a newer page is asked again, so a cold `nyx lyra search` answers once YouTube Music
is up. Playback needs the window: `play` opens it.

Commands that change something are taken only from Nyx programs (the `nyx` command,
Glimpse, the Assistant: their executable sits beside Lyra's or in `/etc/nyx/trusted-bin`);
others may read status and search.

Topics: `playback` (every state change) and `playlists`. Namespace `lyra`, aliases
`youtube-music` and `ytm`.

## MPRIS

Lyra publishes `org.mpris.MediaPlayer2.lyra` (Identity "Lyra", DesktopEntry the app ID):
metadata with the cover as a local file from Lyra's art cache, position and `Seeked`,
volume, PlayPause, Play, Pause, Stop (pauses), Next, Previous, SetPosition. No shuffle or
loop, as in the window. Media keys and `playerctl` work through it.

WebKitGTK publishes its own player for any page using Media Session
(`org.mpris.MediaPlayer2.<app ID>.Sandboxed.instance-N`), with no setting to turn it off.
Two measures keep it out of sight:

- The bridge hides the page's Media Session metadata from WebKit (Lyra reads it before
  WebKit sees it), so WebKit's player has no title and mostly doesn't register.
- When it does, the shell leaves it out: a `….Sandboxed.instance-N` player whose app also
  publishes a player of its own (the DesktopEntry matches) is skipped by nyx-core's
  `nyx-plugin-sdk` MPRIS service (the bar, once it takes that nyx-core) and Control
  Center's media card. The dock lists by window, not by player.

## Glimpse

`lyra` opens Lyra's panel (`panels/lyra`): what's playing with its cover, the controls and
volume, and the playlists, filtered by the words after `lyra`. Glimpse shows one panel per
query, so `music` still opens Nyx Music's.

## Assistant

The `nyx-apps` ability (1.7.0) has `lyra.play`, `lyra.control`, `lyra.volume`,
`lyra.status`, `lyra.search`, `lyra.playlists` and `lyra.open`, and answers "what's playing
in Lyra" without the model.

## Closing and the mini-player

Closing Lyra quits it and stops the music: its web process and its MPRIS player end, so
the bar and Control Center drop it at once. (Hiding the window instead, as Lyra first did,
let WebKit throttle the page: the song playing ran out and the next never loaded.)

The mini-player does hide the main window. The bridge keeps the page counting as seen
(`document.hidden` false), and YouTube Music plays on across songs with every control
confirmed; WebKitGTK 2.54 offers apps none of its own hidden-page throttling switches.

## Audio quality and the equalizer

Lyra keeps the account's Audio quality (YouTube Music's own setting, `account/set_setting`)
at the highest option YouTube Music offers it, unless another is picked in Settings →
Playback. With Premium that is High: the stream is AAC at 256 kbps (`141`), where Normal
played Opus at about 160 kbps (`251`).

Settings → Equalizer is Nyx Music's: a preamp and ten bands (31 Hz to 16 kHz, ±12 dB)
with its presets and saved presets. It runs in the page: the bridge routes YouTube Music's
media element through Web Audio (a gain, ten peaking filters), built the first time it's
turned on and sent again with each new page. Checked by its output level: flat about 0.1,
all bands +12 dB about 3, all −12 dB about 0.005, playback going on across a song change.

## Checked

- `nyx lyra status`, `search`, `playlists`, `pause`, `resume`, `toggle`, `next`,
  `previous`, `volume`, `play <words>` and `play --playlist` against a test copy; `status`,
  `search` and `playlists` against the installed Lyra, started cold by D-Bus.
- MPRIS properties and PlayPause with `busctl`; one WebKit player left out by the rule.
- Assistant: `nyx-apps` 1.7.0 loaded and running with the Lyra tools.

## Still to do

- Glimpse: `music` showing both Nyx Music and Lyra (needs Glimpse to offer more than one
  panel for a query).
- A `volume 100` once confirmed late (the page's volume reached 100 after the answer
  timed out).
