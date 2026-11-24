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

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc::Sender, mpsc::channel};
use std::time::{Duration, Instant, SystemTime};

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::ApiClient;
use crate::models::Failure;
use crate::processor;
use crate::stats::StatsStore;
use crate::storage;

// ---------------------------------*
// ---- LISTENER.RS ----------------*
// ---------------------------------*

// [TODO] Bit of a general thing: SauceBottle is still a bit of a mess in regards to logging.
// Certain println! should either be logged to the frontend or deleted, but right now some messages do get logged while others are internal-only.
// This is messy and needs a thorough clean-up.

/// How long a file's size and modification time must stay unchanged before we trust that it's complete.
const SETTLE_TIME: Duration = Duration::from_millis(300);
/// Empty files are often placeholders that get filled a moment later (e.g. by browsers), so they get longer.
const EMPTY_SETTLE_TIME: Duration = Duration::from_secs(10);
/// A non-empty file that hasn't been written to for this long is considered settled without waiting.
const STALE_AGE: Duration = Duration::from_secs(5);
/// Give up if the file stays locked or keeps changing without settling for this long.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Safety-net interval for re-sweeping the input folder, catching anything the OS watcher missed.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);

/// Extensions of in-progress downloads/transfers. These files get renamed once they're done,
/// so we ignore them and pick up the final file instead.
const TEMP_EXTENSIONS: [&str; 7] = ["part", "crdownload", "tmp", "partial", "download", "opdownload", "!ut"];

/// Returns `true` if the path looks like an unfinished download/transfer.
fn is_temp_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| TEMP_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

/// The file's name for showing to the user. The folder is always the same, so it's left out.
fn file_label(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Outcome of waiting for a file to be fully written.
enum FileState {
    /// The file is complete and nobody else has it open.
    Ready,
    /// The file disappeared in the meantime (already moved/renamed by someone else).
    Gone,
}

/// Tries to open the file in a way that fails while *any* other process still has it open.
/// Unlike opening for writing, this can't be fooled by writers that allow shared write access
/// (C `fopen`, .NET, most image editors and save dialogs).
#[cfg(windows)]
fn is_unlocked(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().read(true).share_mode(0).open(path).is_ok()
}

/// Other OSes have no mandatory locking, so we rely on the size/mtime settle check alone.
#[cfg(not(windows))]
fn is_unlocked(path: &Path) -> bool {
    std::fs::File::open(path).is_ok()
}

/// Asynchronously waits for a file to finish writing to disk.
/// A file counts as ready once its size and modification time have stopped changing
/// *and* no other process has it open anymore. Polls with an exponential backoff
/// (10ms, x1.5, capped at 150ms) to stay cheap on large files or slow drives.
///
/// # Arguments
/// * `path` - The file path of the file we want to read.
async fn wait_for_file_ready(path: &Path) -> Result<FileState, &'static str> {
    let mut current_delay = Duration::from_millis(10);
    let max_delay = Duration::from_millis(150);

    let mut last_seen: Option<(u64, Option<SystemTime>)> = None;
    let mut unchanged_since = Instant::now();

    loop {
        match std::fs::metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(FileState::Gone),
            Err(_) => {} // Transient (e.g. locked by a cloud sync client), just try again
            Ok(meta) => {
                let len = meta.len();
                let modified = meta.modified().ok();

                if last_seen != Some((len, modified)) {
                    last_seen = Some((len, modified));
                    unchanged_since = Instant::now();
                }

                let settle_time = if len == 0 { EMPTY_SETTLE_TIME } else { SETTLE_TIME };
                let untouched_for = modified
                    .and_then(|m| SystemTime::now().duration_since(m).ok())
                    .unwrap_or_default();
                let settled = unchanged_since.elapsed() >= settle_time
                    || (len > 0 && untouched_for >= STALE_AGE);

                if settled && is_unlocked(path) {
                    return Ok(FileState::Ready);
                }
            }
        }

        if unchanged_since.elapsed() > STALL_TIMEOUT {
            return Err("Timed out waiting for the file to finish writing (is another program still using it?)");
        }

        tokio::time::sleep(current_delay).await;

        // Increase the delay by x1.5 for the next loop, capping at max_delay
        current_delay = std::cmp::min(current_delay.mul_f32(1.5), max_delay);
    }
}

