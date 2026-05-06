use std::sync::{Arc, LazyLock, Mutex};

use keyring_core::Entry;
use regex::Regex;
use reqwest::multipart;
use scraper::{Html, Selector};
use tauri::Emitter;

use crate::models::{AppConfig, BooruResponse};

// ---------------------------------*
// ---- API.RS ---------------------*
// ---------------------------------*

// Regex's to scrape similarity and year from IQDB
static SIMILARITY_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)%").unwrap());
static YEAR_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(19|20)\d{2}\b").unwrap());

// ---- Helpers --------------------*

/// Calculates the Levenshtein distance, also known as the edit distance, between two strings.
/// This algorithm determines the minimum number of single-character edits (insertions, deletions, substitutions)
/// to get from one word to another.
///
/// # Arguments:
/// * `a` - The first string (tag)
/// * `b` - The second string (tag)
///
/// # Returns:
/// * `usize` - The total number of edits required.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let mut dp = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        dp[i][0] = i;
    }
    for j in 0..=b.len() {
        dp[0][j] = j;
    }
    for (i, ca) in a.chars().enumerate() {
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            dp[i + 1][j + 1] = *[dp[i][j + 1] + 1, dp[i + 1][j] + 1, dp[i][j] + cost]
                .iter()
                .min()
                .unwrap();
        }
    }
    dp[a.len()][b.len()]
}

/// Evaluates a fetched image tag against the user's blacklist rules.
/// Emits warnings to the frontend for partial context matches or likely
/// typos based on a proportional Levenshtein distance check.
///
/// # Arguments
/// * `tag` - The raw tag fetched from the service in question.
/// * `blacklist` - A slice of blacklisted tag strings.
/// * `handle` - The Tauri AppHandle used to emit warning events to the UI.
///
/// # Returns
/// * `bool` - Returns `true` if the tag is an exact match and should be completely ignored, `false` otherwise.
fn process_blacklist_rules(tag: &str, blacklist: &[String], handle: &tauri::AppHandle) -> bool {
    let tag_lower = tag.to_lowercase();

    if blacklist.contains(&tag_lower) {
        return true;
    }

    for b in blacklist {
        if tag_lower.starts_with(&format!("{}_(", b)) {
            let msg = format!("Your blacklisted tag '{}' partially matched '{}'. Did you mean to blacklist the latter?", b, tag);
            let _ = handle.emit("warn", msg.clone());
        }

        let length_diff = (tag_lower.len() as isize - b.len() as isize).abs();

        // Determine how many typos we tolerate based on the blacklist tag's length
        let max_dist: i32 = match b.len() {
            0..=4 => 1,
            5..=9 => 2,
            _ => 3,
        };

        // [TODO] as isize... as usize... as isize... as usize...
        if length_diff <= max_dist as isize {
            let dist = levenshtein_distance(&tag_lower, b);

            if dist >= 1 && dist <= max_dist as usize {
                let msg = format!("Your blacklisted tag '{}' is very similar to the found image tag '{}'. Did you mean to blacklist the latter?", b, tag);
                let _ = handle.emit("warn", msg.clone());
            }
        }
    }

    false
}

/// Custom URL encodes a given string to safely pass it as a query parameter in HTTP requests.
///
/// # Arguments
/// * `input` - The raw string to encode.
///
/// # Returns
/// * `String` - The URL-encoded string.
fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

