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

mod api;
mod config;
mod listener;
mod models;
mod processor;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keyring_core::Entry;
use tauri::{Emitter, Manager};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::api::ApiClient;
use crate::models::AppConfig;

// ---------------------------------*
// ---- LIB.RS ---------------------*
// ---------------------------------*

// Limits for the drag-n-drop folder functionality to prevent someone dropping in half their C: drive into the app
const MAX_DEPTH: usize = 5;
const MAX_FILES: usize = 5000;

struct AppState {
    is_active: Arc<AtomicBool>,
    queue_tx: std::sync::mpsc::Sender<PathBuf>,
    queued_tracker: Arc<Mutex<HashSet<PathBuf>>>,
    config: Arc<Mutex<Arc<AppConfig>>>,
    /// Problems found while loading the config, shown in the Logs tab once the UI is listening.
    startup_warnings: Mutex<Vec<String>>,
}

#[derive(serde::Serialize)]
struct DLImage {
    id: String,
    url: String,
    ext: String,
}

/// Updates the background scanning state of the application.
/// If toggled to `true`, it immediately sweeps the input folder.
///
/// # Arguments
/// * `active` - Whether the background listener should be active.
/// * `state` - The managed Tauri application state containing the processing queues.
/// * `handle` - The Tauri AppHandle used for emitting queue events.
///
/// # Returns
/// * `usize` - How many files the sweep newly queued (0 if nothing was swept). The sweep only
///             lists the folder, so it runs synchronously and the frontend can rely on the count.
#[tauri::command]
fn set_scan_state(active: bool, state: tauri::State<'_, AppState>, handle: tauri::AppHandle) -> usize {
    let was_active = state.is_active.swap(active, Ordering::Relaxed);
    println!("Scan state set to: {}", active);

    if active && !was_active {
        return listener::run_sweep(&handle, &state.queue_tx, &state.queued_tracker);
    }

    0
}

/// Retrieves the current application configuration from memory.
///
/// # Arguments
/// * `state` - The managed Tauri application state.
///
/// # Returns
/// * `Result<Arc<AppConfig>, String>` - A clone of the current configuration.
#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> Result<Arc<AppConfig>, String> {
    Ok(state.config.lock().unwrap().clone())
}

/// Saves a new configuration to disk and updates the active in-memory state.
///
/// # Arguments
/// * `config` - The updated `AppConfig` object from the frontend.
/// * `state` - The managed Tauri application state.
/// * `handle` - The Tauri app handle.
///
/// # Returns
/// * `Result<(), String>` - Success or an error string if disk writing fails.
#[tauri::command]
fn save_config(
    config: AppConfig, 
    state: tauri::State<'_, AppState>, 
    handle: tauri::AppHandle
) -> Result<(), String> {
    use tauri::Manager;
    use tauri_plugin_autostart::ManagerExt;
    
    // Using the secure App Config Directory
    let config_dir = handle.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;

    config::save(&config_dir.join("config.json"), &config)?;

    let run_on_boot = config.flags.get("runOnBoot").copied().unwrap_or(false);
    let autolaunch = handle.autolaunch();
    
    if run_on_boot {
        let _ = autolaunch.enable();
    } else {
        let _ = autolaunch.disable();
    }

    // Wrap the saved config in the PR's new Arc
    *state.config.lock().unwrap() = Arc::new(config);
    Ok(())
}

/// A hook called by the Vue frontend once the UI has fully mounted.
/// Triggers an initial sweep of the input folder to process any files left over from a previous session.
///
/// # Arguments
/// * `state` - The managed Tauri application state.
/// * `handle` - The Tauri AppHandle.
#[tauri::command]
fn frontend_ready(state: tauri::State<'_, AppState>, handle: tauri::AppHandle) {
    println!("Frontend is ready!");

    for warning in state.startup_warnings.lock().unwrap().drain(..) {
        let _ = handle.emit("warn", warning);
    }

    if state.is_active.load(Ordering::Relaxed) {
        let tx = state.queue_tx.clone();
        let tracker = state.queued_tracker.clone();
        std::thread::spawn(move || {
            println!("Triggering initial sweep...");
            listener::run_sweep(&handle, &tx, &tracker);
        });
    }
}

