use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkItem {
    pub url: String,
    pub title: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarksData {
    pub items: Vec<BookmarkItem>,
}

impl Default for BookmarksData {
    fn default() -> Self {
        Self {
            items: vec![
                BookmarkItem {
                    url: "https://wiki.archlinux.org".to_string(),
                    title: "ArchWiki".to_string(),
                    icon: "A".to_string(),
                },
                BookmarkItem {
                    url: "https://github.com".to_string(),
                    title: "GitHub".to_string(),
                    icon: "G".to_string(),
                },
                BookmarkItem {
                    url: "https://reddit.com".to_string(),
                    title: "Reddit".to_string(),
                    icon: "R".to_string(),
                },
                BookmarkItem {
                    url: "https://www.youtube.com".to_string(),
                    title: "YouTube".to_string(),
                    icon: "Y".to_string(),
                },
            ],
        }
    }
}

impl BookmarksData {
    pub fn config_path() -> PathBuf {
        #[cfg(test)]
        {
            return std::env::temp_dir().join(format!("ferro_test_bookmarks_{}.json", std::process::id()));
        }
        #[cfg(not(test))]
        {
            if let Some(config_dir) = dirs::config_dir() {
                let dir = config_dir.join("ferro");
                let _ = fs::create_dir_all(&dir);
                dir.join("bookmarks.json")
            } else {
                PathBuf::from("bookmarks.json")
            }
        }
    }

    pub fn is_spam_bookmark(url: &str) -> bool {
        let lower = url.to_ascii_lowercase();
        lower.contains("clck.showmelinks.com") || lower.contains("yandex.ru/search/?clid=2296555")
    }

    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(mut parsed) = serde_json::from_str::<BookmarksData>(&data) {
                    let before = parsed.items.len();
                    parsed.items.retain(|b| !Self::is_spam_bookmark(&b.url));
                    if parsed.items.len() != before {
                        parsed.save();
                    }
                    return parsed;
                }
            }
            // If file exists on disk, never overwrite it with defaults on a transient read error
            return Self::default();
        }
        let def = Self::default();
        def.save();
        def
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(serialized) = serde_json::to_string_pretty(self) {
            let tmp_path = path.with_extension("json.tmp");
            if fs::write(&tmp_path, &serialized).is_ok() {
                let _ = fs::rename(&tmp_path, &path);
            } else {
                let _ = fs::write(&path, serialized);
            }
        }
    }

    pub fn contains(&self, url: &str) -> bool {
        self.items.iter().any(|b| b.url == url)
    }

    pub fn add_if_missing(&mut self, url: &str, title: &str) -> bool {
        if self.contains(url) || Self::is_spam_bookmark(url) {
            return false;
        }
        let icon = title
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "★".to_string());
        self.items.push(BookmarkItem {
            url: url.to_string(),
            title: if title.is_empty() { url.to_string() } else { title.to_string() },
            icon,
        });
        true
    }

    pub fn toggle(&mut self, url: &str, title: &str) -> bool {
        if let Some(idx) = self.items.iter().position(|b| b.url == url) {
            self.items.remove(idx);
            self.save();
            false
        } else {
            self.add_if_missing(url, title);
            self.save();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmarks_toggle() {
        let mut bms = BookmarksData::default();
        let test_url = "https://news.ycombinator.com";
        assert!(!bms.contains(test_url));
        let added = bms.toggle(test_url, "Hacker News");
        assert!(added);
        assert!(bms.contains(test_url));
        let removed = bms.toggle(test_url, "Hacker News");
        assert!(!removed);
        assert!(!bms.contains(test_url));
    }
}

