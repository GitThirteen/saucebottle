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

use serde_json::{Map, Value};

use crate::models::AppConfig;

// ---------------------------------*
// ---- CONFIG.RS ------------------*
// ---------------------------------*

/// Where a settings file that couldn't be read at all is kept, next to `config.json`.
const BROKEN_CONFIG_NAME: &str = "config.broken.json";

/// The result of loading the config file from disk.
pub struct LoadedConfig {
    pub config: AppConfig,
    /// Messages for the user about settings that had to be reset (shown in the Logs tab).
    pub warnings: Vec<String>,
}

/// Loads the config file, recovering as much of it as possible.
/// A single invalid value only resets that one setting (or that one flag) to its default,
/// instead of silently throwing away every setting. A file that isn't valid JSON at all is
/// kept as `config.broken.json` before falling back to the defaults.
/// A missing file is created with the default settings.
///
/// # Arguments
/// * `path` - The path to `config.json`.
pub fn load(path: &Path) -> LoadedConfig {
    if !path.exists() {
        let config = AppConfig::default();
        if let Err(e) = save(path, &config) {
            println!("Error: Failed to write default config: {}", e);
        }
        return LoadedConfig { config, warnings: Vec::new() };
    }

    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            return LoadedConfig {
                config: AppConfig::default(),
                warnings: vec![format!(
                    "Could not read your settings file ({}). Default settings are used for this session.",
                    e
                )],
            };
        }
    };

    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(raw)) => {
            let (config, reset_keys) = merge_with_defaults(raw);
            let warnings = if reset_keys.is_empty() {
                Vec::new()
            } else {
                vec![format!(
                    "Some settings had invalid values and were reset to their defaults: {}",
                    reset_keys.join(", ")
                )]
            };
            LoadedConfig { config, warnings }
        }
        _ => {
            let backup = path.with_file_name(BROKEN_CONFIG_NAME);
            let kept = fs::copy(path, &backup).is_ok();
            let mut warning = "Your settings file was damaged and all settings were reset to their defaults.".to_string();
            if kept {
                warning.push_str(&format!(" The damaged file was kept as {}.", backup.display()));
            }
            LoadedConfig { config: AppConfig::default(), warnings: vec![warning] }
        }
    }
}

/// Lays the values from the file over the defaults, one top-level setting at a time,
/// skipping any value that doesn't fit its setting. Flags are checked individually.
///
/// # Returns
/// * `(AppConfig, Vec<String>)` - The recovered config and the names of the settings that were reset.
fn merge_with_defaults(raw: Map<String, Value>) -> (AppConfig, Vec<String>) {
    let defaults = AppConfig::default();
    let Ok(Value::Object(mut merged)) = serde_json::to_value(&defaults) else {
        return (defaults, Vec::new());
    };
    let mut reset_keys = Vec::new();

    for (key, value) in raw {
        // Flags are a plain map of booleans - keep every valid one, even if a neighbour is broken
        if key == "flags" {
            match (value, merged.get_mut("flags").and_then(Value::as_object_mut)) {
                (Value::Object(file_flags), Some(flags)) => {
                    for (flag, flag_value) in file_flags {
                        if flag_value.is_boolean() {
                            flags.insert(flag, flag_value);
                        } else {
                            reset_keys.push(format!("flags.{}", flag));
                        }
                    }
                }
                _ => reset_keys.push(key),
            }
            continue;
        }

        let mut candidate = merged.clone();
        candidate.insert(key.clone(), value);
        if serde_json::from_value::<AppConfig>(Value::Object(candidate.clone())).is_ok() {
            merged = candidate;
        } else {
            reset_keys.push(key);
        }
    }

    let config = serde_json::from_value(Value::Object(merged)).unwrap_or(defaults);
    (config, reset_keys)
}

/// Writes the config to disk without ever leaving a half-written file behind:
/// the data goes to a temporary file first, which then replaces `config.json` in one step.
///
/// # Arguments
/// * `path` - The path to `config.json`.
/// * `config` - The configuration to save.
pub fn save(path: &Path, config: &AppConfig) -> Result<(), String> {
    let data = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let temp_path = path.with_extension("json.tmp");

    fs::write(&temp_path, data).map_err(|e| format!("Failed to write settings: {}", e))?;
    fs::rename(&temp_path, path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to replace settings file: {}", e)
    })
}