/// Securely saves API credentials to the host operating system's native credential manager.
/// Automatically triggers a queue sweep in case files were waiting for credentials to process.
///
/// # Arguments
/// * `service` - The name of the Booru service (e.g., "danbooru").
/// * `key` - The JSON-stringified credential payload containing the username and API key.
/// * `state` - The managed Tauri application state.
/// * `handle` - The Tauri AppHandle.
///
/// # Returns
/// * `Result<(), String>` - Success or an error if the native vault access fails.
#[tauri::command]
fn save_credential(
    service: &str,
    key: &str,
    state: tauri::State<'_, AppState>,
    handle: tauri::AppHandle,
) -> Result<(), String> {
    let entry =
        keyring_core::Entry::new("saucebottle_vault", service).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())?;

    println!("Saved {} credentials.", service);

    if state.is_active.load(Ordering::Relaxed) {
        let tx = state.queue_tx.clone();
        let tracker = state.queued_tracker.clone();

        std::thread::spawn(move || {
            println!("Triggering sweep...");
            listener::run_sweep(&handle, &tx, &tracker);
        });
    }

    Ok(())
}

/// Retrieves a saved credential payload from the native OS credential manager.
///
/// # Arguments
/// * `service` - The name of the service.
///
/// # Returns
/// * `Result<String, String>` - The JSON credential string; empty string if not found.
#[tauri::command]
fn get_credential(service: &str) -> Result<String, String> {
    let entry = Entry::new("saucebottle_vault", service).map_err(|e| e.to_string())?;
    Ok(entry.get_password().unwrap_or_default())
}

/// Checks if a given file path has a supported image extension.
///
/// # Arguments
/// * `path` - The path to the file.
///
/// # Returns
/// * `bool` - `true` if the file has a valid image extension.
fn is_valid_image(path: &Path) -> bool {
    let valid_exts = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff", "jfif", "ico", "tga", "avif", "heic", "heif"];
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return valid_exts.contains(&ext.to_lowercase().as_str());
    }

    false
}

/// Deletes a saved credential from the native OS credential manager.
///
/// # Arguments
/// * `service` - The name of the service to delete.
///
/// # Returns
/// * `Result<(), String>` - Always returns Ok, even if the credential didn't exist.
#[tauri::command]
fn delete_credential(service: &str) -> Result<(), String> {
    if let Ok(entry) = keyring_core::Entry::new("saucebottle_vault", service) {
        let _ = entry.delete_credential();
    }

    Ok(())
}

/// SauceBottle's own folders, which a drop must not blindly import from (canonicalized paths).
struct OwnFolders {
    /// The input folder. Importing from it into itself would feed on its own copies.
    input: Option<PathBuf>,
    /// The results folder. Reached as a *subfolder* of a drop (e.g. dropping "SauceBottle" or
    /// "Pictures"), it would re-import the whole sorted library.
    results: Option<PathBuf>,
}

/// Running totals of a drag-and-drop import.
#[derive(Default)]
struct ImportStats {
    imported: usize,
    skipped_own_folders: bool,
}

