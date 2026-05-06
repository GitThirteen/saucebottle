mod api;
mod listener;
mod models;
mod processor;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keyring_core::Entry;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

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
    queue_tx: std::sync::mpsc::Sender<(PathBuf, bool)>,
    queued_tracker: Arc<Mutex<HashSet<PathBuf>>>,
    config: Arc<Mutex<AppConfig>>,
}

#[derive(serde::Serialize)]
struct DLImage {
    id: String,
    url: String,
    ext: String,
}

/// Updates the background scanning state of the application.
/// If toggled to `true`, it immediately triggers a sweep of the input folder.
///
/// # Arguments
/// * `active` - Whether the background listener should be active.
/// * `state` - The managed Tauri application state containing the processing queues.
/// * `handle` - The Tauri AppHandle used to spawn the listener thread.
#[tauri::command]
fn set_scan_state(active: bool, state: tauri::State<'_, AppState>, handle: tauri::AppHandle) {
    let was_active = state.is_active.swap(active, Ordering::Relaxed);
    println!("Scan state set to: {}", active);

    if active && !was_active {
        let tx = state.queue_tx.clone();
        let tracker = state.queued_tracker.clone();
        std::thread::spawn(move || {
            listener::run_sweep(&handle, &tx, &tracker);
        });
    }
}

/// Retrieves the current application configuration from memory.
///
/// # Arguments
/// * `state` - The managed Tauri application state.
///
/// # Returns
/// * `Result<AppConfig, String>` - A clone of the current configuration.
#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config.lock().unwrap().clone())
}

/// Saves a new configuration to disk and updates the active in-memory state.
///
/// # Arguments
/// * `config` - The updated `AppConfig` object from the frontend.
/// * `state` - The managed Tauri application state.
///
/// # Returns
/// * `Result<(), String>` - Success or an error string if disk writing fails.
#[tauri::command]
fn save_config(config: AppConfig, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let data = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write("./config.json", data).map_err(|e| e.to_string())?;

    *state.config.lock().unwrap() = config;
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
    let valid_exts = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff"];
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

/// Recursively traverses a dragged-and-dropped directory, pulling all valid images
/// out of subfolders and placing them directly into the target input directory.
/// Renames files automatically to avoid collisions.
///
/// # Arguments
/// * `path` - The current file or folder path being inspected.
/// * `target_dir` - The destination directory where files should be moved.
/// * `depth` - The current recursion depth (to countermeasure faulty inputs).
/// * `copied_count` - A mutable tracker for the number of files successfully processed.
///
/// # Returns
/// * `Result<(), String>` - Success, or an error if safety limits are breached.
fn import_to_input_recursive(
    path: &Path,
    target_dir: &Path,
    depth: usize,
    copied_count: &mut usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "Folder hierarchy too deep! Stopped at depth {}.",
            MAX_DEPTH
        ));
    }

    if *copied_count >= MAX_FILES {
        return Err(format!(
            "Safety limit reached: Cannot import more than {} files at once.",
            MAX_FILES
        ));
    }

    if path.is_file() {
        if is_valid_image(path) {
            if let Some(name) = path.file_name() {
                let mut dest = target_dir.join(name);

                let mut counter = 0;
                let file_stem = dest
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let extension = dest
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                while dest.exists() {
                    counter += 1;
                    dest =
                        target_dir.join(format!("{}_import{}.{}", file_stem, counter, extension));
                }

                let moved = if depth == 0 {
                    match fs::rename(path, &dest) {
                        Ok(_) => true,
                        Err(_) => fs::copy(path, &dest).is_ok() && fs::remove_file(path).is_ok(),
                    }
                } else {
                    fs::copy(path, &dest).is_ok()
                };

                if moved {
                    *copied_count += 1;
                }
            }
        }
    } else if path.is_dir() {
        if path.parent().is_none() {
            return Err("For safety, dropping an entire root drive is not allowed.".to_string());
        }

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                import_to_input_recursive(&entry.path(), target_dir, depth + 1, copied_count)?;
            }
        }
    }
    Ok(())
}

