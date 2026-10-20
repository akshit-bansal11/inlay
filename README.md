<p align="center"><img src="assets/inlay.svg" width="96" alt="Inlay"></p>

<h1 align="center">Inlay</h1>

<p align="center"><b>Live widgets, set into your Windows desktop.</b></p>

<p align="center">A clock, a visualizer, synced lyrics and a player, set into your desktop by one small native app. A light alternative to Rainmeter.</p>

<p align="center">Needs Windows 11 and the Spotify desktop app on the same PC. A preview for macOS and Linux is below.</p>

<p align="center">
  <a href="https://github.com/akshit-bansal11/inlay/releases/latest"><b>Download</b></a> ·
  <a href="https://inlay.akshitbansal.dev">Website</a> ·
  <a href="https://inlay.akshitbansal.dev/docs">Documentation</a>
</p>

<p align="center"><img src="assets/demo.gif" width="880" alt="A desktop with Inlay's widgets: a spectrum along the top edge, lyrics on the left lighting up line by line, and the day, a progress bar, the track title and playback buttons at the bottom"></p>

[![CI](https://github.com/akshit-bansal11/inlay/actions/workflows/ci.yml/badge.svg)](https://github.com/akshit-bansal11/inlay/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/akshit-bansal11/inlay)](https://github.com/akshit-bansal11/inlay/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> **Inlay was called Deskbeat**, and before that Sonic Veil. Your settings, fonts and start-with-Windows switch carry over the first time `Inlay-<version>.exe` runs.


## What it is

Inlay sets four live widgets into the desktop. They sit on the desktop under your windows, and they draw over the desktop icons. They aren't windows you manage. They're part of the desktop.

| Widget | What it shows |
| --- | --- |
| **Clock** | Day, time and date, each placed and styled on its own. Works with nothing else running. |
| **Visualizer** | A spectrum of Spotify's audio alone, not your games or calls. Bars, mirrored bars or a wave. |
| **Lyrics** | Time-synced lyrics, line by line from LRCLIB. With word timing turned on, they light up or fill word by word where a server has timed words. |
| **Player** | Album art, title, artist, a progress bar you can click to seek, and previous, play/pause and next. |

The visualizer, lyrics and player follow whatever Spotify plays. There's no Spotify login and no client ID: the track and the controls come from Windows itself, so a free account works.

## Why it stays light

A widget is redrawn only when something on it changes. Nothing is drawn while it sits still.

- **One native process.** Rust and Direct2D, with no webview, no runtime and no skin interpreter.
- **Event-driven.** The clock wakes once a minute and the player once a second. In complete mode the lyrics widget wakes at each new line; in progress mode, or with word timing on, it redraws every frame while a line is being sung. The visualizer runs only while there's sound.
- **Nothing hidden is drawn.** A widget under a maximized or fullscreen window stops entirely.
- **The low-power GPU.** On a laptop with two GPUs, it uses the integrated one.

Memory use was measured once at about 54 MB. How it was measured wasn't recorded, so treat it as a rough figure.

## Install

1. Download `Inlay-<version>.exe` from [Releases](https://github.com/akshit-bansal11/inlay/releases/latest). There is no installer.
2. Run it. The exe is unsigned, so the first time SmartScreen warns: choose **More info**, then **Run anyway**.

It needs Windows 11. The music widgets need the Spotify desktop app playing on the same PC; playback sent to another device through Spotify Connect isn't seen.

## macOS and Linux (preview)

New in 2.1.0, and not yet tried on real machines beyond CI. It draws the same four widgets from the same `config.toml` format.

**What works**

- The clock, visualizer, lyrics and player.
- A tray or menu-bar icon with Open config file, Open fonts folder and Quit. On GNOME the tray needs the AppIndicator extension.

**What is missing**

- The settings panel: edit `config.toml` instead. It reloads within 2 seconds.
- Edit-layout dragging, hotkeys and start at login.
- Vertical or sideways text, letter spacing, and per-label font weights beyond the face found.
- Wayland doesn't let apps place their windows, so positions may be ignored there. On GNOME, widgets can't stay below other windows.
- On Linux each widget may show in the taskbar and Alt-Tab; there is no way to hide them there. On macOS they may show in Mission Control.

**Install**

- macOS: download `Inlay-<version>-macos.zip` and unzip it to get `Inlay.app`. It's ad-hoc signed, not notarized, so macOS refuses it the first time: open System Settings, Privacy & Security, then **Open Anyway**. macOS also asks once whether Inlay may control Spotify.
- Linux: download `Inlay-<version>-linux-x86_64.tar.gz`, which holds the `inlay` binary. It needs ALSA and D-Bus, present on desktop distributions.

**Where things are**

| | macOS | Linux |
| --- | --- | --- |
| Config | `~/Library/Application Support/inlay/config.toml` | `~/.config/inlay/config.toml` |
| Fonts | a `fonts` folder beside the config | a `fonts` folder beside the config |
| Lyrics cache | `~/Library/Caches/inlay` | `~/.cache/inlay` |

**Spotify and sound**

- Spotify is read through AppleScript on macOS and MPRIS on Linux. On some versions Spotify's MPRIS position stays at 0, which makes lyrics lag.
- The visualizer listens to the system's **default input**, and only while Spotify is playing; there is no per-app capture. Out of the box that input may be the microphone, and macOS asks for microphone access the first time. Point it at a loopback or monitor device instead. Set the default input to a monitor of your speakers (PulseAudio or PipeWire "Monitor of ...") on Linux, or to a loopback device such as BlackHole on macOS.
- Album art is downloaded from the https address Spotify gives. Windows gets it from the system instead.

## Use

Inlay lives in the notification area. **Double-click** its icon for settings. **Right-click** it for a small menu: the version, a link to this repository, the start-with-Windows switch and Quit.

| Hotkey | Does |
| --- | --- |
| `Ctrl+Alt+S` | Open settings |
| `Ctrl+Alt+E` | Edit layout: drag any element of the clock or player on its own, or drag the lyrics or visualizer |
| `Ctrl+Alt+H` | Hide or show every widget |
| `Ctrl+Alt+Up` / `Down` | Show lyrics 100 ms earlier / later |

Hotkeys can be turned off in settings.

### Edit layout

The clock and the player have no box. Each of their elements (the day, time and date, and the player's art, title, artist, each button, the bar and both times) can be dragged anywhere on its own. The one under the pointer is outlined, and nothing else is drawn. Shift+click selects several, which then move together. The Player tab has two ready-made arrangements to start from.

### Settings

The settings panel has a page for each widget, split into its parts. Every part has its own position, in the display's own pixels, on sliders notched at every quarter of the screen.

- **Text:** every piece of text has its own font, size, weight, colour, opacity, letter spacing and capitals. It can run across, down or up, with its letters upright or lying sideways, so the day can stand as a column or turn on its side like the spine of a book.
- **Visualizer:** bar width and gap stay fixed whatever the number of bars; bars that don't fit are left out. It can turn a quarter to run down the side of the screen.
- **Player:** the bar's length, thickness and colour, the buttons' size and colour, and the art's size and corner rounding. Titles show the song's name alone, without the version, credits or film Spotify appends, and are never cut short.
- **Colour:** every colour setting has a picker, with a colour square, a hue bar and a hex field.
- **Layer:** widgets can sit on the desktop under your windows (the default), behave like a normal window, or stay on top of everything.

### The config file

Everything is stored in one file. You can edit it by hand, and it reloads when you save:

```
%APPDATA%\inlay\config.toml
```

The file accepts two things the panel doesn't: any installed font, and a hand-written clock format (`%H:%M`, `%l:%M %p`, `%A, %e %B` and so on).

To use a font without installing it, put its `.ttf` or `.otf` file in `%APPDATA%\inlay\fonts` and restart Inlay. The clock's day and time default to Anurati and Quicksand, the pairing of the Mond Rainmeter skin. They aren't shipped with the app, so the theme font is used until they're in that folder.

## Uninstall

On Windows: turn off start-with-Windows in the tray menu, choose Quit from the tray, and delete the exe. To remove everything it wrote, also delete `%APPDATA%\inlay` (settings, fonts) and `%LOCALAPPDATA%\inlay` (log, lyrics cache).

On macOS and Linux: quit from the tray or menu bar, delete the app or the binary, and delete the config and cache folders listed above.

## Troubleshooting

Inlay has no console, so its diagnostics write to a file. In a terminal, with Spotify playing:

```
Inlay-<version>.exe --probe > probe.txt
```

That records, for eight seconds, what Windows reports about the track and how loud Spotify's audio is. Two more:

- `--probe-system` listens to everything the PC plays, not Spotify alone.
- `--probe-lyrics "Title" "Artist"` looks one track up, for word timing and on LRCLIB.

Errors go to `%LOCALAPPDATA%\inlay\inlay.log`. The lyrics cache is in the same folder and is safe to delete.

Word timing is off by default for new installs; an existing config keeps its `word_sync` value (`word_sync = false` turns it off). When you turn it on (settings, or `word_sync = true` in the config), it depends on community-run LyricsPlus servers; when none answers, lyrics fall back to line timing from LRCLIB.

## What goes over the network

Only lyrics lookups, and on macOS and Linux album art. For each new track, its title, artist, album and length go over HTTPS to [LRCLIB](https://lrclib.net) whenever the lyrics widget is on. Only when word timing is on (off by default for new installs; an existing config keeps its `word_sync` value, and `word_sync = false` turns it off) do they also go to the community-run LyricsPlus servers listed in the config, which serve word-timed lyrics mirrored from other services; Inlay does not host or check them, so turn it on at your own discretion. Only `https://` lyric servers are accepted. Nothing else is sent: no identifier, no listening history, no audio. On macOS and Linux the album art is downloaded from the https address Spotify gives. Turning the lyrics widget off stops the lyrics requests. [SECURITY.md](SECURITY.md) has the full model.

## What has and hasn't been tested

On a real Windows 11 desktop:

- All four widgets draw, sit on the desktop layer, and cost no CPU while idle.
- Lyrics lookup, title cleaning and the disk cache against LRCLIB, and word timing from a LyricsPlus server.
- Audio capture, for one app and for the whole system.
- Reading the track, position and album art from a running Spotify.
- An older config file loading with its sizes and fonts intact.

Drawn in CI snapshots where they draw something, but not exercised automatically and not yet checked on a real desktop:

- The settings panel, the colour picker, and dragging and multi-selecting in edit layout.
- The word-by-word fill staying in time with a song.
- The transport buttons and seeking.
- Staying visible through Show Desktop (`Win+D`).
- Settings carrying over from the Deskbeat and Sonic Veil names.

The macOS and Linux preview is built and checked in CI only; it has not been run on a real Mac or Linux desktop.

## Build from source

You need Windows 11, stable Rust 1.89 or later with `rustfmt`, `clippy` and the MSVC build tools, and PowerShell 7 (`pwsh`).

```powershell
git clone https://github.com/akshit-bansal11/inlay.git
cd inlay
cargo run --release                             # the tray app; quit any other copy first
cargo run --release -- --snapshot snapshots     # every widget and the settings panel, to PNG
pwsh scripts/check.ps1                          # format, clippy with warnings as errors, tests
```

Only one copy runs at a time: a second launch hands over to the first and exits. `--snapshot` draws from made-up data with no GPU or Spotify, and it's how CI checks the drawing code.

On macOS or Linux, build the preview instead:

```bash
cargo run -p inlay-desk --release
```

On Linux you also need `libasound2-dev libdbus-1-dev libxkbcommon-dev pkg-config` and the xcb development packages.

| Path | What |
| --- | --- |
| `crates/core` | Pure logic with no Windows types: lyric parsing and timing, the sync clock, band mapping, colour, the config schema, the logo's geometry. Its tests run anywhere. |
| `crates/app` | The exe: windows, Direct2D drawing, the media session, audio capture, the settings panel, the tray. |
| `crates/desk` | The macOS and Linux preview: egui windows, AppleScript and MPRIS, audio input, the tray. |
| `scripts/` | The quality gate and the script that writes the exe's icon. |
| `docs/` | [How it works](docs/ARCHITECTURE.md). |
| `packaging/` | The winget manifests. |
| `site/` | The [website](site/README.md), with its own checks and workflow. |

## Contributing

- **A bug or an idea:** [open an issue](https://github.com/akshit-bansal11/inlay/issues/new/choose). Open one before starting anything bigger than a fix.
- **A security hole:** report it privately, as [SECURITY.md](SECURITY.md) describes.
- **A change:** branch from your fork, run the gate until it's clean, and open a pull request against `main`. [CONTRIBUTING.md](CONTRIBUTING.md) has the details.

[CHANGELOG.md](CHANGELOG.md) lists what each release changed.

## Support

Inlay is free and stays free. If it's useful to you and you'd like to say so with money:

- **[Ko-fi](https://ko-fi.com/akshit_bansal11)**, from anywhere.
- **[PayPal](https://paypal.me/AkshitBansal141)**, from anywhere.
- **UPI**, from any UPI app in India: `artistbansal2004@okaxis`

A star, a bug report or telling someone about it helps just as much.

## License and credits

[MIT](LICENSE).

- Line-timed lyrics come from [LRCLIB](https://lrclib.net). Optional word-timed lyrics come from community-run LyricsPlus servers.
- Built on [windows-rs](https://github.com/microsoft/windows-rs), [RustFFT](https://github.com/ejmahler/RustFFT), [serde](https://serde.rs) and [toml](https://github.com/toml-rs/toml). The macOS and Linux preview adds [egui and eframe](https://github.com/emilk/egui), [cpal](https://github.com/RustAudio/cpal), [mpris](https://github.com/Mange/mpris-rs), [tray-icon](https://github.com/tauri-apps/tray-icon), [ureq](https://github.com/algesten/ureq), [fontdb](https://github.com/RazrFalcon/fontdb) and [image](https://github.com/image-rs/image). All are MIT or Apache 2.0, or both.
- The clock's default pairing, Anurati and Quicksand, follows the Mond Rainmeter skin. Neither font is distributed with Inlay.
- Spotify is a trademark of Spotify AB. Inlay isn't affiliated with or endorsed by Spotify; it reads what the system reports about the app that's playing.