/// Handles the end-to-end processing lifecycle of a single image file.
/// This includes file validation, querying IQDB, formatting data, moving the file
/// to the appropriate results/invalid folder, and emitting state updates to the UI.
///
/// # Arguments
/// * `path` - The exact file path of the image being processed.
/// * `client` - Thread-safe reference to the API client for making HTTP requests.
/// * `handle` - The Tauri AppHandle used to emit progress events to the frontend.
/// * `queued_tracker` - Thread-safe set of currently processing file paths to prevent duplicates.
async fn process_single_file(
    path: PathBuf,
    client: Arc<ApiClient>,
    handle: AppHandle,
    queued_tracker: Arc<Mutex<HashSet<PathBuf>>>,
) {
    // Every file goes through this, including ones found by a sweep: a sweep can just as well
    // run while a file is still being copied into the input folder.
    match wait_for_file_ready(&path).await {
        Ok(FileState::Ready) => {}
        Ok(FileState::Gone) => {
            // Already moved away (e.g. a duplicate event for a file we just sorted) - nothing to do
            queued_tracker.lock().unwrap().remove(&path);
            let _ = handle.emit("task-done", ());
            return;
        }
        Err(e) => {
            queued_tracker.lock().unwrap().remove(&path);
            let _ = handle.emit("failure", Failure { file: file_label(&path), message: e.to_string(), moved_to: None });
            let _ = handle.emit("task-done", ());
            return;
        }
    }

    // Never empty: Yande.re needs no credentials and is always available
    let active_services = client.get_active_credentials();

    let config_snapshot = client.config();

    let default_results_dir = handle
        .path()
        .picture_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("SauceBottle")
        .join("results");

    // keep this alive for as long as `invalid_folder`
    let invalid_folder_temp;
    let invalid_folder = if config_snapshot.invalid_folder.trim().is_empty() {
        invalid_folder_temp = default_results_dir.join(".invalid");
        invalid_folder_temp.to_string_lossy()
    } else {
        (&config_snapshot.invalid_folder).into()
    };

    let started = Instant::now();

    let process_result = async {
        let info = processor::process_image(&path)?;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let detected = info.format.to_lowercase();

        // Extensions that are just alternative names for the detected format
        let is_mismatch = match ext.as_str() {
            "jpg" | "jpeg" | "jpe" | "jfif" => detected != "jpeg",
            "tif" | "tiff" => detected != "tiff",
            "heic" | "heif" => detected != "heic",
            "" => false,
            _ => ext != detected,
        };

        if is_mismatch {
            let msg = format!(
                "{}: the extension says .{}, but the file is actually a {} image",
                info.filename, ext, info.format
            );
            let _ = handle.emit("warn", msg.clone());
            println!("{}", msg);
        }

        let _ = handle.emit("image-processing", info.clone());

        let payload = processor::get_iqdb_payload(&path, &info, &config_snapshot)?;

        let mut booru_data = client
            .search_iqdb(payload.clone(), &active_services, &handle)
            .await?;

        let mut ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("jpg")
            .to_string();

        let apply_mods = config_snapshot
            .flags
            .get("applyModsToSaved")
            .copied()
            .unwrap_or(false);
        if apply_mods {
            let is_raw_supported = matches!(
                info.format.to_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif"
            );
            let needs_resize = info.width > 7500 || info.height > 7500;
            let needs_shrink = (info.size_kb * 1024) > (8 * 1024 * 1024);

            if !is_raw_supported || needs_resize || needs_shrink {
                ext = "jpg".to_string(); // get_iqdb_payload always outputs JPEGs when it modifies files
            }
        }

        let new_p = processor::move_to_results(
            &path,
            &booru_data,
            &ext,
            &config_snapshot,
            Some(&payload),
            &default_results_dir,
        )?;

        // Populate a clean display path (e.g. "Arknights/Yvonne/D12345.png")
        let output_base = if config_snapshot.output_folder.trim().is_empty() {
            default_results_dir.to_string_lossy()
        } else {
            (&config_snapshot.output_folder).into()
        };
        let clean_full = new_p.to_string_lossy().replace("\\\\?\\", "");
        let display_path = clean_full
            .strip_prefix(&*output_base)
            .unwrap_or(&clean_full)
            .trim_start_matches(['/', '\\'])
            .replace('\\', "/");
        booru_data.file_path = display_path;
        booru_data.source_file = file_label(&path);

        if let Some(stats) = handle.try_state::<StatsStore>() {
            stats.record_sort(&config_snapshot, &booru_data, info.size_kb, started.elapsed());
        }

        let _ = handle.emit("success", booru_data);
        Ok::<(), String>(())
    }
    .await;

    if let Err(e) = process_result {
        if let Some(stats) = handle.try_state::<StatsStore>() {
            stats.record_failure(&config_snapshot);
        }
        println!(
            "Error: Error processing {:?}: {}",
            path.file_name().unwrap_or_default(),
            e
        );
        let invalid_name = match config_snapshot.invalid_folder.trim() {
            "" => ".invalid",
            name => name,
        };

        // Say where the image ended up, or why it couldn't be moved
        let (message, moved_to) = match processor::move_to_invalid(&path, &config_snapshot, &default_results_dir) {
            Ok(()) => {
                println!("Error: Moved to invalid folder: {}", invalid_folder);
                (e, Some(invalid_name.to_string()))
            }
            Err(move_err) => {
                println!("Error: Failed to move to invalid folder: {}", move_err);
                (format!("{}. It also could not be moved to the {} folder: {}", e, invalid_name, move_err), None)
            }
        };

        let _ = handle.emit("failure", Failure { file: file_label(&path), message, moved_to });
    }

    queued_tracker.lock().unwrap().remove(&path);
    let _ = handle.emit("task-done", ());
}