/// Tauri command invoked when the user drags and drops files/folders onto the frontend window.
///
/// # Arguments
/// * `paths` - A list of absolute file paths dropped by the user.
/// * `handle` - The Tauri AppHandle for path resolution.
///
/// # Returns
/// * `Result<String, String>` - A success message with the file count, or an error.
#[tauri::command]
fn process_dropped_files(paths: Vec<String>, handle: tauri::AppHandle) -> Result<String, String> {
    let app_dir = handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let input_dir = app_dir.join("input");

    fs::create_dir_all(&input_dir).map_err(|e| e.to_string())?;

    let mut copied_count = 0;

    for p in paths {
        if let Err(e) = import_to_input_recursive(Path::new(&p), &input_dir, 0, &mut copied_count) {
            return Err(e);
        }
    }

    Ok(format!("Successfully queued {} images.", copied_count))
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
    let app_dir = handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let input_dir = app_dir.join("input");

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
    if service != "Yande.re" {
        if let Ok(entry) = keyring_core::Entry::new("saucebottle_vault", &service.to_lowercase()) {
            if let Ok(secret_string) = entry.get_password() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&secret_string) {
                    username = parsed["username"].as_str().unwrap_or("").to_string();
                    api_key = parsed["apiKey"].as_str().unwrap_or("").to_string();
                }
            }
        }
    }

    // Correctly format tags (e.g. "blue archive, official art" -> "blue_archive+official_art")
    let formatted_tags = tags
        .split(',')
        .map(|s| s.trim().replace(" ", "_"))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("+");

    // URL magic
    let url = match service.as_str() {
        "Danbooru" => format!("https://danbooru.donmai.us/posts.json?tags={}&login={}&api_key={}&page={}", formatted_tags, username, api_key, page),
        "Gelbooru" => format!("https://gelbooru.com/index.php?page=dapi&s=post&q=index&tags={}&user_id={}&api_key={}&pid={}&json=1&limit=42", formatted_tags, username, api_key, page - 1),
        "Yande.re" => format!("https://yande.re/post.json?tags={}&page={}", formatted_tags, page),
        _ => return Err("Unknown service".to_string())
    };

    let user_agent = if username.is_empty() {
        "SauceBottle/1.0".to_string()
    } else {
        format!("Booru user {}", username)
    };

    let client = reqwest::Client::new();
    let res = client
        .get(&url)
        .header("User-Agent", user_agent)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;

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
                    .unwrap_or_else(|| file_url.split('.').last().unwrap_or("jpg").to_string());

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
///
/// # Returns
/// * `Result<(), String>` - Success, or an error if the network request or disk write fails.
#[tauri::command]
async fn download_image(
    url: String,
    filename: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();

    let mut base_dir = PathBuf::from(&config.output_folder);
    if base_dir.as_os_str().is_empty() {
        base_dir = PathBuf::from("./results");
    }

    // Use user setting or fallback to .downloads
    let dl_folder_name = if config.downloads_folder.trim().is_empty() {
        ".downloads".to_string()
    } else {
        config.downloads_folder.clone()
    };

    let dl_dir = base_dir.join(dl_folder_name);
    std::fs::create_dir_all(&dl_dir).map_err(|e| e.to_string())?;
    let dest = dl_dir.join(filename);

    let res = reqwest::get(&url).await.map_err(|e| e.to_string())?;
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;

    Ok(())
}

/// Attempts to spawn the native OS file explorer (Windows Explorer, Finder, or Linux File Manager)
/// pointing directly to the requested application directory.
///
/// # Arguments
/// * `folder_target` - What folder we save to (either "results" or "downloads").
/// * `state` - The managed Tauri application state.
///
/// # Returns
/// * `Result<(), String>` - Success, or an error if it fails to spawn.
#[tauri::command]
fn open_system_folder(
    folder_target: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();

    // Resolve the base results directory
    let mut target_dir = std::path::PathBuf::from(&config.output_folder);
    if target_dir.as_os_str().is_empty() {
        target_dir = std::path::PathBuf::from("./results");
    }

    // Append the downloads folder if requested
    if folder_target == "downloads" {
        let dl_folder = if config.downloads_folder.trim().is_empty() {
            ".downloads".to_string()
        } else {
            config.downloads_folder.clone()
        };
        target_dir = target_dir.join(dl_folder);
    }

    // Ensure the directory actually exists so the OS doesn't throw an error
    let _ = std::fs::create_dir_all(&target_dir);

    let absolute_path = std::fs::canonicalize(&target_dir).unwrap_or(target_dir);
    let clean_path = absolute_path.to_string_lossy().replace("\\\\?\\", "");

    println!("{}", clean_path);

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
        apple_native_keyring_store::Store::new().expect("Failed to init Apple Credential Manager"),
    );
    #[cfg(target_os = "linux")]
    keyring_core::set_default_store(
        dbus_secret_service_keyring_store::Store::new()
            .expect("Failed to init Linux Secret Service"),
    );

    let config_data = fs::read_to_string("./config.json").unwrap_or_else(|_| "{}".to_string());
    let config: AppConfig = serde_json::from_str(&config_data).unwrap_or_default();

    let is_permanently_scanning = config.flags.get("isPermanentScan").copied().unwrap_or(true);
    let scan_active_flag = Arc::new(std::sync::atomic::AtomicBool::new(is_permanently_scanning));

    let live_config = Arc::new(Mutex::new(config));
    let api_client = Arc::new(ApiClient::new(live_config.clone()));

    tauri::Builder::default()
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
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                window.hide().unwrap();
                api.prevent_close();
            }
            _ => {}
        })
        .setup(move |app| {
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
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app);

            let (queue_tx, queued_tracker) =
                listener::spawn_watcher(handle, client, scan_active_flag.clone());

            app.manage(AppState {
                is_active: scan_active_flag,
                queue_tx,
                queued_tracker,
                config: live_config,
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error while running Tauri application");
}
