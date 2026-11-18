use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_restore_session_mode() -> String {
    "prompt".to_string()
}

fn default_search_engine() -> String {
    "google".to_string()
}

fn default_homepage_url() -> String {
    "ferro://newtab".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_zoom_level() -> u32 {
    100
}

fn default_download_dir() -> String {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .to_string_lossy()
        .to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BrowserSettings {
    pub theme_mode: String,            // "dark" | "light"
    pub accent_color: String,          // "#ececec", "#3b82f6", "#10b981", "#8b5cf6", etc.
    pub corner_radius: u32,            // 0 to 20
    pub ui_transparency: u32,          // 0 to 60 (% transparency)
    pub top_panel_theme: String,       // "default", "carbon", "aurora", "sunset", "neon"
    pub wallpaper_type: String,        // "none", "forest", "cyberpunk", "nebula", "obsidian", "custom"
    pub wallpaper_url: String,         // URL or base64 data
    pub wallpaper_blur: u32,           // 0 to 25 px
    pub wallpaper_brightness: u32,     // 20 to 100 %
    pub has_prompted_import: bool,
    #[serde(default = "default_restore_session_mode")]
    pub restore_session_mode: String,  // "prompt" | "always" | "never"
    #[serde(default = "default_search_engine")]
    pub search_engine: String,         // "google" | "duckduckgo" | "yandex" | "bing" | "brave" | "youtube" | "github"
    #[serde(default = "default_homepage_url")]
    pub homepage_url: String,
    #[serde(default = "default_true")]
    pub show_bookmarks_bar: bool,
    #[serde(default = "default_true")]
    pub show_ntp_shortcuts: bool,
    #[serde(default = "default_zoom_level")]
    pub default_zoom: u32,             // 50 to 200 (%)
    #[serde(default = "default_true")]
    pub smooth_scrolling: bool,
    #[serde(default = "default_true")]
    pub adblock_enabled: bool,
    #[serde(default = "default_true")]
    pub do_not_track: bool,
    #[serde(default = "default_true")]
    pub offer_save_passwords: bool,
    #[serde(default = "default_true")]
    pub enable_javascript: bool,
    #[serde(default = "default_true")]
    pub enable_webgl: bool,
    #[serde(default = "default_download_dir")]
    pub download_dir: String,
    #[serde(default = "default_true")]
    pub sponsorblock_enabled: bool,
    #[serde(default = "default_true")]
    pub sb_sponsor: bool,
    #[serde(default = "default_true")]
    pub sb_selfpromo: bool,
    #[serde(default = "default_true")]
    pub sb_interaction: bool,
    #[serde(default = "default_true")]
    pub sb_intro: bool,
    #[serde(default = "default_true")]
    pub sb_outro: bool,
    #[serde(default = "default_true")]
    pub sb_preview: bool,
    #[serde(default = "default_true")]
    pub sb_music_offtopic: bool,
    #[serde(default = "default_false")]
    pub sb_filler: bool,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            theme_mode: "dark".to_string(),
            accent_color: "#ececec".to_string(),
            corner_radius: 4,
            ui_transparency: 0,
            top_panel_theme: "default".to_string(),
            wallpaper_type: "none".to_string(),
            wallpaper_url: "".to_string(),
            wallpaper_blur: 0,
            wallpaper_brightness: 80,
            has_prompted_import: false,
            restore_session_mode: "prompt".to_string(),
            search_engine: "google".to_string(),
            homepage_url: default_homepage_url(),
            show_bookmarks_bar: true,
            show_ntp_shortcuts: true,
            default_zoom: 100,
            smooth_scrolling: true,
            adblock_enabled: true,
            do_not_track: true,
            offer_save_passwords: true,
            enable_javascript: true,
            enable_webgl: true,
            download_dir: default_download_dir(),
            sponsorblock_enabled: true,
            sb_sponsor: true,
            sb_selfpromo: true,
            sb_interaction: true,
            sb_intro: true,
            sb_outro: true,
            sb_preview: true,
            sb_music_offtopic: true,
            sb_filler: false,
        }
    }
}

impl BrowserSettings {
    pub fn file_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        config_dir.join("ferro").join("settings.json")
    }

    pub fn load_or_default() -> Self {
        let path = Self::file_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut settings) = serde_json::from_str::<BrowserSettings>(&content) {
                    let mut changed = false;
                    if settings.accent_color != "#ececec" && settings.accent_color != "#18181b" {
                        settings.accent_color = if settings.theme_mode == "light" {
                            "#18181b".to_string()
                        } else {
                            "#ececec".to_string()
                        };
                        changed = true;
                    }
                    if changed {
                        let _ = settings.save();
                    }
                    return settings;
                }
            }
        }
        let default_settings = Self::default();
        let _ = default_settings.save();
        default_settings
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::file_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn generate_css_variables(&self) -> String {
        let (bg_strip, bg_toolbar, bg_surface, bg_elevated, text_pri, text_sec, border) = if self.theme_mode == "light" {
            (
                "rgba(244, 244, 246, VAR_OP)",
                "rgba(244, 244, 246, VAR_OP)",
                "rgba(255, 255, 255, VAR_OP)",
                "rgba(241, 242, 245, VAR_OP)",
                "#18181b",
                "#52525b",
                "#e4e4e7",
            )
        } else {
            (
                "rgba(18, 18, 20, VAR_OP)",
                "rgba(18, 18, 20, VAR_OP)",
                "rgba(20, 20, 22, VAR_OP)",
                "rgba(27, 27, 31, VAR_OP)",
                "#ececef",
                "#a1a1aa",
                "#27272a",
            )
        };

        let op_val = ((100 - self.ui_transparency.min(70)) as f32 / 100.0).to_string();
        let bg_strip = bg_strip.replace("VAR_OP", &op_val);
        let bg_toolbar = bg_toolbar.replace("VAR_OP", &op_val);
        let bg_surface = bg_surface.replace("VAR_OP", &op_val);
        let bg_elevated = bg_elevated.replace("VAR_OP", &op_val);

        let eff_accent = if self.theme_mode == "light" { "#18181b" } else { "#ececec" };

        let blur_val = if self.ui_transparency > 0 {
            format!("blur({}px) saturate(180%)", 12 + self.ui_transparency / 3)
        } else {
            "none".to_string()
        };

        format!(
            r#"
            :root {{
                --accent: {accent};
                --accent-hover: {accent};
                --accent-subtle: {accent}22;
                --radius-tab: {rad_tab}px;
                --radius-ctrl: {rad_ctrl}px;
                --radius-card: {rad_card}px;
                --radius-pill: {rad_pill}px;
                --bg-strip: {bg_strip};
                --bg-toolbar: {bg_toolbar};
                --bg-surface: {bg_surface};
                --bg-elevated: {bg_elevated};
                --text-primary: {text_pri};
                --text-secondary: {text_sec};
                --border-hairline: {border};
                --top-panel-bg: {top_bg};
                --ui-backdrop-filter: {blur_val};
            }}
            "#,
            accent = eff_accent,
            rad_tab = self.corner_radius,
            rad_ctrl = self.corner_radius.max(2),
            rad_card = (self.corner_radius * 2).max(4),
            rad_pill = if self.corner_radius > 0 { 9999 } else { 0 },
            bg_strip = bg_strip,
            bg_toolbar = bg_toolbar,
            bg_surface = bg_surface,
            bg_elevated = bg_elevated,
            text_pri = text_pri,
            text_sec = text_sec,
            border = border,
            top_bg = bg_toolbar,
            blur_val = blur_val,
        )
    }
}
