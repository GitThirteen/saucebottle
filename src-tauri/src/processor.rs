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
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::ImageReader;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;

use crate::models::{AppConfig, BooruResponse, ImageInfo};

// ---------------------------------*
// ---- PROCESSOR.RS ---------------*
// ---------------------------------*

// 8MB IQDB limit
const MAX_FILE_SIZE: u64 = 8 * 1024 * 1024;

/// Strips illegal characters from a string so it can be safely used as a folder or file name.
///
/// # Arguments
/// * `name` - The raw string to sanitize.
///
/// # Returns
/// * `String` - The safe, cleaned string.
fn sanitize_filename(name: &str) -> String {
    name.replace(|c: char| "/\\?%*:|\"<>".contains(c), "")
        .replace("  ", " ")
        .trim()
        .to_string()
}

fn finalize_path(target_dir: &Path, file_stem: &str, extension: &str) -> PathBuf {
    // Build the base path
    let mut dest_path = target_dir.join(file_stem);
    dest_path.add_extension(extension);
    
    // Handle collisions
    let mut counter = 0;
    while dest_path.exists() {
        counter += 1;
        
        dest_path = target_dir.join(format!("{}_copy{}", file_stem, counter));
        dest_path.add_extension(extension);
    }

    dest_path
}
 
/// Opens an image file to quickly read its metadata headers without decoding the entire
/// pixel grid into memory. This extracts the exact dimensions, format, and file size.
///
/// # Arguments
/// * `path` - The path to the image file on disk.
///
/// # Returns
/// * `Result<ImageInfo, String>` - A populated metadata struct, or an error if the file is corrupt/unreadable.
pub fn process_image(path: &Path) -> Result<ImageInfo, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    let size_kb = metadata.len() / 1024;

    let reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| format!("Failed to read image headers: {}", e))?;

    let format = reader.format().ok_or("Unrecognized image format")?;
    let dimensions = reader.into_dimensions().map_err(|e| e.to_string())?;

    let clean_path = path.to_string_lossy().replace("\\\\?\\", "");

    Ok(ImageInfo {
        path: clean_path,
        filename: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        format: format!("{:?}", format),
        width: dimensions.0,
        height: dimensions.1,
        size_kb,
    })
}

/// Prepares the image byte payload for the IQDB API upload.
/// IQDB has strict limits: max. 8MB file size, max. 7500x7500 resolution, and requires specific formats.
/// If the image exceeds these limits, this function dynamically scales, converts, or compresses
/// the image into a JPEG buffer entirely in memory, based on user configuration.
///
/// # Arguments
/// * `path` - The path to the original image.
/// * `info` - The metadata previously extracted by `process_image`.
/// * `config` - The user configuration dictating if resizing/compressing is permitted.
///
/// # Returns
/// * `Result<Vec<u8>, String>` - The raw bytes ready to be uploaded via HTTP multipart form.
pub fn get_iqdb_payload(
    path: &Path,
    info: &ImageInfo,
    config: &AppConfig,
) -> Result<Vec<u8>, String> {
    let size_bytes = info.size_kb * 1024;

    // 1. Read our user flags (default to true if missing for safety)
    let allow_shrinking = config.flags.get("allowShrinking").copied().unwrap_or(true);
    let allow_resizing = config.flags.get("allowResizing").copied().unwrap_or(true);
    let allow_conversion = config
        .flags
        .get("allowImageConversion")
        .copied()
        .unwrap_or(true);

    // 2. Identify if it needs conversion (IQDB natively accepts JPEG, PNG, GIF)
    let needs_conversion = matches!(
        info.format.to_lowercase().as_str(),
        "webp" | "bmp" | "tiff" | "ico" | "tga"
    );

    // 3. Identify if it breaks IQDB's hard 7500x7500 pixel limit
    let needs_resize = info.width > 7500 || info.height > 7500;

    // 4. If everything is within limits and compatible, return raw bytes
    if size_bytes <= MAX_FILE_SIZE && !needs_conversion && !needs_resize {
        return fs::read(path).map_err(|e| e.to_string());
    }

    // --- SAFETY CHECKS ---
    if needs_resize && !allow_resizing {
        return Err(format!(
            "Dimensions ({}x{}) exceed IQDB's 7500px limit, but image resizing is disabled.",
            info.width, info.height
        ));
    }
    if needs_conversion && !allow_conversion {
        return Err(format!(
            "Format '{}' requires conversion to JPEG for IQDB, but conversion is disabled.",
            info.format
        ));
    }
    if size_bytes > MAX_FILE_SIZE && !allow_shrinking {
        return Err("File exceeds IQDB's 8MB limit, and image shrinking is disabled.".to_string());
    }

    // --- PROCESSING PIPELINE ---
    let mut img = ImageReader::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to read image headers: {}", e))?
        .decode()
        .map_err(|e| format!("Failed to decode image for processing: {}", e))?;

    // 5. Resolve Dimensions
    if needs_resize {
        let scale = 7500.0 / f64::max(img.width() as f64, img.height() as f64);
        let new_w = (img.width() as f64 * scale) as u32;
        let new_h = (img.height() as f64 * scale) as u32;
        img = img.resize_exact(new_w, new_h, FilterType::Triangle);
    }

    // 6. Resolve Size and Format
    let mut buffer = Vec::new();
    let mut scale_factor = 1.0;

    loop {
        // Clone if scale is 1.0, otherwise shrink
        let resized: image::DynamicImage = if scale_factor < 1.0 {
            let new_w = (img.width() as f64 * scale_factor) as u32;
            let new_h = (img.height() as f64 * scale_factor) as u32;
            img.resize_exact(new_w, new_h, FilterType::Triangle)
        } else {
            img.clone()
        };

        buffer.clear();
        let mut cursor = Cursor::new(&mut buffer);

        // Encoding to JPEG solves both format conversion and file size shrinking
        let mut encoder = JpegEncoder::new_with_quality(&mut cursor, 80);
        encoder.encode_image(&resized).map_err(|e| e.to_string())?;

        if (buffer.len() as u64) <= MAX_FILE_SIZE {
            return Ok(buffer);
        }

        scale_factor *= 0.85; // Step down by 15% each iteration

        if scale_factor < 0.2 {
            return Err("Failed to compress image below the 8MB limit.".to_string());
        }
    }
}

