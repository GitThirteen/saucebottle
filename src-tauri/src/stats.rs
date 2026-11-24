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

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::models::{AppConfig, BooruResponse};

// ---------------------------------*
// ---- STATS.RS -------------------*
// ---------------------------------*

/// Config flag that turns statistics on. Off by default.
pub const STATS_FLAG: &str = "enableStats";

/// Returns `true` if the user opted in to statistics.
pub fn is_enabled(config: &AppConfig) -> bool {
    config.flags.get(STATS_FLAG).copied().unwrap_or(false)
}

/// Everything that is counted, as stored in `stats.json`.
/// Only totals and counters per name are kept, no file names or paths.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(default)]
struct Stats {
    sorted: u64,
    originals: u64,
    /// Images that could not be sorted (no match, below the threshold, ...).
    failed: u64,
    /// Sum of the match percentages of all sorted images. Divide by `sorted` for the average.
    confidence_sum: u64,
    characters: HashMap<String, u64>,
    fandoms: HashMap<String, u64>,
    artists: HashMap<String, u64>,
    services: HashMap<String, u64>,
    /// Size of all sorted images, in KB.
    size_kb: u64,
    /// Time spent on all sorted images, from the start of processing to the end of the move, in milliseconds.
    sort_millis: u64,
    /// Time saved compared to sorting by hand, in milliseconds. Per image that's the assumed manual
    /// time minus the time SauceBottle needed, but never below zero.
    saved_millis: u64,
    downloads: u64,
}

/// How long sorting one image by hand is assumed to take. Used for "time saved".
const MANUAL_SORT_MILLIS: u64 = 30_000;

/// Separates fandom and name in the character counter keys, so characters with the same name
/// in different fandoms are counted apart. Older files have plain names as keys.
const KEY_SEP: char = '\u{1f}';

/// A name together with how often it was sorted.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct TopEntry {
    pub name: String,
    /// Extra info to tell entries apart (a character's fandom).
    pub detail: Option<String>,
    pub count: u64,
}

/// What the welcome screen shows.
#[derive(Debug, Serialize)]
pub struct StatsView {
    /// `false` if statistics are off. All other fields are empty then.
    pub enabled: bool,
    pub sorted: u64,
    pub originals: u64,
    pub failed: u64,
    pub top_character: Option<TopEntry>,
    pub top_fandom: Option<TopEntry>,
    pub top_artist: Option<TopEntry>,
    pub top_service: Option<TopEntry>,
    /// Average match percentage of all sorted images.
    pub average_confidence: Option<f64>,
    pub size_kb: u64,
    /// Average time spent per sorted image, in milliseconds.
    pub average_sort_millis: Option<u64>,
    /// Time saved compared to sorting by hand, in milliseconds.
    pub saved_millis: u64,
    pub downloads: u64,
}

impl StatsView {
    fn disabled() -> Self {
        Self::from_stats(&Stats::default(), false)
    }

    fn from_stats(stats: &Stats, enabled: bool) -> Self {
        Self {
            enabled,
            sorted: stats.sorted,
            originals: stats.originals,
            failed: stats.failed,
            top_character: top_entry(&stats.characters).map(split_character),
            top_fandom: top_entry(&stats.fandoms),
            top_artist: top_entry(&stats.artists),
            top_service: top_entry(&stats.services),
            average_confidence: (stats.sorted > 0).then(|| stats.confidence_sum as f64 / stats.sorted as f64),
            size_kb: stats.size_kb,
            average_sort_millis: (stats.sorted > 0).then(|| stats.sort_millis / stats.sorted),
            saved_millis: stats.saved_millis,
            downloads: stats.downloads,
        }
    }
}

/// The most frequent entry. On a tie, the name that comes first alphabetically wins, so the result doesn't flip around.
fn top_entry(counts: &HashMap<String, u64>) -> Option<TopEntry> {
    counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(name, count)| TopEntry { name: name.clone(), detail: None, count: *count })
}

/// Splits a character key into the character's name and its fandom.
fn split_character(entry: TopEntry) -> TopEntry {
    match entry.name.split_once(KEY_SEP) {
        Some((fandom, name)) => TopEntry {
            name: name.to_string(),
            detail: (!fandom.is_empty()).then(|| fandom.to_string()),
            count: entry.count,
        },
        None => entry,
    }
}

/// Placeholder names (nothing could be determined) aren't worth counting.
fn is_countable(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && !matches!(name, "Unknown" | "Original" | "0riginal")
}

/// Statistics in memory and in `stats.json`. Nothing is recorded unless the user turned them on.
pub struct StatsStore {
    path: PathBuf,
    data: Mutex<Stats>,
}

