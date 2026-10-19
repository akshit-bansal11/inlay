# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.1.0] - 2026-10-20

### Added

- A **preview for macOS and Linux** (`crates/desk`): the same four widgets,
  reading the same `config.toml` format. Spotify is read through AppleScript
  on macOS and MPRIS on Linux; the visualizer listens to the system's default
  input; a tray or menu-bar icon offers Open config file, Open fonts folder
  and Quit. Not yet tried on real machines beyond CI. Not in the preview: the
  settings panel, edit-layout dragging, hotkeys, start at login.
  Downloads are `Inlay-<version>-macos.zip` and
  `Inlay-<version>-linux-x86_64.tar.gz`.
- The Windows exe now carries version information.

### Changed

- Word-by-word timing is **off by default** (`word_sync = false`, a switch in
  settings). Turn it on to ask the community-run LyricsPlus servers; Inlay
  does not host or check them. New installs only: an existing `config.toml`
  keeps its setting; set `word_sync = false` to turn it off. With it off, the
  lyrics widget wakes at each new line instead of redrawing every frame, in
  complete mode; progress mode still fills the line smoothly.
- Lyrics servers must use `https://`; others are refused.
- Settings open on the first run.
- The README, SECURITY.md, CONTRIBUTING.md and the architecture notes are
  corrected and updated. Widgets are described as sitting on the desktop under
  your windows and drawing over the desktop icons, not behind them. The
  memory figure is described as measured once, method not recorded. The
  tested section no longer calls the settings panel, the colour picker, edit
  layout, word fill, the transport buttons, Win+D or the settings carry-over
  tested by automated tests.
- The repository is now https://github.com/akshit-bansal11/inlay and the
  website https://inlay.akshitbansal.dev. Files of 2.0.0 and earlier are
  attested under `akshit-bansal11/deskbeat`; 2.1.0 and later under
  `akshit-bansal11/inlay`.

### Fixed

- Programs the app starts are no longer named bare, so the system cannot
  resolve them from an unexpected place.
- Album art over 4096 pixels a side is refused.
- The audio-activation event is no longer closed while its handler may still signal it.
- Claims in the documentation that the app did not back up were corrected,
  including the architecture notes on which thread computes the spectrum.

### Security

- Start with Windows uses the registry API instead of running `reg.exe`.
- The repository link opens through the system's handler instead of
  `explorer.exe`.
- Notepad is started by its full path.

## [2.0.0] - 2026-10-17

Deskbeat is now **Inlay**. To upgrade, quit Deskbeat from its tray menu and
run `Inlay-2.0.0.exe`: your settings, fonts and start-with-Windows switch
come with it, and the old `Deskbeat-*.exe` can be deleted.

### Changed

- The website moved to https://inlay.akshitbansal.dev.
- **Deskbeat is now Inlay.** The exe is `inlay.exe`, released as
  `Inlay-<version>.exe`. The settings, fonts and lyrics cache move from
  `%APPDATA%\deskbeat` and `%LOCALAPPDATA%\deskbeat` (or the older
  `sonic-veil` folders) to `inlay` the first time it runs, and the
  start-with-Windows entry is renamed with them. The log is now `inlay.log`.
- A new mark: four flat pieces set into one tile, one for each widget, in a
  new palette of ink, bone, orange, cobalt and sun. The tray icon, the exe's
  icon, the tray menu, the settings panel, the README, the website and the
  social preview all use it. Its geometry lives in one place,
  `crates/core/src/mark.rs`.
- The README, the website and the other documents are rewritten to describe
  Inlay as a desktop widget app rather than a Spotify add-on.

## [1.4.0] - 2026-10-10

### Added

- `general.fit_screen`, on by default ("Scale widgets with the screen"):
  every widget grows or shrinks with the height of the screen, so a layout
  made on a big monitor keeps its proportions on a laptop. The screen it is
  measured against is the first one the app sees, kept as
  `general.layout_height`.

### Changed

- A widget that is dragged, or moved with its sliders, is measured from the
  edge, corner or middle of the screen it sits nearest, so it keeps that
  place on a screen of another size. A layout from an older version is
  measured this way once, at start, without anything moving.
- On a line of text that runs down or up, "Grows from its" picks its top,
  middle or bottom as well. "Middle" keeps a day name centred where it is
  however long it is: Wednesday no longer reaches lower than Tuesday.
  `right` on such a line now marks its bottom, where it marked its top.

### Fixed

- Widgets left off the edge, or bunched to one side, after the screen
  changed: unplugging a monitor, docking, or a new resolution or scale.
  They are now kept on the screen.
- The widgets' scale is read from the primary monitor, the one they are
  placed on, not from a hidden window that could still be on another one.

## [1.3.0] - 2026-10-04

### Added

- The exe has an icon: the mark, in Explorer, on the taskbar and in the
  settings window's title bar.
- `visualizer.bar_width` and `visualizer.bar_gap`, in pixels. The number of
  bars no longer changes their width: bars that do not fit are left out, the
  rest sit in the middle, and the whole range of pitch is spread over the
  ones shown. `visualizer.gap`, a share of the bar, is gone.
