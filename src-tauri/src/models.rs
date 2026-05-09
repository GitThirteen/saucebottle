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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------*
// ---- MODELS.RS ------------------*
// ---------------------------------*

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BooruConfig {
    pub url: String,
    pub post_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HierarchyBlock {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub services: HashMap<String, BooruConfig>,
    pub flags: HashMap<String, bool>,

    // Directory Settings
    pub output_folder: String,
    pub original_folder: String,
    pub invalid_folder: String,
    pub blacklist: String,

    // File Handling
    pub rename_behavior: String,
    pub duplicate_behavior: String,
    pub confidence_threshold: u8,

    // Folder Hierarchy
    pub active_hierarchy: Vec<HierarchyBlock>,
    pub available_blocks: Vec<HierarchyBlock>,

    #[serde(default)]
    pub downloads_folder: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut services = HashMap::new();
        
        services.insert("Gelbooru".to_string(), BooruConfig {
            url: "https://gelbooru.com/index.php?page=dapi&s=post&q=index&id=${ID}&json=1&user_id=${USERNAME}&api_key=${API_KEY}".to_string(),
            post_url: Some("https://gelbooru.com/index.php?page=dapi&s=tag&q=index&names=${TAGS}&json=1&user_id=${USERNAME}&api_key=${API_KEY}".to_string()),
        });
        
        services.insert("Yande.re".to_string(), BooruConfig {
            url: "https://yande.re/post.json?tags=id:${ID}".to_string(),
            post_url: None,
        });
        
        services.insert("Danbooru".to_string(), BooruConfig {
            url: "https://danbooru.donmai.us/posts/${ID}.json?login=${USERNAME}&api_key=${API_KEY}".to_string(),
            post_url: None,
        });

        let mut flags = HashMap::new();
        flags.insert("applyModsToSaved".to_string(), false);
        flags.insert("runOnBoot".to_string(), false);
        flags.insert("listDupes".to_string(), true);
        flags.insert("allowImageConversion".to_string(), true);
        flags.insert("allowShrinking".to_string(), true);
        flags.insert("renameFoundImages".to_string(), true);
        flags.insert("renameInvalidImages".to_string(), true);
        flags.insert("isPermanentScan".to_string(), true);
        flags.insert("allowResizing".to_string(), true);
        flags.insert("autoUpdateEnabled".to_string(), true);

        let active_hierarchy = vec![
            HierarchyBlock { id: 1, name: "Fandom".to_string() },
            HierarchyBlock { id: 2, name: "Character".to_string() },
        ];

        let available_blocks = vec![
            HierarchyBlock { id: 3, name: "Artist".to_string() },
            HierarchyBlock { id: 5, name: "Rating (SFW/NSFW)".to_string() },
            HierarchyBlock { id: 4, name: "Year".to_string() },
        ];

        Self {
            services,
            flags,
            output_folder: "".to_string(),
            original_folder: ".original".to_string(),
            invalid_folder: ".invalid".to_string(),
            blacklist: "".to_string(),
            rename_behavior: "site_id".to_string(),
            duplicate_behavior: "rename_copy".to_string(),
            confidence_threshold: 80,
            active_hierarchy,
            available_blocks,
            downloads_folder: ".downloads".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BooruResponse {
    pub id: String,
    pub name: String,
    pub fandom: String,
    pub artist: String,
    pub rating: String,
    pub year: String,
    pub service: String,
    pub similarity: u8,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub path: String,
    pub filename: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub size_kb: u64,
}