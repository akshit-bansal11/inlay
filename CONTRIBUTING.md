# Contributing

Thanks for looking. Inlay is small on purpose; the bar for a change is that it makes the product better for someone using it, and that it passes the same gate CI runs.

## Before you start

- Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). The split between the crates, and the rule that a widget draws only when something changed, are deliberate.
- Open an issue for anything bigger than a fix, so the design can be talked through before code exists.

## Setting up

Fork the repository on GitHub and clone your fork. Your work lives on your fork, and reaches this repository as a pull request.

```powershell
git clone https://github.com/<you>/inlay.git inlay
cd inlay
git remote add upstream https://github.com/akshit-bansal11/inlay.git
```

You need Windows 11, the stable Rust toolchain (1.89 or later, with `rustfmt` and `clippy`) and the MSVC build tools that `rustup` asks for. PowerShell 7 (`pwsh`) runs the gate.

Build and run directly, without the gate, while iterating:

```powershell
cargo run --release                               # the tray app
cargo run --release -- --snapshot snapshots       # every widget drawn to PNG, from made-up data
```

Only one copy runs at a time. If a release build is already in your tray, quit it first, or the new one hands over to it and exits.

### macOS and Linux

`crates/desk` is the macOS and Linux preview. It is not a default workspace member, so name it. On macOS or Linux (Linux needs `libasound2-dev libdbus-1-dev libxkbcommon-dev pkg-config` and the xcb development packages):

```bash
cargo run -p inlay-desk --release
cargo clippy -p inlay-core -p inlay-desk --all-targets -- -D warnings
cargo test -p inlay-core -p inlay-desk
```

CI builds and checks both front-ends: the Windows gate for `crates/app`, and the two commands above for `crates/desk`.

### Where to start without Spotify

`crates/core` has no Windows types at all, and holds most of the logic that matters: lyric parsing and timing, the sync clock, band mapping, colour, the config schema and its migrations. Its tests run anywhere, in milliseconds.

`crates/app` needs Windows; `crates/desk` needs macOS or Linux. The snapshot mode draws every widget from made-up data, with no GPU, no Spotify and no audio device, which is how CI checks the drawing code and how you can see a visual change without a live session. What it cannot show is anything in the app loop: dragging, the settings panel's pointer handling, the tray, live reload. Those need a real run.

## Making a change

1. Bring your fork's `main` up to date with `upstream/main`, then branch from it on your fork.
2. Keep `crates/core` free of Windows types. Anything that needs a window, a device or Spotify goes in `crates/app` (Windows) or `crates/desk` (macOS and Linux).
3. A new setting is four things in one commit: the field with its default, its clamp in `Config::sanitized`, a row in the settings panel, and a test if an older config file has to keep loading.
4. Add or change a test when you add logic, a branch or fix a bug. Prefer testing in `crates/core` over anything that needs a window.
5. Run `pwsh scripts/check.ps1` until it is clean. It formats in place, then runs clippy with warnings as errors and the tests. CI runs the same script in its non-mutating `-Ci` mode: it writes locally because fixing your formatting is useful; CI must not write to your branch, so it fails instead. Do not suppress a lint finding to get green.
6. After changing a `Cargo.toml`, commit the `Cargo.lock` that `cargo` rewrites. The gate uses `--locked` and fails on a stale one.
7. Commit with a [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) subject (`feat:`, `fix:`, `docs:`, `ci:`, with an optional scope such as `feat(lyrics):`) and a body that says what changed and why, including what you rejected.
8. Add a line under **Unreleased** in [CHANGELOG.md](CHANGELOG.md).
9. Push the branch to your fork and open a pull request against `main` in this repository. CI runs the gate, builds the exe and renders the snapshots. The maintainer reviews and merges.

## Style

- No new dependency without a reason in the pull request. Check `Cargo.toml` for how many the workspace has.
- The `windows` crate stays on 0.62. Its API moves between minor versions; a bump is its own pull request.
- Constants are named. A number that appears twice is a constant.
- Doc comments say why, not what the next line already says.
- A widget that asks for a frame while nothing on it is moving is a bug. It is the whole reason this app exists instead of a Rainmeter skin.

## Releases

The maintainer builds and publishes releases. A pull request does not bump the version, tag, or build a release; a merged change ships in the next one.