/// Standard `fs::rename` fails if the source and destination are on different disk partitions/drives.
/// This attempts the fast rename first, and gracefully falls back to a full copy + delete if required.
///
/// # Arguments
/// * `from` - Current file path.
/// * `to` - Target file path.
///
/// # Returns
/// * `Result<(), String>` - Success, or a detailed error if both methods fail.
fn atomic_move(from: &Path, to: &Path) -> Result<(), String> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }

    fs::copy(from, to).map_err(|e| format!("Copy failed: {}", e))?;
    fs::remove_file(from).map_err(|e| format!("Cleanup failed: {}", e))?;
    Ok(())
}

/// Parses the user's custom folder hierarchy settings, generates the final directory structure,
/// applies naming/duplicate rules, and moves the processed image to its permanent home.
///
/// # Arguments
/// * `source_path` - The current location of the file.
/// * `char_data` - The fetched Booru metadata used to name the folders/file.
/// * `extension` - The file extension to use.
/// * `config` - The user's application configuration.
/// * `payload` - Optional modified image bytes (if the user chose to save the compressed version).
///
/// # Returns
/// * `Result<PathBuf, String>` - The absolute final path where the file was saved.
pub fn move_to_results(
    source_path: &Path,
    char_data: &BooruResponse,
    extension: &str,
    config: &AppConfig,
    payload: Option<&[u8]>,
    default_base_dir: &Path,
) -> Result<PathBuf, String> {
    // 1. Resolve base results directory (Default: ./results)
    let mut base_dir = PathBuf::from(&config.output_folder);
    if base_dir.as_os_str().is_empty() {
        base_dir = default_base_dir.to_path_buf();
    }

    // 2. Build Hierarchy
    for block in &config.active_hierarchy {
        let folder_name = match block.name.as_str() {
            "Fandom" => &char_data.fandom,
            "Character" => &char_data.name,
            "Artist" => &char_data.artist,
            "Year" => &char_data.year,
            "Rating (SFW/NSFW)" => &char_data.rating,
            _ => "Misc",
        };

        let safe_folder_name = sanitize_filename(folder_name);
        base_dir = base_dir.join(safe_folder_name);
    }
    fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;

    // 3. Filename Logic
    let service_prefix = char_data.service.chars().next().unwrap_or('U');
    let final_filename = match config.rename_behavior.as_str() {
        "original" => source_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        "random_id" => uuid::Uuid::new_v4().to_string(),
        _ => format!("{}{}", service_prefix, char_data.id),
    };

    // 4. Handle duplicate entries
    let mut dest_path = base_dir.join(format!("{}.{}", final_filename, extension));
    if dest_path.exists() {
        let list_dupes = config.flags.get("listDupes").copied().unwrap_or(true);
        if list_dupes {
            println!(
                "Duplicate detected: {} | Applying behavior: {}",
                final_filename, config.duplicate_behavior
            );
        }

        let mut target_dir = base_dir.clone();

        match config.duplicate_behavior.as_str() {
            "delete" => {
                let _ = fs::remove_file(source_path);
                return Ok(dest_path);
            }
            "move_folder_deep" => {
                target_dir = base_dir.join(".duplicates");
                fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
            }
            "move_folder_root" => {
                let mut dup_root = PathBuf::from(&config.output_folder);
                if dup_root.as_os_str().is_empty() {
                    dup_root = default_base_dir.to_path_buf();
                }

                target_dir = dup_root.join(".duplicates");
                fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
            }
            _ => {}
        }

        dest_path = finalize_path(&target_dir, &final_filename, extension);
    }

    let apply_mods = config
        .flags
        .get("applyModsToSaved")
        .copied()
        .unwrap_or(false);

    if apply_mods && let Some(payload) = payload {
        // Write modified buffer directly to new destination & delete original
        fs::write(&dest_path, payload)
            .map_err(|e| format!("Failed to write modified file: {}", e))?;
        let _ = fs::remove_file(source_path);
    } else {
        // Perform a regular move
        atomic_move(source_path, &dest_path)?;
    }

    Ok(dest_path)
}

pub fn move_to_invalid(
    source_path: &Path,
    config: &AppConfig,
    default_base_dir: &Path,
) -> Result<(), String> {
    let mut base_dir = PathBuf::from(&config.output_folder);
    if base_dir.as_os_str().is_empty() {
        base_dir = default_base_dir.to_path_buf();
    }

    let folder_name = if config.invalid_folder.trim().is_empty() {
        ".invalid"
    } else {
        config.invalid_folder.trim()
    };

    base_dir = base_dir.join(folder_name);
    fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;

    let file_stem = source_path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = source_path.extension().unwrap_or_default().to_string_lossy();
    
    let dest_path = finalize_path(&base_dir, &file_stem, &extension);
    atomic_move(source_path, &dest_path)
}
