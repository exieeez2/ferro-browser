use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

static DETECTED_CACHE: OnceLock<Vec<DetectedBrowser>> = OnceLock::new();

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
        DETECTED_CACHE.get_or_init(Self::scan_all_profiles).clone()
    }

    fn fast_sqlite_count(db_path: &Path, sql: &str) -> usize {
        if !db_path.exists() {
            return 0;
        }
        let uri = format!("file:{}?immutable=1", db_path.to_string_lossy());
        Command::new("sqlite3")
            .args(["-readonly", &uri, sql])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().lines().next()?.trim().parse::<usize>().ok())
            .unwrap_or(0)
    }

    fn read_chromium_profile_label(root_dir: &Path, sub_name: &str) -> Option<String> {
        let local_state = root_dir.join("Local State");
        let raw = fs::read_to_string(local_state).ok()?;
        let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
        val.get("profile")?
            .get("info_cache")?
            .get(sub_name)?
            .get("name")?
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn scan_all_profiles() -> Vec<DetectedBrowser> {
        let mut list = Vec::new();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/exieeez"));

        let chromium_roots = [
            ("chrome", "Google Chrome", home.join(".config/google-chrome")),
            ("chrome_flatpak", "Google Chrome (Flatpak)", home.join(".var/app/com.google.Chrome/config/google-chrome")),
            ("chromium", "Chromium", home.join(".config/chromium")),
            ("chromium_flatpak", "Chromium (Flatpak)", home.join(".var/app/org.chromium.Chromium/config/chromium")),
            ("ungoogled", "Ungoogled Chromium", home.join(".var/app/io.github.ungoogled_software.ungoogled_chromium/config/chromium")),
            ("brave", "Brave", home.join(".config/BraveSoftware/Brave-Browser")),
            ("brave_flatpak", "Brave (Flatpak)", home.join(".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser")),
            ("vivaldi", "Vivaldi", home.join(".config/vivaldi")),
            ("edge", "Microsoft Edge", home.join(".config/microsoft-edge")),
            ("edge_flatpak", "Microsoft Edge (Flatpak)", home.join(".var/app/com.microsoft.Edge/config/microsoft-edge")),
        ];

        for (base_id, base_name, root_dir) in chromium_roots {
            if !root_dir.exists() {
                continue;
            }
            let Ok(entries) = fs::read_dir(&root_dir) else { continue };
            let mut profile_dirs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    if !p.is_dir() {
                        return false;
                    }
                    let nm = p.file_name().unwrap_or_default().to_string_lossy();
                    nm == "Default" || nm.starts_with("Profile ")
                })
                .collect();

            profile_dirs.sort();

            for prof_path in profile_dirs {
                let sub_name = prof_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let bm_path = prof_path.join("Bookmarks");
                let hist_path = prof_path.join("History");
                let pass_path = prof_path.join("Login Data");

                if !bm_path.exists() && !hist_path.exists() {
                    continue;
                }

                let bm_count = if bm_path.exists() {
                    Self::import_chromium_bookmarks(&bm_path).len()
                } else {
                    0
                };
                let hist_count = Self::fast_sqlite_count(&hist_path, "SELECT MIN(COUNT(*), 1000) FROM urls;");

                if bm_count == 0 && hist_count == 0 {
                    continue;
                }

                let label = Self::read_chromium_profile_label(&root_dir, &sub_name);
                let display_name = match (sub_name.as_str(), label) {
                    ("Default", Some(lbl)) => format!("{} ({})", base_name, lbl),
                    ("Default", None) => base_name.to_string(),
                    (_, Some(lbl)) => format!("{} — {} ({})", base_name, lbl, sub_name),
                    (_, None) => format!("{} ({})", base_name, sub_name),
                };

                let safe_sub = sub_name.replace(' ', "_").to_lowercase();
                list.push(DetectedBrowser {
                    id: format!("chromium:{}_{}", base_id, safe_sub),
                    name: display_name,
                    profile_path: prof_path.to_string_lossy().to_string(),
                    has_bookmarks: bm_count > 0,
                    bookmarks_count: bm_count,
                    has_history: hist_count > 0,
                    history_count: hist_count,
                    has_passwords: pass_path.exists(),
                });
            }
        }

        // Scan Firefox / Zen / LibreWolf / Waterfox roots (any folder containing places.sqlite!)
        let gecko_roots = [
            ("firefox", "Mozilla Firefox", home.join(".mozilla/firefox")),
            ("firefox_flatpak", "Firefox (Flatpak)", home.join(".var/app/org.mozilla.firefox/.mozilla/firefox")),
            ("zen", "Zen Browser", home.join(".config/zen")),
            ("zen_home", "Zen Browser", home.join(".zen")),
            ("zen_flatpak", "Zen Browser (Flatpak)", home.join(".var/app/app.zen_browser.zen/.zen")),
            ("librewolf", "LibreWolf", home.join(".librewolf")),
            ("librewolf_flatpak", "LibreWolf (Flatpak)", home.join(".var/app/io.gitlab.librewolf-community/.librewolf")),
            ("waterfox", "Waterfox", home.join(".waterfox")),
            ("waterfox_flatpak", "Waterfox (Flatpak)", home.join(".var/app/net.waterfox.waterfox/.waterfox")),
        ];

        for (base_id, base_name, root_dir) in gecko_roots {
            if !root_dir.exists() {
                continue;
            }
            let Ok(entries) = fs::read_dir(&root_dir) else { continue };
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_dir() {
                    continue;
                }
                let places = p.join("places.sqlite");
                if !places.exists() {
                    continue;
                }
                let sub_name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                let bm_count = Self::fast_sqlite_count(
                    &places,
                    "SELECT MIN(COUNT(*), 500) FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id WHERE b.type = 1 AND p.url LIKE 'http%';",
                );
                let hist_count = Self::fast_sqlite_count(
                    &places,
                    "SELECT MIN(COUNT(*), 1000) FROM moz_places WHERE visit_count > 0 AND url LIKE 'http%';",
                );
                if bm_count == 0 && hist_count == 0 {
                    continue;
                }

                let short_prof = sub_name.split('.').nth(1).unwrap_or(&sub_name);
                list.push(DetectedBrowser {
                    id: format!("gecko:{}_{}", base_id, sub_name.replace(' ', "_")),
                    name: format!("{} ({})", base_name, short_prof),
                    profile_path: p.to_string_lossy().to_string(),
                    has_bookmarks: bm_count > 0,
                    bookmarks_count: bm_count,
                    has_history: hist_count > 0,
                    history_count: hist_count,
                    has_passwords: p.join("logins.json").exists(),
                });
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

        let pid = std::process::id();
        let tmp_copy = std::env::temp_dir().join(format!("ferro_imp_ch_{}.sqlite", pid));
        let tmp_wal = std::env::temp_dir().join(format!("ferro_imp_ch_{}.sqlite-wal", pid));
        let tmp_shm = std::env::temp_dir().join(format!("ferro_imp_ch_{}.sqlite-shm", pid));
        let _ = fs::copy(history_file, &tmp_copy);
        let wal = history_file.with_file_name("History-wal");
        if wal.exists() {
            let _ = fs::copy(&wal, &tmp_wal);
        }

        let output = Command::new("sqlite3")
            .args([
                tmp_copy.to_string_lossy().as_ref(),
                "SELECT url, REPLACE(REPLACE(COALESCE(title,''), char(10), ' '), char(13), ' ') FROM urls ORDER BY last_visit_time DESC LIMIT 1000;",
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
        let _ = fs::remove_file(&tmp_wal);
        let _ = fs::remove_file(&tmp_shm);
        results
    }

    pub fn import_firefox_places(places_file: &Path) -> (Vec<(String, String)>, Vec<(String, String)>) {
        let mut bookmarks = Vec::new();
        let mut history = Vec::new();
        if !places_file.exists() {
            return (bookmarks, history);
        }

        let pid = std::process::id();
        let tmp_copy = std::env::temp_dir().join(format!("ferro_imp_ff_{}.sqlite", pid));
        let tmp_wal = std::env::temp_dir().join(format!("ferro_imp_ff_{}.sqlite-wal", pid));
        let tmp_shm = std::env::temp_dir().join(format!("ferro_imp_ff_{}.sqlite-shm", pid));
        let _ = fs::copy(places_file, &tmp_copy);
        let wal = places_file.with_file_name("places.sqlite-wal");
        let shm = places_file.with_file_name("places.sqlite-shm");
        if wal.exists() {
            let _ = fs::copy(&wal, &tmp_wal);
        }
        if shm.exists() {
            let _ = fs::copy(&shm, &tmp_shm);
        }

        if let Ok(out) = Command::new("sqlite3")
            .args([
                tmp_copy.to_string_lossy().as_ref(),
                "SELECT p.url, REPLACE(REPLACE(COALESCE(b.title,''), char(10), ' '), char(13), ' ') FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id WHERE b.type = 1 AND p.url LIKE 'http%' LIMIT 500;",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some((url, title)) = line.split_once('|') {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        let clean_title = if title.trim().is_empty() { url } else { title.trim() };
                        bookmarks.push((url.to_string(), clean_title.to_string()));
                    }
                }
            }
        }

        if let Ok(out) = Command::new("sqlite3")
            .args([
                tmp_copy.to_string_lossy().as_ref(),
                "SELECT url, REPLACE(REPLACE(COALESCE(title,''), char(10), ' '), char(13), ' ') FROM moz_places WHERE visit_count > 0 AND url LIKE 'http%' ORDER BY last_visit_date DESC LIMIT 1000;",
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
        let _ = fs::remove_file(&tmp_wal);
        let _ = fs::remove_file(&tmp_shm);
        (bookmarks, history)
    }

    pub fn execute_import(browser_id: &str) -> ImportResult {
        let all = Self::detect_all();
        let found = all
            .iter()
            .find(|b| b.id == browser_id || b.id.starts_with(browser_id))
            .or_else(|| all.first());

        let mut bookmarks_count = 0;
        let mut history_count = 0;
        let mut browser_name = "Браузер".to_string();

        if let Some(target) = found {
            browser_name = target.name.clone();
            let profile_path = PathBuf::from(&target.profile_path);

            if target.id.starts_with("gecko:") || target.id.starts_with("firefox") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_all_profiles() {
        let t0 = std::time::Instant::now();
        let browsers = BrowserImporter::detect_all();
        println!(
            "[Importer Test] Detected {} browser profiles in {:?}: {:?}",
            browsers.len(),
            t0.elapsed(),
            browsers
                .iter()
                .map(|b| format!("{} (bm={}, hist={})", b.name, b.bookmarks_count, b.history_count))
                .collect::<Vec<_>>()
        );
    }
}