/// Performs a manual sweep of the input directory to find files that already exist on disk.
/// This acts as a catch-up mechanism for files added while the app was closed or paused.
///
/// # Arguments
/// * `handle` - The Tauri AppHandle for emitting queue events.
/// * `input_dir` - The input folder to sweep (see `config::input_dir`).
/// * `tx` - The transmit channel to send found files to the processor queue.
/// * `queued_tracker` - Thread-safe set to prevent re-queuing files already in progress.
///
/// # Returns
/// * `usize` - The number of files newly added to the queue (files already queued aren't counted).
pub fn run_sweep(
    handle: &AppHandle,
    input_dir: &Path,
    tx: &Sender<PathBuf>,
    queued_tracker: &Arc<Mutex<HashSet<PathBuf>>>,
) -> usize {
    let watch_path = input_dir;
    let mut queued = 0;

    println!("Sweeping folder for unprocessed files...");
    if let Ok(entries) = std::fs::read_dir(watch_path) {
        let mut tracker = queued_tracker.lock().unwrap();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && !is_temp_file(&path) && tracker.insert(path.clone()) {
                let _ = handle.emit("queue-add", file_label(&path));
                let _ = tx.send(path);
                queued += 1;
            }
        }
    }

    queued
}

/// Messages for the file-system watcher thread.
enum WatchMsg {
    /// An event (or error) reported by the OS watcher.
    Event(notify::Result<notify::Event>),
    /// Watch this folder from now on instead of the current one.
    SetInputDir(PathBuf),
    /// Watch the current folder again (a network share that came back has lost its watch).
    Rewatch,
}

/// Lets other parts of the app point the running watcher at a different input folder.
#[derive(Clone)]
pub struct WatchControl(Sender<WatchMsg>);

impl WatchControl {
    /// Switches the watcher to another input folder. Does nothing if it's already watched.
    pub fn set_input_dir(&self, dir: PathBuf) {
        let _ = self.0.send(WatchMsg::SetInputDir(dir));
    }

    /// Watches the current input folder again.
    pub fn rewatch(&self) {
        let _ = self.0.send(WatchMsg::Rewatch);
    }
}

/// Tracks whether the input and output folders can be reached. This matters when one of them is on
/// a network share or an external drive.
///
/// An image is searched on IQDB before its file is moved. With an unreachable folder, every image
/// would be searched, fail to move, and be searched again by the next sweep. So processing waits
/// until both folders answer, and the user gets one message when that starts and one when it ends.
struct StorageGuard {
    handle: AppHandle,
    client: Arc<ApiClient>,
    watch: WatchControl,
    down: AtomicBool,
}