- `visualizer.rotation`: 0, 90, 180 or 270. At a quarter turn the bars run
  down the screen.
- `visualizer.show_idle`, on by default. Off, a bar with no sound in it is
  not drawn at all, where it was a dot.
- An Across slider for the visualizer. Moving it ends "Span the whole screen".

### Changed

- Word by word lights one word at a time: a word goes out again when it is
  over. It stayed lit until the end of its line. A word that lights up at
  once now fades in and out over a moment.
- The position sliders of the lyrics and the visualizer place their middle,
  so the middle notch is the middle of the screen, and keep them on the
  screen. They placed the top-left corner, so a frame as tall as the screen
  read 0 and could be pushed out of view.

### Fixed

- Dragging something in Edit layout did not move its sliders in the settings
  panel until the panel was touched.

## [1.2.0] - 2026-10-04

### Added

- Lyrics have two modes, `lyrics.mode`: what is being sung lights up at
  once (`complete`), or fills from its start to its end (`progress`). That
  is each word where the song has timed words and "Word by word where
  available" is on (`lyrics.word_sync`), and the whole line otherwise. Older
  files' `line` and `word` are read as `complete` and `progress`.
- Sliders for where each element of the clock and the player sits, and for
  the lyrics and the visualizer, in the display's own pixels, with a notch at
  every quarter of the screen that the knob jumps onto.
- `player.short_title`, on by default: the player shows the name of the song
  alone. The title is cut before a dash with a space either side, a bracket,
  `ft.` or `feat.`, and `from` or `with` followed by a quoted name. A hyphen
  inside a name stays, and so do `with` and `from` as words of the title.

### Changed

- The settings panel is reorganised: a sidebar of five pages, each split
  into its parts, with the rows of one part in cards. Every setting has one
  row. The clock's day no longer offers "Saturday, 3 October", which is the
  date's job, and the day and date no longer offer "Hidden" beside their Show
  switch. "Measured from" is gone from the panel.
- Lyrics are only followed word by word with times a server gave. A song
  without them is followed line by line; before, the times were worked out
  from the line's length.
- The song's title is never cut short with an ellipsis.
- A song with no lyrics, or an instrumental, shows nothing where the lyrics
  go. It showed "No lyrics for this track".

### Fixed

- Word timing went missing for many songs. The servers refuse a third
  request within ten seconds, and a refusal was dropped without a second
  try. Requests are now spaced out, a refusal is asked again, and a song
  that was skipped past is not asked about.

## [1.1.0] - 2026-10-04

### Added

- Any text of the clock and the player can run across, down or up, with its
  letters upright or lying sideways. So the day can be a column of upright
  letters, or the title can read up the side of the album art. In the settings
  panel these are Runs and Letters; in the file, `direction` and `letters`.
- A menu on a right-click of the tray icon: the name and version, a link to
  the repository, the start-with-Windows switch and Quit. The app draws it
  itself. Escape or a click anywhere else closes it.
- A website, with the documentation on one page.

### Changed

- A new mark: four sound bars, centred like a waveform, that draw the letter
  D. It is drawn in the three colours everything of the app's own now uses:
  ink, paper and one accent. The tray icon, the tray menu and the settings
  panel use the same three.
- The README shows a recording of the app instead of a rendered still.
- In Edit layout the space between the elements of the clock and the player
  no longer moves them all together. Every element moves alone; Shift+click
  several to move them as one.

### Fixed

- Dragging one element of the clock or the player made its other elements
  jump or shimmer from side to side.

## [1.0.1] - 2026-10-03

No change to the app itself. This release is the repository growing up.

### Changed

- The release file is named for its version, `Deskbeat-1.0.1.exe`, so several
  downloads on one PC can be told apart. It was `deskbeat.exe`.
- Releases are published by the workflow's own token with `gh`, and every
  action in every workflow is pinned to a commit. Rust comes from the runner
  image, not from an action.

### Added

- `scripts/check.ps1`, the one quality gate, which CI runs in its
  non-mutating mode.
- Draft builds: running the release workflow by hand attaches a build to a
  private draft release.
- CONTRIBUTING, SECURITY, an architecture document, issue and pull request
  templates, Dependabot for actions and crates, and a winget manifest.

## [1.0.0] - 2026-10-03

### Changed

- **Renamed from Sonic Veil to Deskbeat.** The exe is `deskbeat.exe` and its
  folders are `%APPDATA%\deskbeat` and `%LOCALAPPDATA%\deskbeat`. Settings,
  fonts, the lyrics cache and the start-with-Windows entry are carried over
  the first time the new exe runs.

### Added

- Lyrics timed word by word, from a LyricsPlus server, with LRCLIB as the
  fallback. Each word fills in as it is sung and stays lit afterwards.
  Before, word mode spread a line's time over its words by their length,
  which was a guess. `word_sync = false` under `[lyrics]` turns the extra
  lookup off, and `word_servers` lists the servers to ask.
