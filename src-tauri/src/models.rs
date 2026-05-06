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

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
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
