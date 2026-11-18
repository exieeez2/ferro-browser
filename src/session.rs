use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SessionData {
    pub tabs: Vec<SessionTab>,
    pub closed_at: u64,
}

impl SessionData {
    pub fn file_path() -> PathBuf {
        #[cfg(test)]
        {
            return std::env::temp_dir().join(format!("ferro_test_session_{}.json", std::process::id()));
        }
        #[cfg(not(test))]
        {
            let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
            config_dir.join("ferro").join("session.json")
        }
    }

    pub fn save(tabs: &[SessionTab]) -> Result<(), std::io::Error> {
        let path = Self::file_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Filter out empty tabs, settings/internal utility views, or solitary newtab
        let valid_tabs: Vec<SessionTab> = tabs
            .iter()
            .filter(|t| {
                !t.url.is_empty()
                    && t.url != "about:blank"
                    && t.url != "ferro://newtab"
                    && t.url != "ferro://settings"
                    && t.url != "ferro://view-source"
            })
            .cloned()
            .collect();

        if valid_tabs.is_empty() {
            Self::clear();
            return Ok(());
        }

        let data = SessionData {
            tabs: valid_tabs,
            closed_at: now,
        };

        let json = serde_json::to_string_pretty(&data)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load() -> Option<Self> {
        let path = Self::file_path();
        if !path.exists() {
            return None;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(data) = serde_json::from_str::<SessionData>(&content) {
                if !data.tabs.is_empty() {
                    return Some(data);
                }
            }
        }
        None
    }

    pub fn clear() {
        let path = Self::file_path();
        let _ = fs::remove_file(path);
    }

    #[allow(dead_code)]
    pub fn has_restorable_session() -> bool {
        if let Some(session) = Self::load() {
            session.tabs.iter().any(|t| t.url != "ferro://newtab")
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_save_and_load() {
        let tabs = vec![
            SessionTab {
                url: "https://github.com".to_string(),
                title: "GitHub".to_string(),
            },
            SessionTab {
                url: "https://archlinux.org".to_string(),
                title: "Arch Linux".to_string(),
            },
        ];

        let res = SessionData::save(&tabs);
        assert!(res.is_ok());

        let loaded = SessionData::load();
        assert!(loaded.is_some());
        let session = loaded.unwrap();
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[0].url, "https://github.com");
        assert_eq!(session.tabs[1].url, "https://archlinux.org");

        SessionData::clear();
        assert!(SessionData::load().is_none());
    }
}
