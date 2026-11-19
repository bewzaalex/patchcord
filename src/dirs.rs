//! Where Patchcord keeps its files, in each system's usual place.

use std::path::PathBuf;

/// The settings folder: `~/.config/patchcord` (or `$XDG_CONFIG_HOME`), `%APPDATA%\patchcord`,
/// or `~/Library/Application Support/patchcord`.
pub fn config() -> Option<PathBuf> {
    base("APPDATA", "Library/Application Support", "XDG_CONFIG_HOME", ".config")
}

/// Where a report the user is to attach somewhere goes: the home folder (until it asks, see
/// issue #17).
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn base(windows: &str, macos: &str, xdg: &str, home: &str) -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        env(windows)
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join(macos))
    } else {
        env(xdg).or_else(|| env("HOME").map(|h| h.join(home)))
    }?;
    Some(base.join("patchcord"))
}