- Lyrics: the line at the centre is drawn larger, lines fade the further
  they are from it, and the unsung words of the current line have their own
  opacity. All three are settings.
- A colour picker on every colour setting: a saturation and brightness
  square, a hue bar, and a hex field that takes typing.
- Shift+click in edit layout selects several elements, which then move
  together.
- Every text of the clock and the player has its own font, size, weight,
  colour, opacity, letter spacing and capitals setting. The lyrics take
  their own font.
- The progress bar has a thickness, colour and track opacity; the buttons a
  size and colour; the album art a corner rounding.
- "Hide all widgets", "Start with Windows" and "Quit" in the settings panel.
- The visualizer can span the whole width of the screen, which is now the
  default, with up to 512 bars.
- The visualizer can be flipped left to right and upside down.
- Every element of the clock and the player has its own position: the day,
  time and date, and the player's art, title, artist, three buttons,
  progress bar, elapsed time and total time. In edit mode each is dragged
  separately; empty space still moves the whole widget.
- The clock and the player have no box of their own: their window wraps
  around wherever their elements are, so an element can be dragged anywhere
  without being clipped. In edit mode they show only element outlines.
- The clock's day, time and date can each be hidden.
- "Art on the left" and "Centred stack" arrange the player in one click, as
  a starting point. Album art size and progress bar length are settings.
- Lyrics: separate colours for the current line, the current word and the
  other lines; an outline with its own colour and width; a shadow with its
  own colour and strength.
- Separate fonts for the clock's day row and its time and date rows.
- Fonts placed in `%APPDATA%\deskbeat\fonts` can be used without
  installing them in Windows.

### Removed

- The tray icon's right-click menu. A double-click opens the settings
  panel, which now holds everything the menu did.
- Frames and labels in edit mode. Only the element under the pointer is
  outlined.
- `clock.day_font`, `day_size`, `time_font`, `time_size`, `text_size` and
  `time_weight`, and `player.title_size` and `artist_size`, replaced by keys
  of each element such as `clock.day.size`. Files that use the old keys
  load and keep their values.
- `clock.align` from the config. A file that still has it loads, and the
  key is ignored.

### Fixed

- The settings panel flickered when scrolled past its top or bottom.

### Changed

- In word mode the rest of the current line is drawn in the current-line
  colour, and the word being sung in its own colour, which defaults to the
  accent.
- The clock's day and time default to Anurati and Quicksand, the pairing the
  Mond Rainmeter skin uses. Without those fonts the theme font is used.
- Widget width and height can be set up to the size of the screen.

## [0.1.0] - 2026-10-03

First build. A pre-release: see "Not yet verified" below before relying on it.

### Added

- Four desktop widgets for Spotify on Windows 11, in one native app: a
  spectrum visualizer, synced lyrics, a clock and a now-playing player.
- Track, position and transport controls from the Windows media session, so
  there is no Spotify login and no client ID.
- Visualizer audio captured from Spotify's process alone, with the whole
  system as an option. Bars, mirrored bars and wave styles.
- Lyrics from LRCLIB with a disk cache, highlighted line by line or word by
  word, scrolling between lines.
- A player card with album art, a blurred-art background, a progress bar that
  seeks on click, and previous, play/pause and next buttons.
- A settings panel, a tray menu, four one-click looks, and an edit mode that
  moves and resizes widgets by dragging.
- A hand-editable `config.toml` that reloads when saved.
- Global hotkeys: Ctrl+Alt+E edit layout, Ctrl+Alt+H hide, Ctrl+Alt+S
  settings, Ctrl+Alt+Up and Down to nudge lyric timing.
- Widgets stop drawing while a window covers their monitor.
- `--snapshot`, `--probe`, `--probe-system` and `--probe-lyrics` diagnostics.

### Not yet verified

- Nothing has been run against a live Spotify session: track details, the
  position Spotify reports, transport buttons and Spotify-only audio capture
  are untested on a real machine.
- Staying visible through Show Desktop (Win+D) is untested.
- The exe is unsigned, so Windows SmartScreen warns on first launch.

[Unreleased]: https://github.com/akshit-bansal11/inlay/compare/v2.1.0...HEAD
[2.1.0]: https://github.com/akshit-bansal11/inlay/releases/tag/v2.1.0
[2.0.0]: https://github.com/akshit-bansal11/inlay/releases/tag/v2.0.0
[1.4.0]: https://github.com/akshit-bansal11/inlay/releases/tag/v1.4.0
[1.3.0]: https://github.com/akshit-bansal11/inlay/releases/tag/v1.3.0
[1.2.0]: https://github.com/akshit-bansal11/inlay/releases/tag/v1.2.0
[1.1.0]: https://github.com/akshit-bansal11/inlay/releases/tag/v1.1.0
[1.0.1]: https://github.com/akshit-bansal11/inlay/releases/tag/v1.0.1
[1.0.0]: https://github.com/akshit-bansal11/inlay/releases/tag/v1.0.0
[0.1.0]: https://github.com/akshit-bansal11/inlay/releases/tag/v0.1.0
