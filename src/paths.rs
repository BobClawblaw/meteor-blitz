//! Where the game data is read from and where saves go (port addition).
//!
//! From the repo (`cargo run`), both are the working directory, as before.
//! A release build carries its data beside the executable (Windows zip) or
//! in the macOS bundle's `Contents/Resources`, which may be read-only, so
//! its saves go to the user's app-data folder instead. `MB_ROOT` still
//! points the game at a data directory explicitly.

use std::path::PathBuf;
use std::sync::OnceLock;

/// The save folder, once the game knows it runs packaged.
static SAVE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Finds the data and switches to it; call before loading anything.
pub fn init() {
    let mut packaged = None;
    if let Some(root) = std::env::var_os("MB_ROOT") {
        let _ = std::env::set_current_dir(root);
    } else if let Some(data) = bundled_data() {
        let _ = std::env::set_current_dir(&data);
        packaged = user_dir();
    }
    if let Some(dir) = &packaged {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = SAVE_DIR.set(packaged);
}

/// A save file's path: in the app-data folder when packaged, else in the
/// working directory.
pub fn save(name: &str) -> PathBuf {
    match SAVE_DIR.get() {
        Some(Some(dir)) => dir.join(name),
        _ => PathBuf::from(name),
    }
}

/// The data directory of a packaged build: `../Resources` from the
/// executable in a macOS bundle, or the executable's own directory.
#[cfg(not(target_arch = "wasm32"))]
fn bundled_data() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join("../Resources"), dir.to_path_buf()]
        .into_iter()
        .find(|d| d.join("assets").is_dir() && d.join("original").is_dir())
}

#[cfg(target_arch = "wasm32")]
fn bundled_data() -> Option<PathBuf> {
    None
}

/// The per-user save folder for "Meteor Blitz".
fn user_dir() -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        Some(home()?.join("Library/Application Support/Meteor Blitz"))
    } else if cfg!(windows) {
        Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Meteor Blitz"))
    } else {
        let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| Some(home()?.join(".local/share")))?;
        Some(base.join("meteor-blitz"))
    }
}
