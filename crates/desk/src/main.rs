//! Inlay on macOS and Linux. The Windows app is crates/app; on Windows this binary
//! only says so, so a workspace build there still compiles.
#[cfg(not(windows))]
mod app;
#[cfg(not(windows))]
mod clock;
#[cfg(not(windows))]
mod paths;
#[cfg(not(windows))]
mod settings;

#[cfg(not(windows))]
fn main() -> eframe::Result {
    app::run()
}

#[cfg(windows)]
fn main() {
    eprintln!("inlay-desk is the macOS and Linux build; on Windows run the inlay crate.");
}
