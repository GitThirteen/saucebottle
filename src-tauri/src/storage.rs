// SauceBottle - An anime artwork sorter daemon written in Tauri & Rust.
// Copyright © 2026    Thirteen
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

// ---------------------------------*
// ---- STORAGE.RS -----------------*
// ---------------------------------*

/// How long a folder gets to answer before it counts as unreachable.
/// Windows blocks for about 20 seconds on a network share that is switched off.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Returns `true` if the folder (or the closest parent that exists, if the folder is not there yet)
/// answers within `timeout`. This tells a missing network share or drive apart from a folder that
/// just hasn't been created yet.
///
/// The check runs on its own thread, because a dead network share can block for much longer than
/// we want to wait. The thread finishes on its own in the background.
///
/// # Arguments
/// * `path` - The folder to check.
/// * `timeout` - How long to wait for an answer.
pub fn is_reachable(path: &Path, timeout: Duration) -> bool {
    let path = path.to_path_buf();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let answered = path
            .ancestors()
            .filter(|ancestor| !ancestor.as_os_str().is_empty())
            .any(|ancestor| fs::metadata(ancestor).is_ok());
        let _ = tx.send(answered);
    });

    rx.recv_timeout(timeout).unwrap_or(false)
}

/// A path the way the user knows it, for showing or for handing to the file explorer.
/// Canonical Windows paths start with `\\?\`, and network shares with `\\?\UNC\`
/// (`\\?\UNC\nas\share` is `\\nas\share`), so the second one can't just be stripped like the first.
pub fn plain_path(path: &Path) -> String {
    let text = path.to_string_lossy();

    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", rest)
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn plain_path_keeps_network_shares_intact() {
        assert_eq!(plain_path(&PathBuf::from(r"\\?\UNC\nas\share\Images")), r"\\nas\share\Images");
        assert_eq!(plain_path(&PathBuf::from(r"\\?\C:\Users\me")), r"C:\Users\me");
        assert_eq!(plain_path(&PathBuf::from(r"C:\Users\me")), r"C:\Users\me");
        assert_eq!(plain_path(&PathBuf::from("/home/me")), "/home/me");
    }

    #[test]
    fn missing_folder_on_an_existing_parent_is_reachable() {
        let missing = std::env::temp_dir().join("saucebottle_does_not_exist").join("deeper");
        assert!(is_reachable(&missing, Duration::from_secs(5)));
    }
}
