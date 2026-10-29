# Architecture

Inlay is one Windows process that sets four live widgets into the desktop: a clock, a spectrum visualizer, synced lyrics and a now-playing player. The last three follow whatever Spotify plays. It is not a skin engine. There are no plugins, no scripts and no user-authored skins; every visual and behavioural parameter of the four widgets is a setting instead.

The reason it exists is cost. A Rainmeter setup re-runs interpreted measures on timers and redraws whether or not anything changed. Here each widget is compiled, sleeps until something it shows has changed, and is not drawn at all while it is covered.

```
crates/core   pure logic, no Windows types, unit tested anywhere
  lrc         LRC and enhanced LRC parsing
  timing      line ends, word timing, the invariants the renderer relies on
  clock       the sync clock: where the track is right now
  lrclib      LRCLIB requests, title cleaning, choosing among results
  lyricsplus  word-timed lyrics: request and response parsing
  bands       FFT bins to bars, smoothing
  color       hex, HSV, the accent taken from album art
  timefmt     clock formats
  config      the TOML schema, defaults, clamps, migrations
  mark        the logo's geometry and colours, shared by the tray, the menu and the icon script

crates/app    the exe
  UI thread       message loop, windows, Direct2D drawing, settings panel
  media thread    Windows media session events for Spotify
  capture thread  WASAPI loopback; hands samples to the UI thread
  lyrics thread   one short-lived thread per track

crates/desk   the macOS and Linux preview (egui), see below
```

## The split

`crates/core` never names a Windows type. That is a rule, not a tendency: it is what lets every test in it run on any machine in milliseconds, and it keeps the parts that are easy to get subtly wrong, such as timing and config migration, away from the parts that can only be checked by looking.

`crates/app` is everything that needs a window, a device or Spotify.

## Where the data comes from

- **Track, position, art and transport** come from the Windows media session API (GSMTC), filtered to Spotify's session. There is no Spotify Web API, no client ID and no OAuth, so it works on free accounts and needs no setup.
- **Audio** comes from WASAPI process loopback on Spotify's process tree: the capture thread keeps the newest samples, and the visualizer widget runs the FFT and maps it to bands on the UI thread (`crates/app/src/widgets/visualizer.rs`). So the visualizer moves to Spotify and not to a game or a call. System loopback is a setting for when that is wanted.
- **Lyrics** come from two places. LRCLIB answers first, with the time each line starts. When word timing is on (off by default), a LyricsPlus server, if it has the track, then replaces that with the time each syllable is sung. Both answers are cached on disk; a miss is remembered for a week. A server that is down is not remembered as a miss.

## The loop

One thread owns every window. Each pass it drains the message queue, ticks each widget, and draws the ones that said they changed. A widget's tick returns what it wants next:

- `Idle`: nothing until an event arrives.
- `After(duration)`: wake me then. The clock asks for the next minute, the lyrics for the next line (with word timing on, every frame while a line is being sung), the player for the next second.
- `Frame`: something is moving. The loop waits on the compositor's vblank and comes back.

The most impatient answer wins. When every widget is idle the thread blocks on the message queue and uses no CPU. `Frame` is reserved for motion that is actually on screen: a scroll between lyric lines, a word filling in, the visualizer while there is sound. Returning it otherwise is the bug this project exists to not have.

Other threads never touch a window. They write to shared state and post a message; the UI thread reads the state when the message arrives.

## Drawing

Each widget is its own layered, click-through, no-activate tool window with a DirectComposition swap chain, drawn with Direct2D and DirectWrite through one shared device. Widgets sit at the bottom of the window stack, on the desktop under every app; they draw over the desktop icons rather than behind them. A widget under a maximized or fullscreen window is not drawn.

The clock and the player have no box. Each of their elements has its own position, and the window is wrapped around whatever was drawn. In edit mode that window covers the work area instead and does not move, so dragging one element cannot disturb the others; the space between elements passes clicks through. The lyrics and the visualizer keep a frame, because it is what sizes them.

## Keeping lyrics in time

The media session reports a position now and then, not continuously, and the reports jitter. The sync clock advances on its own between reports, ignores jitter inside a deadband, takes the median of recent errors so one outlier cannot move it, eases small corrections in over a few frames, and snaps on a seek or a resume.

Word timing from a server is used as it is: words can overlap, as with background vocals, and can leave gaps. When only line timing is known, a line's time is spread over its words by length, which is a guess and is marked as one; the lyrics widget never shows a guess, and draws such a line as a whole. The servers answer two requests in ten seconds, so requests to one server are spaced out across every fetch, and a refusal is asked again once.

## Settings and state

Everything is in `%APPDATA%\inlay\config.toml`, and the file reloads when it is saved. Loading lays the file over the default config table by table, so a partial file keeps every default it does not mention, and unknown keys are ignored. Keys that were renamed are carried over, as is the whole folder from the app's earlier names (`deskbeat`, then `sonic-veil`) on first run, and every number is clamped, so a hand-edited or older file cannot put the app in a state the settings panel could not.

The settings panel is an ordinary window drawn by the app itself, immediate-mode: each frame it lays its rows out from the config and writes changes straight back.

## What the tests cover

`crates/core` carries most of them: the LRC parser on real, half-broken files; word timing and its invariants; the sync clock against drift, jitter, seeks and pauses; title cleaning; config defaults, clamps and migrations from older files; band mapping; colour.

`crates/app` tests what can be tested without a window: sample mixing, the lyric widget's place-in-the-song logic, the settings panel's hex field, the tray icon's pixels.

`inlay.exe --snapshot <dir>` renders every widget and the settings panel to PNG from made-up data, with no GPU, Spotify or audio device. CI runs it on every push. It proves the drawing code runs and lets a change be looked at; it does not exercise the app loop, a pointer, or a live session.

## Build and release

There is one gate, `scripts/check.ps1`: format, clippy with warnings as errors, tests. CI runs it non-mutating on every push and pull request, then builds the exe and renders the snapshots.

Pushing a `v*` tag runs the release workflow: the gate again, a release build, a check that the tag matches the crate version, the snapshot smoke test, then a GitHub Release carrying `Inlay-<version>.exe`, `SHA256SUMS.txt` and a build provenance attestation, with notes taken from that version's section of `CHANGELOG.md`. Running the same workflow by hand attaches the build to a private draft release instead.

## crates/desk

The macOS and Linux preview. It reads the same `config.toml` and uses `inlay-core` for lyric parsing, timing, the sync clock, band mapping and config.

- **Windows.** One egui viewport (eframe) per widget.
- **Media thread.** Reads Spotify and sends the transport commands: AppleScript on macOS, MPRIS on Linux.
- **Audio.** cpal listens to the system's default input; the samples go through the FFT into bands, as on Windows.
- **Lyrics.** `ureq` requests, with the same cache format as the Windows app. Album art is downloaded from the https address Spotify gives.
- **Tray.** A tray or menu-bar icon with Open config file, Open fonts folder and Quit.

Not in the preview: the settings panel, edit-layout dragging, hotkeys, start at login. CI builds and checks it; it has not been tried on real machines beyond that.
