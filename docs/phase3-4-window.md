# Phases 3 and 4: the data and the window

Lyra has its native window (direction A of nyx-design's `mockups/lyra`), fed by YouTube
Music's own data from the page that's playing. Checked on the laptop, 11 November 2026:
`tools/check-app.py` 19/19 with the window, the core's tests 17/17, and every saved real
reply parsed.

## Data (Phase 3)

| Screen | Request | Read |
| --- | --- | --- |
| Home | browse `FEmusic_home` | Shelves: Quick picks (songs), playlists, mixes |
| Explore | browse `FEmusic_explore` | New albums & singles, top songs, trending, new music videos |
| Search | search | Top result card, then every result (YouTube Music now lists one per section) |
| Library | browse `FEmusic_liked_playlists`, `_liked_albums`, `_library_corpus_track_artists`, `_liked_videos` | Playlists, albums, artists, songs; an empty tab reads as empty, not as an error |
| Liked Music | browse `VLLM` | Header and songs |
| Album, artist, playlist | browse by ID | Header (cover, kind and year, counts, description) and shelves; an artist's top songs, albums, singles, videos, playlists, similar artists |
| Long lists | browse with the `continuation` token | 100 at a time; checked on a 143-song playlist |
| Up next | next with the song's list, or its radio (`RDAMVM…`) as YouTube Music plays it | The queue YouTube Music will play |
| Lyrics, Related | browse by the IDs in Up next | Lyrics with their source (Musixmatch, LyricFind); related shelves |

Lyra keeps no queue of its own: Up next is fetched again whenever the song changes. Action
rows ("Shuffle all", "New playlist") are left out, since Lyra has its own.

**Caches:** pages in `~/.cache/nyx-lyra/pages.db` (SQLite, newest 300): a page opens from
here at once and is replaced by YouTube Music's answer. Covers in `~/.cache/nyx-lyra/art`
(at most 200 MB, oldest first), fetched only from YouTube's image hosts.

## The window (Phase 4)

- **Header:** Lyra and the page, back and forward, the search field, the mini-player,
  Settings.
- **Sidebar** (`nyx_ui::sidebar`, the icon rail below 980 px): Home, Explore, Search; Library,
  Liked Music, Artists, Albums, Songs; the playlists from YouTube Music.
- **Now-playing column** (300 px, wide windows only): the cover, the song, Up next.
- **Pages:** headers for albums, playlists and artists; tile grids; song grids for Home's
  songs; numbered track lists; the search's top result; Load more; empty and error states
  with Try again.
- **Player bar:** the song, previous/play/next, the seek bar (a seek is sent on release),
  lyrics, Up next, volume, the mini-player, Now playing. Every control is a command to the
  page; the bar shows what the page reports, with the position counted forward between
  reports.
- **Now playing** (the art-and-tabs player): the cover large, the song, seek and controls;
  Up next, Lyrics, Related.
- **Mini-player** (the strip): 380 × 96, cover, song, three controls, Open Lyra, a progress
  line; opening it hides the main window, and the music keeps playing.
- **Sign in:** while the page reports no signed-in session, the window shows the sign-in
  card with Google's own page (where YouTube Music's Sign in button leads) and goes back to
  the pages when it's done.
- **Settings** (the shared card): Playback (keep playing when the window closes, the
  column), then Appearance, Keyboard and About.
- **Keys:** Space, Ctrl+←/→, Ctrl+F, Alt+←/→, F, Ctrl+M, Ctrl+B, Esc, Ctrl+, (the list in
  Keyboard).

Shuffle, repeat and like aren't shown: YouTube Music's Media Session doesn't offer them,
and Lyra shows no control it can't make work.

## Where the page lives

YouTube Music's page sits full size beneath the native window, which covers it. A small or
fully transparent page doesn't play: at 256 × 144 or 1 × 1, YouTube's player never leaves
buffering. Full size beneath plays as before (audible 5–8 s after the first play on a fresh
page, as in Phase 2). It costs CPU: 20% of one core while playing a music video, against
14% when the page was the whole window. To try later: YouTube Music Premium's "don't play
music videos", and keeping the page's own animations idle.

## Testing without touching the screen

- `tools/run-windowless.sh` runs Lyra without showing any window (data requests only;
  playback needs a shown window).
- `tools/headless.sh` builds the window hidden, waits for the page, sets the headless
  output's position and the window rule and checks them, and only then shows the window,
  watching it every 20 ms. A config reload had once moved the output and cleared the rule
  between the check and the window appearing, and the window showed on the laptop's screen
  for a moment before it was killed.
- The dev channel (`NYX_LYRA_DEV=1`) navigates (`nav`), opens Now playing (`now`) and the
  mini-player (`mini`) for screenshots.