impl StatsStore {
    /// Loads `stats.json`. If it's missing or unreadable, the counters start at zero.
    pub fn load(path: PathBuf) -> Self {
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Stats>(&text).ok())
            .unwrap_or_default();
        Self { path, data: Mutex::new(data) }
    }

    /// Writes the counters to disk. A temp file is written first, so a half-written file can't be left behind.
    fn try_save(&self, data: &Stats) -> Result<(), String> {
        let temp_path = self.path.with_extension("json.tmp");
        let result = serde_json::to_string(data)
            .map_err(|e| e.to_string())
            .and_then(|text| fs::write(&temp_path, text).map_err(|e| e.to_string()))
            .and_then(|_| fs::rename(&temp_path, &self.path).map_err(|e| e.to_string()));

        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }

    /// Like `try_save`, but only logs a failure. A missed write must not break sorting.
    fn save(&self, data: &Stats) {
        if let Err(e) = self.try_save(data) {
            println!("Error: Could not save statistics: {}", e);
        }
    }

    /// Counts one successfully sorted image.
    ///
    /// # Arguments
    /// * `config` - The current settings. Nothing is recorded if statistics are off.
    /// * `result` - What was found for the image.
    /// * `size_kb` - Size of the image that was sorted.
    /// * `elapsed` - How long sorting the image took.
    pub fn record_sort(&self, config: &AppConfig, result: &BooruResponse, size_kb: u64, elapsed: Duration) {
        if !is_enabled(config) {
            return;
        }

        let mut data = self.data.lock().unwrap();
        data.sorted += 1;
        data.confidence_sum += u64::from(result.similarity);
        data.size_kb += size_kb;
        let elapsed_millis = elapsed.as_millis() as u64;
        data.sort_millis += elapsed_millis;
        data.saved_millis += MANUAL_SORT_MILLIS.saturating_sub(elapsed_millis);
        *data.services.entry(result.service.clone()).or_default() += 1;

        if result.is_original_work {
            data.originals += 1;
        } else {
            if is_countable(&result.fandom) {
                *data.fandoms.entry(result.fandom.trim().to_string()).or_default() += 1;
            }
            if is_countable(&result.name) {
                let fandom = if is_countable(&result.fandom) { result.fandom.trim() } else { "" };
                let key = format!("{}{}{}", fandom, KEY_SEP, result.name.trim());
                *data.characters.entry(key).or_default() += 1;
            }
        }
        if is_countable(&result.artist) {
            *data.artists.entry(result.artist.trim().to_string()).or_default() += 1;
        }

        self.save(&data);
    }

    /// Counts one image that could not be sorted.
    pub fn record_failure(&self, config: &AppConfig) {
        if !is_enabled(config) {
            return;
        }

        let mut data = self.data.lock().unwrap();
        data.failed += 1;
        self.save(&data);
    }

    /// Counts one downloaded image (Download tab).
    pub fn record_download(&self, config: &AppConfig) {
        if !is_enabled(config) {
            return;
        }

        let mut data = self.data.lock().unwrap();
        data.downloads += 1;
        self.save(&data);
    }

    /// What to show on the welcome screen. Empty if statistics are off.
    pub fn view(&self, config: &AppConfig) -> StatsView {
        if !is_enabled(config) {
            return StatsView::disabled();
        }
        StatsView::from_stats(&self.data.lock().unwrap(), true)
    }

    /// Sets all counters back to zero and overwrites the saved data.
    pub fn reset(&self) -> Result<(), String> {
        let mut data = self.data.lock().unwrap();
        *data = Stats::default();
        self.try_save(&data).map_err(|e| format!("Could not reset the saved statistics: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(name: &str, fandom: &str) -> BooruResponse {
        BooruResponse {
            id: "1".into(),
            name: name.into(),
            fandom: fandom.into(),
            artist: "Someone".into(),
            rating: "SFW".into(),
            year: "2024".into(),
            service: "Danbooru".into(),
            similarity: 90,
            file_path: String::new(),
            is_original_work: false,
            source_file: String::new(),
        }
    }

    #[test]
    fn characters_with_the_same_name_are_counted_per_fandom() {
        let path = std::env::temp_dir().join(format!("saucebottle_stats_test_{}.json", std::process::id()));
        let store = StatsStore::load(path.clone());
        let mut config = AppConfig::default();
        config.flags.insert(STATS_FLAG.to_string(), true);

        store.record_sort(&config, &response("Rin", "Fate"), 10, Duration::from_millis(100));
        store.record_sort(&config, &response("Rin", "Vocaloid"), 10, Duration::from_millis(100));
        store.record_sort(&config, &response("Rin", "Vocaloid"), 10, Duration::from_millis(100));

        let view = store.view(&config);
        let top = view.top_character.unwrap();
        assert_eq!((top.name.as_str(), top.detail.as_deref(), top.count), ("Rin", Some("Vocaloid"), 2));
        assert_eq!(view.sorted, 3);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn time_saved_is_manual_time_minus_actual_time_and_never_negative() {
        let path = std::env::temp_dir().join(format!("saucebottle_stats_saved_{}.json", std::process::id()));
        let store = StatsStore::load(path.clone());
        let mut config = AppConfig::default();
        config.flags.insert(STATS_FLAG.to_string(), true);

        // 10 s needed saves 20 s. 45 s is slower than by hand, so it saves nothing.
        store.record_sort(&config, &response("A", "F"), 1, Duration::from_secs(10));
        store.record_sort(&config, &response("B", "F"), 1, Duration::from_secs(45));
        // Failures don't count as sorted or as saved time
        store.record_failure(&config);

        let view = store.view(&config);
        assert_eq!(view.saved_millis, 20_000);
        assert_eq!(view.sorted, 2);
        assert_eq!(view.average_sort_millis, Some(27_500));
        assert_eq!(view.failed, 1);

        let _ = fs::remove_file(path);
    }
}
