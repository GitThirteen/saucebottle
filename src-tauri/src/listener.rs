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
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc::Sender, mpsc::channel};
use std::time::Duration;

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::ApiClient;
use crate::processor;

// ---------------------------------*
// ---- LISTENER.RS ----------------*
// ---------------------------------*

// [TODO] Bit of a general thing: SauceBottle is still a bit of a mess in regards to logging.
// Certain println! should either be logged to the frontend or deleted, but right now some messages do get logged while others are internal-only.
// This is messy and needs a thorough clean-up.

/// Asynchronously waits for a file to finish writing to disk by polling for exclusive write access.
/// Uses an exponential backoff strategy, starting at 10ms for small files, increasing the interval
/// by x1.5 to prevent CPU pegging on larger files or slow drives.
/// 
/// # Arguments
/// * `path` - The file path of the file we want to read.
async fn wait_for_file_ready(path: &PathBuf) -> Result<(), &'static str> {
    let start_time = std::time::Instant::now();

    let mut current_delay = Duration::from_millis(10);
    let max_delay = Duration::from_millis(150);
    let timeout = Duration::from_secs(10);

    loop {
        match std::fs::OpenOptions::new().append(true).open(path) {
            Ok(_) => {
                return Ok(());
            }
            Err(error) => {
                // Cloud transfers often flag files as Read-Only.
                // If it's read-only, append(true) will always fail.
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    if let Ok(metadata) = std::fs::metadata(path) {
                        if metadata.permissions().readonly() {
                            // Ensure we can at least read it before giving the green light
                            if std::fs::OpenOptions::new().read(true).open(path).is_ok() {
                                return Ok(());
                            }
                        }
                    }
                }

                if start_time.elapsed() > timeout {
                    return Err("Timed out waiting for file to finish writing to disk.");
                }

                tokio::time::sleep(current_delay).await;

                // Increase the delay by x1.5 for the next loop, capping at max_delay
                current_delay = std::cmp::min(
                    current_delay.mul_f32(1.5), 
                    max_delay
                );
            }
        }
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
/// * `is_new_drop` - True if the file was *just* created by the OS. Applies a brief delay
///                   to ensure the OS has finished writing the file before we try to read it. (This is actually not very good, see [WARN]!)
/// * `queued_tracker` - Thread-safe set of currently processing file paths to prevent duplicates.
async fn process_single_file(
    path: PathBuf,
    client: Arc<ApiClient>,
    handle: AppHandle,
    is_new_drop: bool,
    queued_tracker: Arc<Mutex<HashSet<PathBuf>>>,
) {
    if is_new_drop {
        if let Err(e) = wait_for_file_ready(&path).await {
            queued_tracker.lock().unwrap().remove(&path);
            let _ = handle.emit("failure", format!("File read error on {:?}: {}", path.file_name().unwrap_or_default(), e));
            let _ = handle.emit("task-done", ());
            return;
        }
    }

    let active_services = client.get_active_credentials();
    if active_services.is_empty() {
        queued_tracker.lock().unwrap().remove(&path);

        let _ = handle.emit(
            "failure",
            "No active API credentials configured. Please add keys in the Credentials tab."
                .to_string(),
        );
        let _ = handle.emit("task-done", ());
        return;
    }

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

    let process_result = async {
        let info = processor::process_image(&path)?;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let detected = info.format.to_lowercase();

        let is_mismatch = match ext.as_str() {
            "jpg" | "jpeg" => detected != "jpeg",
            "tif" | "tiff" => detected != "tiff",
            "" => false,
            _ => ext != detected,
        };

        if is_mismatch {
            let msg = format!(
                "Filetype mismatch on '{}': extension is .{}, but the file is actually a {}",
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

        let _ = handle.emit("success", booru_data);
        Ok::<(), String>(())
    }
    .await;

    if let Err(e) = process_result {
        println!(
            "Error: Error processing {:?}: {}",
            path.file_name().unwrap_or_default(),
            e
        );
        if let Err(move_err) =
            processor::move_to_invalid(&path, &config_snapshot, &default_results_dir)
        {
            println!("Error: Failed to move to invalid folder: {}", move_err);
        } else {
            println!("Error: Moved to invalid folder: {}", invalid_folder);
        }

        let _ = handle.emit("failure", e);
    }

    queued_tracker.lock().unwrap().remove(&path);
    let _ = handle.emit("task-done", ());
}

/// Performs a manual sweep of the input directory to find files that already exist on disk.
/// This acts as a catch-up mechanism for files added while the app was closed or paused.
///
/// # Arguments
/// * `handle` - The Tauri AppHandle for resolving directory paths.
/// * `tx` - The transmit channel to send found files to the processor queue.
/// * `queued_tracker` - Thread-safe set to prevent re-queuing files already in progress.
pub fn run_sweep(
    handle: &AppHandle,
    tx: &Sender<(PathBuf, bool)>,
    queued_tracker: &Arc<Mutex<HashSet<PathBuf>>>,
) {
    // [TODO] This is defined twice, mild bug hazard
    let app_dir = handle.path().picture_dir().expect("Path resolution failed");
    let watch_path = app_dir.join("SauceBottle").join("input");

    println!("Sweeping folder for unprocessed files...");
    if let Ok(entries) = std::fs::read_dir(watch_path) {
        let mut tracker = queued_tracker.lock().unwrap();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && tracker.insert(path.clone()) {
                let _ = handle.emit("queue-add", ());
                let _ = tx.send((path, false)); // already on disk - no write-delay needed
            }
        }
    }
}

/// Initializes the core background daemon for SauceBottle.
/// Spawns two independent threads:
/// 1. An async worker thread that drains the processing channel queue one by one.
/// 2. An OS file-system watcher thread that listens for new files being created in the input directory.
///
/// # Arguments
/// * `handle` - The Tauri AppHandle.
/// * `api_client` - Thread-safe reference to the API client.
/// * `is_scanning` - Atomic flag controlling whether the app should process or ignore files (Pause/Resume feature).
///
/// # Returns
/// * `(Sender<(PathBuf, bool)>, Arc<Mutex<HashSet<PathBuf>>>)` - Returns the channel transmitter so external commands
///                                                              (like `run_sweep`) can feed files into the queue,
///                                                              along with the active queue tracker.
pub fn spawn_watcher(
    handle: AppHandle,
    api_client: Arc<ApiClient>,
    is_scanning: Arc<AtomicBool>,
) -> (Sender<(PathBuf, bool)>, Arc<Mutex<HashSet<PathBuf>>>) {
    let (proc_tx, proc_rx) = channel::<(PathBuf, bool)>();
    let queued_tracker = Arc::new(Mutex::new(HashSet::new()));

    let processor_handle = handle.clone();
    let processor_scanning = is_scanning.clone();
    let worker_tracker = queued_tracker.clone();

    // Thread 1: Worker/Processor
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        for (path, is_new_drop) in proc_rx {
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
            
            rt.block_on(async {
                process_single_file(
                    path,
                    api_client.clone(),
                    processor_handle.clone(),
                    is_new_drop,
                    worker_tracker.clone(),
                )
                .await;
            });
        }
    });

    let watch_tx_clone = proc_tx.clone();
    let observer_tracker = queued_tracker.clone();

    // Thread 2: OS File-System Observer
    std::thread::spawn(move || {
        let app_data_dir = handle
            .path()
            .picture_dir()
            .expect("Failed to resolve observer path");
        let input_dir = app_data_dir.join("SauceBottle").join("input");

        std::fs::create_dir_all(&input_dir).expect("Dir creation failed");

        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(tx, Config::default()).expect("Watcher failed");
        watcher
            .watch(&input_dir, RecursiveMode::Recursive)
            .expect("Watch failed");

        for res in rx {
            let Ok(event) = res else { continue }; // Skip if invalid event
            
            if !event.kind.is_create() { // Skip if not a file creation event
                continue;
            }

            for path in event.paths {
                if !path.is_file() { // Skip if not a file
                    continue;
                }

                if !is_scanning.load(Ordering::Relaxed) { // Skip if scanning is turned off
                    continue;
                }

                if !observer_tracker.lock().unwrap().insert(path.clone()) { // Skip if already being tracked
                    continue;
                }

                println!("Detected new drop: {:?}", path);
                let _ = handle.emit("queue-add", ());
                let _ = watch_tx_clone.send((path, true));
            }
        }
    });

    // [TODO] We might need more threads here.
    // E.g.:
    // - A separate thread for the batch DL tab so the backend handles this gracefully. Right now, the frontend actually handles that asynchronously, and in the >>> worst case <<< this could result in an IP ban.
    // - Others?

    (proc_tx, queued_tracker)
}
