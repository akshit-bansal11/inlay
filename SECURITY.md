# Security

Inlay draws on your desktop, listens to what Spotify plays and asks the internet for lyrics. This is what it touches, what leaves the PC, and how to report a hole.

## The model

- **No account, no login, no telemetry.** There is no Spotify client ID and no token. On Windows, track details and the play, pause and skip buttons go through the Windows media session API, the same one the volume flyout uses.
- **What leaves the PC is the track you are playing, to lyric services.** For each new track, its title, artist, album and length are sent over HTTPS to [LRCLIB](https://lrclib.net) whenever the lyrics widget is on. Only when word timing is on (`word_sync = true`, off by default) are they also sent to the LyricsPlus servers listed under `word_servers` in the config. Only `https://` servers are accepted. Nothing else is sent: no identifier, no listening history, no audio. Turning the lyrics widget off stops both; `word_sync = false` stops the second.
- **The LyricsPlus servers are community-run mirrors, not a service with a contract.** They serve word-timed lyrics mirrored from other services, and Inlay does not host or check them. Whoever runs one sees the requests above, and your IP address as any web server does. Turn word timing on at your own discretion. The list is yours to edit or empty.
- **Audio never leaves the process.** The visualizer captures Spotify's output through WASAPI loopback, turns it into bar heights and discards it. Nothing is recorded or written to disk.
- **Lyric text is treated as text.** A response is parsed as JSON and drawn with DirectWrite. Nothing in it is executed, and nothing from it reaches a shell, a path or a URL.
- **It runs as you, with no elevation.** It writes only its own folders, `%APPDATA%\inlay` and `%LOCALAPPDATA%\inlay`, and one value under `HKCU\...\Run` when Start with Windows is on.
- **On macOS and Linux (preview) the same rules hold, with these differences.** On macOS, Spotify is read and controlled through AppleScript, so macOS asks once whether Inlay may control Spotify. On Linux it goes through MPRIS on the session D-Bus. Album art is downloaded from the `https://` address Spotify gives. The visualizer listens to the system's default input device, which may be a microphone if you have not pointed it at a monitor of your speakers; audio is turned into bar heights and discarded, as on Windows. The macOS app is ad-hoc signed and not notarized.
- **No secrets in the repository.** The release workflow has none to hold: its only credential is the job's own token.

## Known limits

- The exe is not code-signed, so SmartScreen warns on first run. Verify a download against the release's `SHA256SUMS.txt`, or with `gh attestation verify Inlay-<version>.exe --repo akshit-bansal11/inlay`, which proves the file was built by this repository's release workflow at that tag. Files of 2.0.0 and earlier were built under the old name: use `--repo akshit-bansal11/deskbeat` for those.
- Anyone who can write `%APPDATA%\inlay\config.toml` can point `word_servers` at a server of their choosing and learn what you play. That person can already run programs as you.
- Fonts in `%APPDATA%\inlay\fonts` are loaded by DirectWrite. A malicious font file is a risk for any program that renders it; only put fonts there that you trust.

## Reporting

Open a [security advisory](https://github.com/akshit-bansal11/inlay/security/advisories/new) on GitHub; it reaches the maintainer privately. Please do not open a public issue for something exploitable. Expect an acknowledgement within a week.