impl StorageGuard {
    /// Returns `true` if the input and output folder can both be reached right now.
    /// Warns when that stops being true. When they're back, says so and sets up the folder watch again.
    fn is_up(&self) -> bool {
        let config = self.client.config();
        let folders = [
            ("input", crate::config::input_dir(&self.handle, &config)),
            ("output", crate::config::results_dir(&self.handle, &config)),
        ];

        let unreachable: Vec<String> = folders
            .iter()
            .filter(|(_, dir)| !storage::is_reachable(dir, storage::PROBE_TIMEOUT))
            .map(|(name, dir)| format!("{} folder ({})", name, storage::plain_path(dir)))
            .collect();

        if unreachable.is_empty() {
            if self.down.swap(false, Ordering::SeqCst) {
                let _ = self.handle.emit("info", "The input and output folders are reachable again. Processing continues.");
                self.watch.rewatch();
            }
            true
        } else {
            if !self.down.swap(true, Ordering::SeqCst) {
                let _ = self.handle.emit(
                    "warn",
                    format!("Can't reach the {}. Processing is on hold until it is available again.", unreachable.join(" and the ")),
                );
            }
            false
        }
    }
}

/// Initializes the core background daemon for SauceBottle.
/// Spawns three independent threads:
/// 1. An async worker thread that drains the processing channel queue one by one.
/// 2. An OS file-system watcher thread that listens for new files appearing in the input directory.
/// 3. A heartbeat thread that periodically sweeps the input directory as a safety net for missed watcher events.
///
/// # Arguments
/// * `handle` - The Tauri AppHandle.
/// * `api_client` - Thread-safe reference to the API client.
/// * `is_scanning` - Atomic flag controlling whether the app should process or ignore files (Pause/Resume feature).
///
/// # Returns
/// * `(Sender<PathBuf>, Arc<Mutex<HashSet<PathBuf>>>, WatchControl)` - Returns the channel transmitter so external
///   commands (like `run_sweep`) can feed files into the queue, the active queue tracker, and a handle
///   to switch the watched input folder at runtime.
pub fn spawn_watcher(
    handle: AppHandle,
    api_client: Arc<ApiClient>,
    is_scanning: Arc<AtomicBool>,
) -> (Sender<PathBuf>, Arc<Mutex<HashSet<PathBuf>>>, WatchControl) {
    let (proc_tx, proc_rx) = channel::<PathBuf>();
    let queued_tracker = Arc::new(Mutex::new(HashSet::new()));

    let processor_handle = handle.clone();
    let processor_scanning = is_scanning.clone();
    let worker_tracker = queued_tracker.clone();
    let heartbeat_client = api_client.clone();
    let observer_client = api_client.clone();

    let (watch_msg_tx, watch_msg_rx) = channel::<WatchMsg>();
    let watch_control = WatchControl(watch_msg_tx.clone());

    let storage_guard = Arc::new(StorageGuard {
        handle: handle.clone(),
        client: api_client.clone(),
        watch: watch_control.clone(),
        down: AtomicBool::new(false),
    });
    let worker_guard = storage_guard.clone();

    // Thread 1: Worker/Processor
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        for path in proc_rx {
            if !processor_scanning.load(Ordering::Relaxed) {
                let is_permanent = api_client.config().flags.get("isPermanentScan").copied().unwrap_or(true);

                if is_permanent {
                    while !processor_scanning.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(150));
                    }
                } else {
                    worker_tracker.lock().unwrap().remove(&path);
                    let _ = processor_handle.emit("task-done", ());
                    continue;
                }
            }

            // Wait here while the input or output folder can't be reached (see `StorageGuard`)
            while !worker_guard.is_up() {
                std::thread::sleep(Duration::from_secs(10));
            }

            rt.block_on(async {
                process_single_file(
                    path,
                    api_client.clone(),
                    processor_handle.clone(),
                    worker_tracker.clone(),
                )
                .await;
            });
        }
    });

    let watch_tx_clone = proc_tx.clone();
    let observer_tracker = queued_tracker.clone();
    let observer_scanning = is_scanning.clone();
    let observer_handle = handle.clone();

    // Thread 2: OS File-System Observer
    std::thread::spawn(move || {
        let handle = observer_handle;
        let mut input_dir = crate::config::input_dir(&handle, &observer_client.config());

        if let Err(e) = std::fs::create_dir_all(&input_dir) {
            println!("Error: Could not create input folder {:?}: {}", input_dir, e);
        }

        let event_tx = watch_msg_tx;
        let mut watcher = RecommendedWatcher::new(
            move |res| { let _ = event_tx.send(WatchMsg::Event(res)); },
            Config::default(),
        )
        .expect("Watcher failed");
        if let Err(e) = watcher.watch(&input_dir, RecursiveMode::Recursive) {
            println!("Error: Could not watch input folder {:?}: {}", input_dir, e);
        }

        for msg in watch_msg_rx {
            let res = match msg {
                WatchMsg::Event(res) => res,
                WatchMsg::Rewatch => {
                    let _ = watcher.unwatch(&input_dir);
                    let _ = std::fs::create_dir_all(&input_dir);
                    if let Err(e) = watcher.watch(&input_dir, RecursiveMode::Recursive) {
                        println!("Error: Could not watch input folder {:?}: {}", input_dir, e);
                    }
                    continue;
                }
                WatchMsg::SetInputDir(new_dir) => {
                    if new_dir == input_dir {
                        continue;
                    }

                    let _ = watcher.unwatch(&input_dir);
                    let switched = std::fs::create_dir_all(&new_dir)
                        .map_err(|e| e.to_string())
                        .and_then(|_| watcher.watch(&new_dir, RecursiveMode::Recursive).map_err(|e| e.to_string()));

                    match switched {
                        Ok(()) => {
                            println!("Now watching input folder {:?}", new_dir);
                            input_dir = new_dir;
                        }
                        Err(e) => {
                            // Go back to the old folder so we don't end up watching nothing
                            let _ = watcher.watch(&input_dir, RecursiveMode::Recursive);
                            let _ = handle.emit("warn", format!("Could not watch the new input folder {:?}: {}", new_dir, e));
                        }
                    }
                    continue;
                }
            };
            let Ok(event) = res else { continue }; // Skip if invalid event

            if !observer_scanning.load(Ordering::Relaxed) { // Skip if scanning is turned off
                continue;
            }

            // The OS dropped events (e.g. buffer overflow) - we can't know what we missed, so sweep
            if event.need_rescan() {
                run_sweep(&handle, &input_dir, &watch_tx_clone, &observer_tracker);
                continue;
            }

            // Files can appear either by being created, or by being moved/renamed into the folder
            // (Explorer moves on the same drive, browsers renaming finished downloads, ...)
            let is_arrival = match event.kind {
                EventKind::Create(_) => true,
                EventKind::Modify(ModifyKind::Name(mode)) => mode != RenameMode::From,
                _ => false,
            };
            if !is_arrival {
                continue;
            }

            for path in event.paths {
                if !path.is_file() || is_temp_file(&path) { // Skip folders, vanished paths and unfinished downloads
                    continue;
                }

                if !observer_tracker.lock().unwrap().insert(path.clone()) { // Skip if already being tracked
                    continue;
                }

                println!("Detected new drop: {:?}", path);
                let _ = handle.emit("queue-add", file_label(&path));
                let _ = watch_tx_clone.send(path);
            }
        }
    });

    let heartbeat_tx = proc_tx.clone();
    let heartbeat_tracker = queued_tracker.clone();

    // Thread 3: Heartbeat sweep
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(HEARTBEAT_INTERVAL);
            if is_scanning.load(Ordering::Relaxed) && storage_guard.is_up() {
                let input_dir = crate::config::input_dir(&handle, &heartbeat_client.config());
                run_sweep(&handle, &input_dir, &heartbeat_tx, &heartbeat_tracker);
            }
        }
    });

    // [TODO] We might need more threads here.
    // E.g.:
    // - A separate thread for the batch DL tab so the backend handles this gracefully. Right now, the frontend actually handles that asynchronously, and in the >>> worst case <<< this could result in an IP ban.
    // - Others?

    (proc_tx, queued_tracker, watch_control)
}