/// Recursively traverses a dragged-and-dropped directory, pulling all valid images
/// out of subfolders and placing them directly into the target input directory.
/// Renames files automatically to avoid collisions.
///
/// # Arguments
/// * `path` - The current file or folder path being inspected.
/// * `target_dir` - The destination directory where files should be moved.
/// * `depth` - The current recursion depth (to countermeasure faulty inputs).
/// * `own_folders` - SauceBottle's own folders, which are skipped (see [`OwnFolders`]).
/// * `stats` - Running totals: files imported, and whether any of our own folders were skipped.
///
/// # Returns
/// * `Result<(), String>` - Success, or an error if safety limits are breached.
fn import_to_input_recursive(
    path: &Path,
    target_dir: &Path,
    depth: usize,
    own_folders: &OwnFolders,
    stats: &mut ImportStats,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "Folder hierarchy too deep! Stopped at depth {}.",
            MAX_DEPTH
        ));
    }

    if stats.imported >= MAX_FILES {
        return Err(format!(
            "Safety limit reached: Cannot import more than {} files at once.",
            MAX_FILES
        ));
    }

    // Files below depth 0 sit inside an already-checked folder, so only folders and the
    // dropped items themselves need the (comparatively slow) canonicalization
    if (depth == 0 || path.is_dir())
        && let Ok(canonical) = fs::canonicalize(path)
    {
        let in_input = own_folders.input.as_ref().is_some_and(|input| canonical.starts_with(input));
        let is_nested_results = depth > 0 && own_folders.results.as_ref() == Some(&canonical);

        if in_input || is_nested_results {
            stats.skipped_own_folders = true;
            return Ok(());
        }
    }

    if path.is_file() {
        if is_valid_image(path)
            && let Some(name) = path.file_name()
        {
            let mut dest = target_dir.join(name);

            let mut counter = 0;
            let file_stem = dest
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let extension = dest
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            while dest.exists() {
                counter += 1;
                dest = target_dir.join(format!("{}_import{}.{}", file_stem, counter, extension));
            }

            // Dropped files are moved (without ever leaving a half-copied duplicate behind),
            // files inside dropped folders are copied
            let moved = if depth == 0 {
                processor::atomic_move(path, &dest).is_ok()
            } else {
                fs::copy(path, &dest).is_ok()
            };

            if moved {
                stats.imported += 1;
            }
        }
    } else if path.is_dir() {
        if path.parent().is_none() {
            return Err("For safety, dropping an entire root drive is not allowed.".to_string());
        }

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                import_to_input_recursive(&entry.path(), target_dir, depth + 1, own_folders, stats)?;
            }
        }
    }
    Ok(())
}

/// Tauri command invoked when the user drags and drops files/folders onto the frontend window.
///
/// # Arguments
/// * `paths` - A list of absolute file paths dropped by the user.
/// * `state` - The managed Tauri application state (for the configured results folder).
/// * `handle` - The Tauri AppHandle for path resolution.
///
/// # Returns
/// * `Result<String, String>` - A success message with the file count, or an error.
#[tauri::command]
fn process_dropped_files(
    paths: Vec<String>,
    state: tauri::State<'_, AppState>,
    handle: tauri::AppHandle,
) -> Result<String, String> {
    let docs_dir = handle.path().picture_dir().expect("Path resolution failed");
    let saucebottle_dir = docs_dir.join("SauceBottle");
    let input_dir = saucebottle_dir.join("input");

    fs::create_dir_all(&input_dir).map_err(|e| e.to_string())?;

    let config = state.config.lock().unwrap().clone();
    let results_dir = if config.output_folder.trim().is_empty() {
        saucebottle_dir.join("results")
    } else {
        PathBuf::from(&config.output_folder)
    };

    // A folder that doesn't exist can't contain anything to skip, so `None` is fine there
    let own_folders = OwnFolders {
        input: fs::canonicalize(&input_dir).ok(),
        results: fs::canonicalize(&results_dir).ok(),
    };

    let mut stats = ImportStats::default();
    for p in paths {
        import_to_input_recursive(Path::new(&p), &input_dir, 0, &own_folders, &mut stats)?;
    }

    let mut message = format!("Successfully queued {} images.", stats.imported);
    if stats.skipped_own_folders {
        message.push_str(" SauceBottle's own input/results folders were skipped.");
    }
    Ok(message)
}

