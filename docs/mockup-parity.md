# Lyra against the mockups

Each screen of nyx-design's `mockups/lyra` (direction A, the art-and-tabs player, the
strip mini-player) beside Lyra on a real account, 11 November 2026, with
`tools/compare.sh SCREEN` (the mockup's shot on the left, Lyra on the right, 1280 × 800).
Checked in Nyx Dark, and Home, Library and Now playing in Nyx Light. The mockups' covers,
counts and names are samples; Lyra shows YouTube Music's.

## Matches

| Screen | What's there |
| --- | --- |
| Header | Lyra, the page, back and forward; the search field (the theme's `bar-search`, 340 px, "Ctrl F"); mini-player and Settings with the mockup's glyphs; the collapse button in Now playing, where the search is hidden |
| Sidebar | Home, Explore, Search; Your library (folds away) with Library, Liked Music (count), Artists, Albums, Songs, History; Playlists with "+", All playlists (count) and every playlist (only the playlists scroll; the rest stays put); the channel at the foot (photo, name, YouTube Music), which lists the channels to switch to and signs out; signed out, "Sign in · To see your library" |
| Home | The moods (All first, wrapping); shelves of square tiles 5 across at 1280 px, the first two rows deep, with More and arrows; tile hover play and "…"; song shelves in three columns; the featured card when YouTube Music's Home has a card shelf |
| Explore | New releases, Charts, Moods & genres as cards; New albums & singles one row; Top songs ranked with how they moved, two columns, "See all"; Moods & genres as coloured tiles |
| Search | The query in the field; All and the filters; the top result with Shuffle and Radio beside its songs; the results a section per kind (Albums, Artists…) with More opening that filter |
| Artist | The banner (photo faded behind, round photo), name, subscribers; Shuffle, Radio, Subscribe/Subscribed, share; top songs two columns with album and plays, "Show all"; albums, singles and the rest one row each |
| Album | Cover, "Album · year", title, the artist with photo, songs and length; Play, Shuffle, Save to library, share; "# TITLE 🕒" and one line per song, the one playing marked |
| Playlist, Liked Music | The same header with the owner; "# TITLE ARTIST ALBUM 🕒" with covers and hearts for liked songs; Liked Music's heart cover |
| Library | Playlists, Albums, Songs, Artists; YouTube Music's sort orders; tiles or list; New playlist and Liked Music first |
| Now playing column | Cover, song with heart and "…", "From the album · N songs" (numbered, the playing song marked) or "Up next · Then autoplay"; nothing playing, a placeholder tile |
| Player bar | Cover, song, heart; shuffle, previous, play, next, repeat (pressed when on); seek with times; lyrics, Up next, volume, expand; a pressed control stays pressed and play spins until YouTube Music confirms; Offline and "Reconnecting…" when the page is gone; Sign in signed out; nothing playing dimmed |
| Now playing | Cover, title with heart and "…", artist and album, seek, the five controls; Up next with "Playing from…", Save queue and Autoplay; Lyrics; Related; it can't open with nothing playing |
| Mini-player | Cover, song, artist and album, three controls, heart, Open Lyra, the progress line |
| Settings | Playback (lyrics when available, the column, notifications, audio quality, output), Account, then Appearance, Keyboard and About; the card keeps one size, its page scrolls |
| Equalizer | Added at Ken's request, as Nyx Music's: the header's Equalizer button opens it as its own card (on or off, presets, a preamp and ten bands) |

## Left out, and why

- **Reordering the queue** (the grips): Lyra keeps no queue of its own; the queue is YouTube
  Music's, and its drag isn't something the page offers a command for.
- **Editing a playlist** (the pencil): Lyra has no playlist editor yet.
- **Counts for Artists and Albums**: YouTube Music gives no totals for them, only pages.
- **"Lyra / Page" and the Settings card's coloured group tiles**: the shared Nyx header and
  Settings card (nyx-ui) draw these for every app; changing them is a suite decision.
- **The sign-in card**: replaced, at Ken's choice, by Google's page filling the window
  under Lyra's strip.
- **Settings: Resume on start** (YouTube Music always brings back the last song, paused),
  **Mini-player stays on top** (a Wayland app can't keep itself on top; a Hyprland rule
  can).
- **Background** ("Keeps playing when closed"): at Ken's choice, closing Lyra quits and
  stops the music.
- **Search's album lines** read "Daft Punk" where the mockup has "Album · 2013": Lyra
  shows the artist under album tiles everywhere, as Home's mockup does.