/// Cleans raw (booru) tags to make them visually appealing for folder and file names.
/// It strips away contextual suffixes enclosed in parentheses, replaces underscores
/// with spaces, and applies standard Title Case capitalization.
///
/// # Arguments
/// * `tag` - The raw string tag fetched from the service.
///
/// # Returns
/// * `String` - The cleaned, formatted tag (e.g., "blue_archive" becomes "Blue Archive").
fn clean_tag(tag: &str) -> String {
    if tag.is_empty() || tag == "Unknown" {
        return "Unknown".to_string();
    }

    let base_name = tag
        .split_once('(')
        .map(|(prefix, _)| prefix)
        .unwrap_or(tag)
        .trim_end_matches('_')
        .trim_end_matches(' ');

    let spaced = base_name.replace('_', " ");

    let clean_name = spaced
        .split_whitespace()
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().chain(c).collect(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ");

    clean_name.trim().to_string()
}

// ---- API Interactions --------------*

pub struct ApiClient {
    config: Arc<Mutex<AppConfig>>,
    client: reqwest::Client,
}

impl ApiClient {
    pub fn new(config: Arc<Mutex<AppConfig>>) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Retrieves a cloned reference to the active application configuration.
    ///
    /// # Returns
    /// * `Arc<Mutex<AppConfig>>` - A thread-safe config object clone.
    pub fn config(&self) -> Arc<Mutex<AppConfig>> {
        self.config.clone()
    }

    /// Checks the native OS credential manager to determine which services
    /// currently have valid credentials (username/id and API key) configured.
    ///
    /// # Returns
    /// * `Vec<String>` - A list of service names (e.g., "Danbooru", "Gelbooru") that are ready to be used.
    pub fn get_active_credentials(&self) -> Vec<String> {
        let mut active = Vec::new();
        for srv in ["Danbooru", "Yande.re", "Gelbooru"] {
            // [TODO] Do we really need Yande.re here? It says it's cool with being scraped.
            if srv == "Yande.re" {
                active.push(srv.to_string());
                continue;
            }

            if let Ok(entry) = keyring_core::Entry::new("saucebottle_vault", &srv.to_lowercase()) {
                if let Ok(secret_string) = entry.get_password() {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&secret_string) {
                        let has_u = parsed["username"]
                            .as_str()
                            .map(|s| !s.trim().is_empty())
                            .unwrap_or(false);
                        let has_k = parsed["apiKey"]
                            .as_str()
                            .map(|s| !s.trim().is_empty())
                            .unwrap_or(false);
                        if has_u && has_k {
                            active.push(srv.to_string());
                        }
                    }
                }
            }
        }
        active
    }

    /// Submits image bytes to IQDB to find the best matching source, and then automatically
    /// fetches the detailed tag metadata from the winning service.
    ///
    /// # Arguments
    /// * `image_bytes` - The raw file bytes of the image (compressed if necessary).
    /// * `active_services` - A list of permitted services to match against.
    /// * `handle` - The Tauri AppHandle used for logging and warnings.
    ///
    /// # Returns
    /// * `Result<BooruResponse, String>` - A populated response object containing character, artist, etc.,
    ///                                     or an error string if the fetch failed.
    pub async fn search_iqdb(
        &self,
        image_bytes: Vec<u8>,
        active_services: &[String],
        handle: &tauri::AppHandle,
    ) -> Result<BooruResponse, String> {
        let part = multipart::Part::bytes(image_bytes)
            .file_name("upload.jpg")
            .mime_str("image/jpeg")
            .map_err(|e| e.to_string())?;

        let mut form = multipart::Form::new().part("file", part);

        let all_iqdb_services = ["1", "2", "3", "4", "5", "6", "10", "11"];
        for id in all_iqdb_services {
            form = form.text("service[]", id.to_string());
        }

        // [TODO] Verify if header is cool
        let res = self.client.post("https://www.iqdb.org/")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .multipart(form)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let html_content = res.text().await.map_err(|e| e.to_string())?;
        let threshold = self.config.lock().unwrap().confidence_threshold;

        let (service, id, similarity) =
            self.parse_iqdb_html(&html_content, active_services, threshold)?;
        self.fetch_booru_details(&service, &id, similarity, handle)
            .await
    }

    /// Asks a specific Booru service (currently Danbooru, Gelbooru, or Yande.re) for metadata
    /// regarding a specific image post, applying blacklist checks and logic unique to each site.
    ///
    /// # Arguments
    /// * `service` - The name of the service.
    /// * `id` - The unique post ID (or MD5 hash in case of Gelbooru) of the image on that service.
    /// * `similarity` - The match confidence percentage returned by IQDB.
    /// * `handle` - The Tauri AppHandle for relaying warnings.
    ///
    /// # Returns
    /// * `Result<BooruResponse, String>` - The cleaned and compiled metadata for the image.
    async fn fetch_booru_details(
        &self,
        service: &str,
        id: &str,
        similarity: u8,
        handle: &tauri::AppHandle,
    ) -> Result<BooruResponse, String> {
        // [WARN] This entire function is an absolute abomination.
        // I *think* it currently works well enough, but this definitely needs a rework.
        // Some things someone might want to look at:
        // [TODO] Modularizing the individual components of this absolute monolith of a function into individual (reusable?) functions
        // [TODO] Reworking the if / else if / else if further at the bottom that handles each service individually (surely there must be a way to handle that better...)
        // [TODO] IQDB supports more services than just the current 3 *puts gun in mouth* (Traits???????)

        let live_cfg = self.config.lock().unwrap().clone();
        let cfg = live_cfg
            .services
            .get(service)
            .ok_or_else(|| format!("Service {} not found", service))?;

        let blacklist: Vec<String> = live_cfg
            .blacklist
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        let mut actual_username = String::new();
        let mut actual_api_key = String::new();

        if let Ok(entry) = Entry::new("saucebottle_vault", &service.to_lowercase()) {
            if let Ok(secret_string) = entry.get_password() {
                if let Ok(parsed_secrets) =
                    serde_json::from_str::<serde_json::Value>(&secret_string)
                {
                    if let Some(u) = parsed_secrets["username"].as_str() {
                        if !u.trim().is_empty() {
                            actual_username = u.to_string();
                        }
                    }
                    if let Some(k) = parsed_secrets["apiKey"].as_str() {
                        if !k.trim().is_empty() {
                            actual_api_key = k.to_string();
                        }
                    }
                }
            }
        }

        let mut api_url = cfg.url.clone();
        if service == "Gelbooru" && id.len() == 32 {
            api_url = api_url.replace("&id=${ID}", "&tags=md5:${ID}");
        }

        api_url = api_url
            .replace("${ID}", id)
            .replace("${USERNAME}", &actual_username)
            .replace("${API_KEY}", &actual_api_key);

        let user_agent = if actual_username.is_empty() {
            "SauceBottle/1.0".to_string()
        } else {
            format!("Booru user {}", actual_username)
        };

        let res = self
            .client
            .get(&api_url)
            .header("User-Agent", &user_agent)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;

        let mut character = String::from("Original");
        let mut fandom = String::from("Unknown");
        let mut artist = String::from("Unknown");
        let mut raw_rating = String::from("u");

        let created_at_str = match service {
            "Gelbooru" => json["post"][0]["created_at"].as_str().unwrap_or(""),
            "Danbooru" => json["created_at"].as_str().unwrap_or(""),
            _ => "",
        };

        let mut year = YEAR_REGEX
            .find(created_at_str)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        if service == "Danbooru" {
            let extract_first_valid = |json_key: &str, default: &str| -> String {
                if let Some(tags_str) = json[json_key].as_str() {
                    for tag in tags_str.split_whitespace() {
                        if process_blacklist_rules(tag, &blacklist, handle) {
                            continue;
                        }
                        return tag.to_string();
                    }
                }
                default.to_string()
            };

            character = extract_first_valid("tag_string_character", "Original");
            fandom = extract_first_valid("tag_string_copyright", "Unknown");
            artist = extract_first_valid("tag_string_artist", "Unknown");
            raw_rating = json["rating"].as_str().unwrap_or("u").to_string();
        } else if service == "Gelbooru" {
            let post_obj = &json["post"][0];
            raw_rating = post_obj["rating"]
                .as_str()
                .unwrap_or("u")
                .chars()
                .next()
                .unwrap_or('u')
                .to_string();
            let tag_string = post_obj["tags"].as_str().unwrap_or("").to_string();

            if let Some(post_url_template) = &cfg.post_url {
                let formatted_tags = tag_string
                    .split_whitespace()
                    .take(25)
                    .map(url_encode)
                    .collect::<Vec<String>>()
                    .join("+");

                let tag_url = post_url_template
                    .replace("${TAGS}", &formatted_tags)
                    .replace("${USERNAME}", &actual_username)
                    .replace("${API_KEY}", &actual_api_key);

                if let Ok(tag_res) = self
                    .client
                    .get(&tag_url)
                    .header("User-Agent", &user_agent)
                    .send()
                    .await
                {
                    if let Ok(raw_text) = tag_res.text().await {
                        if let Ok(tag_json) = serde_json::from_str::<serde_json::Value>(&raw_text) {
                            if let Some(arr) = tag_json["tag"].as_array().or(tag_json.as_array()) {
                                let filtered: Vec<_> = arr
                                    .iter()
                                    .filter(|t| {
                                        let name = t["name"].as_str().unwrap_or("");
                                        !process_blacklist_rules(name, &blacklist, handle)
                                    })
                                    .collect();

                                if let Some(c) = filtered
                                    .iter()
                                    .find(|t| t["type"] == 4)
                                    .and_then(|t| t["name"].as_str())
                                {
                                    character = c.to_string();
                                }
                                if let Some(f) = filtered
                                    .iter()
                                    .find(|t| t["type"] == 3)
                                    .and_then(|t| t["name"].as_str())
                                {
                                    fandom = f.to_string();
                                }
                                if let Some(a) = filtered
                                    .iter()
                                    .find(|t| t["type"] == 1)
                                    .and_then(|t| t["name"].as_str())
                                {
                                    artist = a.to_string();
                                }
                            }
                        }
                    }
                }
            }
        } else if service == "Yande.re" {
            let post_obj = &json[0];
            raw_rating = post_obj["rating"]
                .as_str()
                .unwrap_or("u")
                .chars()
                .next()
                .unwrap_or('u')
                .to_string();

            // Yande.re uses UNIX timestamps. 31556926 seconds = 1 year
            if let Some(ts) = post_obj["created_at"].as_i64() {
                year = (1970 + (ts / 31556926)).to_string();
            }

            // Scraping Yande.re's HTML directly
            let html_url = format!("https://yande.re/post/show/{}", id);
            if let Ok(html_res) = self
                .client
                .get(&html_url)
                .header("User-Agent", &user_agent)
                .send()
                .await
            {
                if let Ok(html_text) = html_res.text().await {
                    let document = Html::parse_document(&html_text);

                    // Helper to find the first non-blacklisted tag in a specific HTML category
                    let extract_yandere = |tag_class: &str, default: &str| -> String {
                        if let Ok(sel) = Selector::parse(&format!("li.{} a", tag_class)) {
                            for a in document.select(&sel) {
                                if let Some(href) = a.value().attr("href") {
                                    if href.starts_with("/post?tags=") {
                                        let tag_name = a.text().collect::<String>();

                                        if process_blacklist_rules(&tag_name, &blacklist, handle) {
                                            continue;
                                        }
                                        return tag_name;
                                    }
                                }
                            }
                        }
                        default.to_string()
                    };

                    character = extract_yandere("tag-type-character", "Original");
                    fandom = extract_yandere("tag-type-copyright", "Unknown");
                    artist = extract_yandere("tag-type-artist", "Unknown");
                }
            }
        }

        let rating = match raw_rating.as_str() {
            "s" | "g" => "SFW",
            "q" | "e" => "NSFW",
            _ => "Unknown",
        }
        .to_string();

        Ok(BooruResponse {
            id: id.to_string(),
            name: clean_tag(&character),
            fandom: clean_tag(&fandom),
            artist: clean_tag(&artist),
            rating,
            year,
            service: service.to_string(),
            similarity,
            file_path: String::new(),
        })
    }

    /// Parses the raw HTML response from IQDB to determine the best matching service result.
    ///
    /// # Arguments
    /// * `html` - The raw HTML response string from IQDB.
    /// * `valid_services` - A list of services the user has configured/allowed.
    /// * `threshold` - The minimum similarity percentage required to constitute a valid match.
    ///
    /// # Returns
    /// * `Result<(String, String, u8), String>` - A tuple containing `(Service Name, Post ID, Similarity)` if a successful match is found,
    ///                                            otherwise an error string.
    fn parse_iqdb_html(
        &self,
        html: &str,
        valid_services: &[String],
        threshold: u8,
    ) -> Result<(String, String, u8), String> {
        let document = Html::parse_document(html);
        let table_sel = Selector::parse("table").unwrap();
        let th_sel = Selector::parse("th").unwrap();
        let a_sel = Selector::parse("a").unwrap();

        let mut best_match: Option<(String, String, u8)> = None;
        let valid_lower: Vec<String> = valid_services.iter().map(|s| s.to_lowercase()).collect();

        for table in document.select(&table_sel) {
            if let Some(th) = table.select(&th_sel).next() {
                let header_text = th.text().collect::<Vec<_>>().join(" ").to_lowercase();

                if header_text.contains("match") {
                    let table_text = table.text().collect::<Vec<_>>().join(" ");
                    let similarity = SIMILARITY_REGEX
                        .captures(&table_text)
                        .and_then(|cap| cap[1].parse::<u8>().ok())
                        .unwrap_or(0);

                    if similarity < threshold {
                        continue;
                    }

                    for a_tag in table.select(&a_sel) {
                        if let Some(href) = a_tag.value().attr("href") {
                            if let Ok((srv, id)) = self.extract_service_and_id(href) {
                                if valid_lower.contains(&srv.to_lowercase()) {
                                    if best_match.is_none()
                                        || similarity > best_match.as_ref().unwrap().2
                                    {
                                        best_match = Some((srv.clone(), id, similarity));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        best_match.ok_or_else(|| "No match found on IQDB that meets the confidence threshold for your configured services.".to_string())
    }

    /// Helper method to extract the destination Booru service and specific image ID from an IQDB result's hyperlink (`href`).
    ///
    /// # Arguments
    /// * `href` - The URL string found in the IQDB result table.
    ///
    /// # Returns
    /// * `Result<(String, String), String>` - A tuple containing `(Service Name, Post ID/MD5)` if successfully parsed,
    ///                                        otherwise an error string.
    fn extract_service_and_id(&self, href: &str) -> Result<(String, String), String> {
        let href_lower = href.to_lowercase();

        if href_lower.contains("danbooru") && href_lower.contains("/posts/") {
            if let Some(id_str) = href_lower.split("/posts/").nth(1) {
                let id: String = id_str.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !id.is_empty() {
                    return Ok(("Danbooru".to_string(), id));
                }
            }
        } else if href_lower.contains("yande.re") && href_lower.contains("/post/show/") {
            if let Some(id_str) = href_lower.split("/post/show/").nth(1) {
                let id: String = id_str.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !id.is_empty() {
                    return Ok(("Yande.re".to_string(), id));
                }
            }
        } else if href_lower.contains("gelbooru") {
            if href_lower.contains("id=") {
                if let Some(id_str) = href_lower.split("id=").nth(1) {
                    let id: String = id_str.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !id.is_empty() {
                        return Ok(("Gelbooru".to_string(), id));
                    }
                }
            } else if href_lower.contains("md5=") {
                if let Some(md5_str) = href_lower.split("md5=").nth(1) {
                    let md5: String = md5_str
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric())
                        .collect();
                    if md5.len() == 32 {
                        return Ok(("Gelbooru".to_string(), md5));
                    }
                }
            }
        }

        Err("Not a valid booru post link".to_string())
    }
}