/// Checks the `input` directory and returns the total count of valid image files waiting to be processed.
/// (Used to populate the offline/paused queue badge on the welcome screen.)
///
/// # Arguments
/// * `handle` - The Tauri AppHandle.
///
/// # Returns
/// * `Result<usize, String>` - The number of valid images found.
#[tauri::command]
fn check_input_folder(handle: tauri::AppHandle) -> Result<usize, String> {
    let docs_dir = handle.path().picture_dir().expect("Path resolution failed");
    let saucebottle_dir = docs_dir.join("SauceBottle");
    let input_dir = saucebottle_dir.join("input");

    // Count how many files in the folder are valid images
    let count = std::fs::read_dir(input_dir)
        .map(|dir| dir.flatten().filter(|e| is_valid_image(&e.path())).count())
        .unwrap_or(0);

    Ok(count)
}

/// Fetches a raw JSON page of image posts from a service for the batch downloader tab.
///
/// # Arguments
/// * `service` - The Booru service to query.
/// * `tags` - Comma-separated tags to search for.
/// * `page` - The pagination index.
///
/// # Returns
/// * `Result<Vec<DLImage>, String>` - A list of lightweight image objects ready for download.
#[tauri::command]
async fn fetch_booru_page(
    service: String,
    tags: String,
    page: u32,
) -> Result<Vec<DLImage>, String> {
    let mut username = String::new();
    let mut api_key = String::new();

    // Yande.re doesn't require auth for standard queries so we can use it as default (wohoo!)
    if service != "Yande.re"
        && let Ok(entry) = keyring_core::Entry::new("saucebottle_vault", &service.to_lowercase())
        && let Ok(secret_string) = entry.get_password()
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&secret_string)
    {
        username = parsed["username"].as_str().unwrap_or("").to_string();
        api_key = parsed["apiKey"].as_str().unwrap_or("").to_string();
    }

    // Correctly format tags (e.g. "blue archive, official art" -> "blue_archive+official_art").
    // Each tag is URL-encoded on its own, so characters like '&', '#' or '+' inside a tag can't
    // break the query - only the '+' separators between tags stay as they are.
    let formatted_tags = tags
        .split(',')
        .map(|s| s.trim().replace(" ", "_"))
        .filter(|s| !s.is_empty())
        .map(|s| api::url_encode(&s))
        .collect::<Vec<_>>()
        .join("+");

    // Credentials go into the query string too, so they need the same treatment
    let encoded_username = api::url_encode(&username);
    let encoded_api_key = api::url_encode(&api_key);

    // Pages are 1-based; a 0 would underflow Gelbooru's 0-based `pid` below
    let page = page.max(1);

    // URL magic
    let url = match service.as_str() {
        "Danbooru" => format!(
            "https://danbooru.donmai.us/posts.json?tags={}&login={}&api_key={}&page={}",
            formatted_tags, encoded_username, encoded_api_key, page
        ),
        "Gelbooru" => format!(
            "https://gelbooru.com/index.php?page=dapi&s=post&q=index&tags={}&user_id={}&api_key={}&pid={}&json=1&limit=42",
            formatted_tags,
            encoded_username,
            encoded_api_key,
            page - 1
        ),
        "Yande.re" => format!(
            "https://yande.re/post.json?tags={}&page={}",
            formatted_tags, page
        ),
        _ => return Err("Unknown service".to_string()),
    };

    let user_agent = if username.is_empty() {
        "SauceBottle/1.0".to_string()
    } else {
        format!("Booru user {}", username)
    };

    // Errors go through `describe_http_error`, since these URLs contain the user's API key
    let res = api::http_client()
        .get(&url)
        .header("User-Agent", user_agent)
        .timeout(api::API_TIMEOUT)
        .send()
        .await
        .map_err(api::describe_http_error)?;

    if !res.status().is_success() {
        return Err(format!("{} returned HTTP {}", service, res.status()));
    }

    let json: serde_json::Value = res.json().await.map_err(api::describe_http_error)?;

    let mut images = Vec::new();
    let items = if service == "Gelbooru" {
        json["post"].as_array()
    } else {
        json.as_array()
    };

    if let Some(arr) = items {
        for item in arr {
            if let Some(file_url) = item["file_url"].as_str() {
                let id = item["id"]
                    .as_i64()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "0".to_string());
                let ext = item["file_ext"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        file_url.split('.').next_back().unwrap_or("jpg").to_string()
                    });

                images.push(DLImage {
                    id,
                    url: file_url.to_string(),
                    ext,
                });
            }
        }
    }
    Ok(images)
}

