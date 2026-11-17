use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DetectedBrowser {
    pub id: String,
    pub name: String,
    pub profile_path: String,
    pub has_bookmarks: bool,
    pub bookmarks_count: usize,
    pub has_history: bool,
    pub history_count: usize,
    pub has_passwords: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImportResult {
    pub browser_name: String,
    pub bookmarks_imported: usize,
    pub history_imported: usize,
    pub passwords_imported: usize,
}

pub struct BrowserImporter;

impl BrowserImporter {
    pub fn detect_all() -> Vec<DetectedBrowser> {
        let mut list = Vec::new();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/exieeez"));

        let candidates = [
            ("chrome", "Google Chrome", home.join(".config/google-chrome/Default")),
            ("chromium", "Chromium", home.join(".config/chromium/Default")),
            ("brave", "Brave Browser", home.join(".config/BraveSoftware/Brave-Browser/Default")),
            ("vivaldi", "Vivaldi", home.join(".config/vivaldi/Default")),
            ("edge", "Microsoft Edge", home.join(".config/microsoft-edge/Default")),
        ];

        for (id, name, path) in candidates {
            if path.exists() {
                let bm_path = path.join("Bookmarks");
                let hist_path = path.join("History");
                let pass_path = path.join("Login Data");

                let bm_count = if bm_path.exists() {
                    Self::import_chromium_bookmarks(&bm_path).len()
                } else {
                    0
                };

                let hist_count = if hist_path.exists() {
                    Self::import_chromium_history(&hist_path).len()
                } else {
                    0
                };

                list.push(DetectedBrowser {
                    id: id.to_string(),
                    name: name.to_string(),
                    profile_path: path.to_string_lossy().to_string(),
                    has_bookmarks: bm_count > 0 || bm_path.exists(),
                    bookmarks_count: bm_count,
                    has_history: hist_count > 0 || hist_path.exists(),
                    history_count: hist_count,
                    has_passwords: pass_path.exists(),
                });
            }
        }

        // Detect Firefox profiles
        let ff_dir = home.join(".mozilla/firefox");
        if ff_dir.exists() {
            if let Ok(entries) = fs::read_dir(&ff_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    if p.is_dir() && (name.contains("default") || name.contains("release")) {
                        let places = p.join("places.sqlite");
                        let logins = p.join("logins.json");
                        let (bm_count, hist_count) = if places.exists() {
                            let (bm, hist) = Self::import_firefox_places(&places);
                            (bm.len(), hist.len())
                        } else {
                            (0, 0)
                        };
                        list.push(DetectedBrowser {
                            id: format!("firefox_{}", name),
                            name: format!("Mozilla Firefox ({})", name),
                            profile_path: p.to_string_lossy().to_string(),
                            has_bookmarks: bm_count > 0 || places.exists(),
                            bookmarks_count: bm_count,
                            has_history: hist_count > 0 || places.exists(),
                            history_count: hist_count,
                            has_passwords: logins.exists(),
                        });
                        break;
                    }
                }
            }
        }

        list
    }

    pub fn import_chromium_bookmarks(bookmarks_file: &Path) -> Vec<(String, String)> {
        let mut results = Vec::new();
        if let Ok(content) = fs::read_to_string(bookmarks_file) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(roots) = val.get("roots").and_then(|r| r.as_object()) {
                    for (_key, root_val) in roots {
                        Self::collect_chromium_nodes(root_val, &mut results);
                    }
                }
            }
        }
        results
    }

    fn collect_chromium_nodes(node: &serde_json::Value, list: &mut Vec<(String, String)>) {
        if let Some(type_str) = node.get("type").and_then(|t| t.as_str()) {
            if type_str == "url" {
                let url = node.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                let name = node.get("name").and_then(|n| n.as_str()).unwrap_or(&url).to_string();
                if !url.is_empty() && (url.starts_with("http://") || url.starts_with("https://")) {
                    list.push((url, name));
                }
            }
        }
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                Self::collect_chromium_nodes(child, list);
            }
        }
    }

    pub fn import_chromium_history(history_file: &Path) -> Vec<(String, String)> {
        let mut results = Vec::new();
        if !history_file.exists() {
            return results;
        }

        // Copy history (+ WAL/SHM if open) to temporary file to avoid database lock
        let tmp_copy = PathBuf::from("/tmp/ferro_import_chrome_hist.sqlite");
        let _ = fs::copy(history_file, &tmp_copy);
        let wal = history_file.with_file_name("History-wal");
        if wal.exists() {
            let _ = fs::copy(&wal, "/tmp/ferro_import_chrome_hist.sqlite-wal");
        }

        let output = Command::new("sqlite3")
            .args([
                tmp_copy.to_string_lossy().as_ref(),
                "SELECT url, title FROM urls ORDER BY last_visit_time DESC LIMIT 1000;",
            ])
            .output();

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some((url, title)) = line.split_once('|') {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        results.push((url.to_string(), title.to_string()));
                    }
                }
            }
        }
        let _ = fs::remove_file(&tmp_copy);
        let _ = fs::remove_file("/tmp/ferro_import_chrome_hist.sqlite-wal");
        let _ = fs::remove_file("/tmp/ferro_import_chrome_hist.sqlite-shm");
        results
    }

    pub fn import_firefox_places(places_file: &Path) -> (Vec<(String, String)>, Vec<(String, String)>) {
        let mut bookmarks = Vec::new();
        let mut history = Vec::new();
        if !places_file.exists() {
            return (bookmarks, history);
        }

        let tmp_copy = PathBuf::from("/tmp/ferro_import_ff_places.sqlite");
        let _ = fs::copy(places_file, &tmp_copy);
        let wal = places_file.with_file_name("places.sqlite-wal");
        let shm = places_file.with_file_name("places.sqlite-shm");
        if wal.exists() {
            let _ = fs::copy(&wal, "/tmp/ferro_import_ff_places.sqlite-wal");
        }
        if shm.exists() {
            let _ = fs::copy(&shm, "/tmp/ferro_import_ff_places.sqlite-shm");
        }

        // Bookmarks
        if let Ok(out) = Command::new("sqlite3")
            .args([
                tmp_copy.to_string_lossy().as_ref(),
                "SELECT b.title, p.url FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id WHERE b.type = 1 AND p.url NOT LIKE 'place:%' LIMIT 500;",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some((title, url)) = line.split_once('|') {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        bookmarks.push((url.to_string(), title.to_string()));
                    }
                }
            }
        }

        // History
        if let Ok(out) = Command::new("sqlite3")
            .args([
                tmp_copy.to_string_lossy().as_ref(),
                "SELECT url, title FROM moz_places WHERE visit_count > 0 ORDER BY last_visit_date DESC LIMIT 1000;",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some((url, title)) = line.split_once('|') {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        history.push((url.to_string(), title.to_string()));
                    }
                }
            }
        }

        let _ = fs::remove_file(&tmp_copy);
        let _ = fs::remove_file("/tmp/ferro_import_ff_places.sqlite-wal");
        let _ = fs::remove_file("/tmp/ferro_import_ff_places.sqlite-shm");
        (bookmarks, history)
    }

    pub fn execute_import(browser_id: &str) -> ImportResult {
        let all = Self::detect_all();
        let found = all.iter().find(|b| b.id == browser_id || b.id.starts_with(browser_id));

        let mut bookmarks_count = 0;
        let mut history_count = 0;
        let mut browser_name = "Unknown".to_string();

        if let Some(target) = found {
            browser_name = target.name.clone();
            let profile_path = PathBuf::from(&target.profile_path);

            if target.id.starts_with("firefox") {
                let places = profile_path.join("places.sqlite");
                let (bm, hist) = Self::import_firefox_places(&places);
                bookmarks_count = bm.len();
                history_count = hist.len();

                let mut bm_data = crate::bookmarks::BookmarksData::load_or_default();
                for (url, title) in bm {
                    if !bm_data.contains(&url) {
                        bm_data.toggle(&url, &title);
                    }
                }
                bm_data.save();

                let mut hist_data = crate::history::HistoryData::load_or_default();
                for (url, title) in hist {
                    hist_data.add(&url, &title);
                }
                hist_data.save();
            } else {
                // Chromium-based
                let bm_file = profile_path.join("Bookmarks");
                let hist_file = profile_path.join("History");

                let bm = Self::import_chromium_bookmarks(&bm_file);
                bookmarks_count = bm.len();
                let mut bm_data = crate::bookmarks::BookmarksData::load_or_default();
                for (url, title) in bm {
                    if !bm_data.contains(&url) {
                        bm_data.toggle(&url, &title);
                    }
                }
                bm_data.save();

                let hist = Self::import_chromium_history(&hist_file);
                history_count = hist.len();
                let mut hist_data = crate::history::HistoryData::load_or_default();
                for (url, title) in hist {
                    hist_data.add(&url, &title);
                }
                hist_data.save();
            }
        }

        ImportResult {
            browser_name,
            bookmarks_imported: bookmarks_count,
            history_imported: history_count,
            passwords_imported: 0,
        }
    }
}