/// Downloads a file directly from a URL to the configured downloads folder.
///
/// # Arguments
/// * `url` - The direct link to the image file.
/// * `filename` - The name the file should be saved as.
/// * `state` - The managed Tauri application state.
/// * `handle` - The Tauri app handle for path resolution.
///
/// # Returns
/// * `Result<(), String>` - Success, or an error if the network request or disk write fails.
#[tauri::command]
async fn download_image(
    url: String,
    filename: String,
    state: tauri::State<'_, AppState>,
    handle: tauri::AppHandle,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();

    let mut base_dir = PathBuf::from(&config.output_folder);
    if base_dir.as_os_str().is_empty() {
        let pic_dir = handle
            .path()
            .picture_dir()
            .map_err(|_| "Failed to resolve Pictures directory")?;
        base_dir = pic_dir.join("SauceBottle").join("results");
    }

    // Use user setting or fallback to .downloads
    let dl_folder_name = if config.downloads_folder.trim().is_empty() {
        ".downloads"
    } else {
        &config.downloads_folder
    };

    let dl_dir = base_dir.join(dl_folder_name);
    std::fs::create_dir_all(&dl_dir).map_err(|e| e.to_string())?;
    let dest = dl_dir.join(filename);

    // No overall timeout here (images can be large) - the client's read timeout catches stalls
    let res = api::http_client()
        .get(&url)
        .send()
        .await
        .map_err(api::describe_http_error)?;

    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status()));
    }

    // Some image hosts answer blocked/expired links with "200 OK" and an HTML page
    let is_html = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.trim_start().to_ascii_lowercase().starts_with("text/html"));
    if is_html {
        return Err("The server returned a web page instead of the image.".to_string());
    }

    let bytes = res.bytes().await.map_err(api::describe_http_error)?;
    std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;

    Ok(())
}

/// Attempts to spawn the native OS file explorer (Windows Explorer, Finder, or Linux File Manager)
/// pointing directly to the requested application directory.
///
/// # Arguments
/// * `folder_target` - What folder we save to (either "results" or "downloads").
/// * `state` - The managed Tauri application state.
/// * `handle` - The Tauri AppHandle for path resolution.
///
/// # Returns
/// * `Result<(), String>` - Success, or an error if it fails to spawn.
#[tauri::command]
fn open_system_folder(
    folder_target: String,
    state: tauri::State<'_, AppState>,
    handle: tauri::AppHandle,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();

    let mut target_dir;

    // Check if it's a known UI keyword, otherwise treat it as a raw absolute path
    if folder_target == "results" || folder_target == "downloads" {
        target_dir = std::path::PathBuf::from(&config.output_folder);
        
        // If the user hasn't set a custom folder, use Pictures/SauceBottle/results as fallback
        if target_dir.as_os_str().is_empty() {
            let pic_dir = handle
                .path()
                .picture_dir()
                .map_err(|_| "Failed to resolve OS Pictures directory".to_string())?;

            target_dir = pic_dir.join("SauceBottle").join("results");
        }

        // Append the downloads folder if requested
        if folder_target == "downloads" {
            let dl_folder = if config.downloads_folder.trim().is_empty() {
                ".downloads"
            } else {
                &config.downloads_folder
            };
            target_dir = target_dir.join(dl_folder);
        }
    } else {
        target_dir = std::path::PathBuf::from(folder_target);
    }

    // Ensure the directory actually exists so the OS doesn't throw an error
    let _ = std::fs::create_dir_all(&target_dir);

    let absolute_path = std::fs::canonicalize(&target_dir).unwrap_or(target_dir);
    let clean_path = absolute_path.to_string_lossy().replace("\\\\?\\", "");

    println!("Opening: {}", clean_path);

    // Trigger the file explorer (OS dependent)
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&clean_path)
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&clean_path)
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&clean_path)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// The main application entry point.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    keyring_core::set_default_store(
        windows_native_keyring_store::Store::new()
            .expect("Failed to init Windows Credential Manager"),
    );
    #[cfg(target_os = "macos")]
    keyring_core::set_default_store(
        apple_native_keyring_store::keychain::Store::new()
            .expect("Failed to init Apple Credential Manager"),
    );
    #[cfg(target_os = "linux")]
    keyring_core::set_default_store(
        dbus_secret_service_keyring_store::Store::new()
            .expect("Failed to init Linux Secret Service"),
    );

    let scan_active_flag = Arc::new(std::sync::atomic::AtomicBool::new(false)); 
    let live_config = Arc::new(Mutex::new(Arc::new(AppConfig::default())));
    let api_client = Arc::new(ApiClient::new(live_config.clone()));

    tauri::Builder::default()
        // Must be registered first. Two instances would both watch the input folder and fight over
        // the same files, so a second launch just brings the already running window to the front.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .invoke_handler(tauri::generate_handler![
            // [TODO] Command registering list from hell -- perhaps find a sleeker way to register them (or at least not here)
            set_scan_state,
            get_config,
            save_config,
            frontend_ready,
            save_credential,
            get_credential,
            delete_credential,
            process_dropped_files,
            check_input_folder,
            fetch_booru_page,
            download_image,
            open_system_folder
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .setup(move |app| {
            use tauri::Manager;
            use tauri_plugin_autostart::ManagerExt;
            
            let config_dir = app.path().app_config_dir().expect("Failed to resolve config dir");
            std::fs::create_dir_all(&config_dir).expect("Failed to create config dir");
            let config_path = config_dir.join("config.json");

            let config::LoadedConfig { config, warnings } = config::load(&config_path);
            for warning in &warnings {
                println!("Warning: {}", warning);
            }

            let is_permanently_scanning = config.flags.get("isPermanentScan").copied().unwrap_or(true);
            scan_active_flag.store(is_permanently_scanning, std::sync::atomic::Ordering::Relaxed);
            *live_config.lock().unwrap() = Arc::new(config.clone());

            let run_on_boot = config.flags.get("runOnBoot").copied().unwrap_or(false);
            let autolaunch = app.autolaunch();
            if run_on_boot {
                let _ = autolaunch.enable();
            } else {
                let _ = autolaunch.disable();
            }

            let start_minimized = config.flags.get("startMinimized").copied().unwrap_or(false);
            if !start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            
            let pic_dir = app.path().picture_dir().expect("Failed to resolve Pictures directory");
            let saucebottle_dir = pic_dir.join("SauceBottle");

            let input_dir = saucebottle_dir.join("input");
            let results_dir = saucebottle_dir.join("results");

            std::fs::create_dir_all(&input_dir).expect("Failed to create input directory");
            std::fs::create_dir_all(&results_dir).expect("Failed to create results directory");

            println!(
                "SauceBottle File System initialized at: {:?}",
                saucebottle_dir
            );

            let handle = app.handle().clone();
            let client = Arc::clone(&api_client);
            let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))
                .expect("Failed to load tray icon");

            let quit_item = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)
                .expect("Failed to create Exit menu item");
            let tray_menu =
                Menu::with_items(app, &[&quit_item]).expect("Failed to create tray menu");

            let _ = TrayIconBuilder::with_id("sauce_tray")
                .icon(icon)
                .tooltip("SauceBottle")
                .menu(&tray_menu)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        println!("Exiting SauceBottle...");
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app);

            let (queue_tx, queued_tracker) =
                listener::spawn_watcher(handle, client, scan_active_flag.clone());

            app.manage(AppState {
                is_active: scan_active_flag,
                queue_tx,
                queued_tracker,
                config: live_config,
                startup_warnings: Mutex::new(warnings),
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error while running Tauri application");
}
