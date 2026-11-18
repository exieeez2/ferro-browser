mod adblocker;
mod bookmarks;
mod downloads;
mod history;
mod importer;
mod injector;
mod lens;
mod passwords;
mod settings;
mod webkit_filter;

use gtk::gdk;
use gtk::prelude::*;
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use webkit2gtk::{
    CacheModel, DownloadExt, FindControllerExt, FindOptions, HardwareAccelerationPolicy, LoadEvent,
    NavigationPolicyDecision, NavigationPolicyDecisionExt, PolicyDecisionExt, PolicyDecisionType,
    PrintOperation, PrintOperationExt, SettingsExt, SnapshotOptions, SnapshotRegion, URIRequestExt,
    WebContextExt, WebInspectorExt, WebViewExt, WebsiteDataManagerExtManual, WebsiteDataTypes,
};
use wry::{WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

mod session;

use adblocker::AdBlocker;
use bookmarks::BookmarksData;
use downloads::DownloadsData;
use history::HistoryData;
use importer::BrowserImporter;
use lens::{
    base64_encode, download_image_bytes, render_bookmarks_panel_html,
    render_lens_in_session_upload_html, render_lens_loading_html, render_side_header_html,
    render_sleeping_tab_html,
};
use passwords::PasswordsData;
use session::{SessionData, SessionTab};
use settings::BrowserSettings;
use injector::{
    ADBLOCK_COSMETIC_CSS, ADBLOCK_JS, CONTEXT_MENU_JS, HOTKEYS_JS, TOGGLE_DARK_MODE_JS, TRANSLATE_PAGE_JS,
};
use webkit_filter::{add_user_style_sheet, attach_to_ucm, detach_from_ucm, load_filter, save_filter, WebKitFilter};

const SHELL_HTML: &str = include_str!("ui/shell.html");
const NEWTAB_HTML: &str = include_str!("ui/newtab.html");
const TOKENS_HTML: &str = include_str!("ui/tokens.html");
const SETTINGS_HTML: &str = include_str!("ui/settings.html");

static NEXT_TAB_ID: AtomicU32 = AtomicU32::new(1);

thread_local! {
    static ALL_WINDOWS: RefCell<Vec<Rc<RefCell<BrowserState>>>> = const { RefCell::new(Vec::new()) };
}

fn open_windows_snapshot() -> Vec<Rc<RefCell<BrowserState>>> {
    ALL_WINDOWS.with(|wins| wins.borrow().clone())
}

fn open_windows_count() -> usize {
    ALL_WINDOWS.with(|wins| wins.borrow().len())
}

#[derive(Clone)]
struct SharedBrowserCore {
    web_context: Rc<RefCell<wry::WebContext>>,
    adblocker: Arc<AdBlocker>,
    native_filter: Rc<RefCell<Option<WebKitFilter>>>,
    css_provider: gtk::CssProvider,
    downloads: Arc<Mutex<DownloadsData>>,
    lens_preview: Arc<Mutex<String>>,
    page_source_cache: Arc<Mutex<(String, String)>>,
    settings_dirty: Arc<AtomicBool>,
    bookmarks_dirty: Arc<AtomicBool>,
    history_dirty: Arc<AtomicBool>,
    passwords_dirty: Arc<AtomicBool>,
    download_handler_attached: Rc<Cell<bool>>,
}

struct DetachedTabSpec {
    url: String,
    title: String,
    is_muted: bool,
    zoom_pct: u32,
    media_sec: u32,
    scroll_x: i32,
    scroll_y: i32,
    size: (i32, i32),
    start_move_drag: bool,
    press_time: u32,
}

enum InitialWindowMode {
    Startup,
    NewBlankWindow,
    DetachedTab(DetachedTabSpec),
}

struct PrewarmedTab {
    id: u32,
    tab_name: String,
    webview: Rc<WebView>,
    inner: webkit2gtk::WebView,
    container: gtk::Box,
}

struct Tab {
    id: u32,
    url: String,
    title: String,
    can_go_back: bool,
    can_go_forward: bool,
    is_loading: bool,
    is_pinned: bool,
    is_muted: bool,
    sleeping_url: Option<String>,
    zoom_pct: u32,
    last_media_sec: u32,
    last_scroll_x: i32,
    last_scroll_y: i32,
    webview: Rc<WebView>,
    inner: webkit2gtk::WebView,
    container: gtk::Box,
}

struct BrowserState {
    core: SharedBrowserCore,
    window: gtk::Window,
    top_box: gtk::Box,
    hpaned: gtk::Paned,
    stack: gtk::Stack,
    side_box: gtk::Box,
    side_header_webview: Option<Rc<WebView>>,
    side_webview: Option<Rc<WebView>>,
    side_panel_open: bool,
    lens_preview: Arc<Mutex<String>>,
    last_lens_result_url: Option<String>,
    spare_tab: Option<PrewarmedTab>,
    active_popover: Option<gtk::Popover>,
    pip_window: Option<(gtk::Window, Rc<WebView>, Option<u32>)>,
    win_fullscreen: bool,
    last_btn1_press_time: Rc<Cell<u32>>,
    css_provider: gtk::CssProvider,
    top_webview: Option<Rc<WebView>>,
    tabs: Vec<Tab>,
    closed_tabs: Vec<(String, String)>,
    active_tab_id: Option<u32>,
    bookmarks: BookmarksData,
    history: HistoryData,
    passwords: PasswordsData,
    downloads: Arc<Mutex<DownloadsData>>,
    pending_password: Option<(String, String, String)>,
    settings: BrowserSettings,
    web_context: Rc<RefCell<wry::WebContext>>,
    adblocker: Arc<AdBlocker>,
    native_filter: Rc<RefCell<Option<WebKitFilter>>>,
    dark_mode: bool,
    theme: String,
    download_handler_attached: Rc<Cell<bool>>,
    settings_dirty: Arc<AtomicBool>,
    bookmarks_dirty: Arc<AtomicBool>,
    history_dirty: Arc<AtomicBool>,
    passwords_dirty: Arc<AtomicBool>,
    page_source_cache: Arc<Mutex<(String, String)>>,
}

#[derive(Deserialize, Debug)]
struct IpcMessage {
    action: String,
    url: Option<String>,
    id: Option<u32>,
    title: Option<String>,
    pass: Option<String>,
    settings: Option<BrowserSettings>,
}

fn send_to_ui(state_rc: &Rc<RefCell<BrowserState>>, js: &str) {
    let top_wv = {
        if let Ok(state) = state_rc.try_borrow() {
            state.top_webview.clone()
        } else {
            None
        }
    };
    if let Some(top) = top_wv {
        let _ = top.evaluate_script(js);
    }
}

fn sync_tabs_to_ui(state_rc: &Rc<RefCell<BrowserState>>) {
    let (tabs_json, active_id, top_wv) = {
        if let Ok(state) = state_rc.try_borrow() {
            let tabs_info: Vec<serde_json::Value> = state
                .tabs
                .iter()
                .map(|t| {
                    let display_url = t.sleeping_url.as_deref().unwrap_or(&t.url);
                    serde_json::json!({
                        "id": t.id,
                        "url": display_url,
                        "title": t.title,
                        "canGoBack": t.can_go_back,
                        "canGoForward": t.can_go_forward,
                        "isLoading": t.is_loading,
                        "isPinned": t.is_pinned,
                        "isMuted": t.is_muted,
                        "isSleeping": t.sleeping_url.is_some(),
                        "zoomPct": t.zoom_pct,
                    })
                })
                .collect();
            let json = serde_json::to_string(&tabs_info).unwrap_or_else(|_| "[]".to_string());
            (Some(json), state.active_tab_id, state.top_webview.clone())
        } else {
            (None, None, None)
        }
    };

    if let (Some(tabs_json), Some(top)) = (tabs_json, top_wv) {
        let active_id_str = match active_id {
            Some(id) => id.to_string(),
            None => "null".to_string(),
        };
        let js = format!("if (window.ferro) {{ window.ferro.setTabs({}, {}); }}", tabs_json, active_id_str);
        let _ = top.evaluate_script(&js);
    }
}

fn sync_tab_update_to_ui(state_rc: &Rc<RefCell<BrowserState>>, tab_id: u32) {
    let (tab_data, top_wv) = {
        if let Ok(state) = state_rc.try_borrow() {
            let data = state.tabs.iter().find(|t| t.id == tab_id).map(|t| {
                let display_url = t.sleeping_url.as_deref().unwrap_or(&t.url);
                serde_json::json!({
                    "url": display_url,
                    "title": t.title,
                    "canGoBack": t.can_go_back,
                    "canGoForward": t.can_go_forward,
                    "isLoading": t.is_loading,
                    "isPinned": t.is_pinned,
                    "isMuted": t.is_muted,
                    "isSleeping": t.sleeping_url.is_some(),
                    "zoomPct": t.zoom_pct,
                })
            });
            (data, state.top_webview.clone())
        } else {
            (None, None)
        }
    };

    if let (Some(data), Some(top)) = (tab_data, top_wv) {
        let js = format!(
            "if (window.ferro) {{ window.ferro.updateTab({}, {}); }}",
            tab_id,
            serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string())
        );
        let _ = top.evaluate_script(&js);
    }
}

fn sync_bookmarks_to_ui(state_rc: &Rc<RefCell<BrowserState>>) {
    let (bms_json, top_wv) = {
        if let Ok(state) = state_rc.try_borrow() {
            let json = serde_json::to_string(&state.bookmarks.items).unwrap_or_else(|_| "[]".to_string());
            (Some(json), state.top_webview.clone())
        } else {
            (None, None)
        }
    };
    if let (Some(bms_json), Some(top)) = (bms_json, top_wv) {
        let js = format!("if (window.ferro) {{ window.ferro.setBookmarks({}); }}", bms_json);
        let _ = top.evaluate_script(&js);
    }
}

fn sync_adblock_to_ui(state_rc: &Rc<RefCell<BrowserState>>) {
    let (status, top_wv) = {
        if let Ok(state) = state_rc.try_borrow() {
            (
                Some((state.adblocker.is_enabled(), state.adblocker.blocked_count())),
                state.top_webview.clone(),
            )
        } else {
            (None, None)
        }
    };
    if let (Some((enabled, count)), Some(top)) = (status, top_wv) {
        let js = format!("if (window.ferro) {{ window.ferro.setAdblockStatus({}, {}); }}", enabled, count);
        let _ = top.evaluate_script(&js);
    }
}

fn compute_surface_rgba(s: &BrowserSettings) -> gdk::RGBA {
    let alpha = ((100 - s.ui_transparency.min(60)) as f64) / 100.0;
    let key = if s.top_panel_theme != "default" && !s.top_panel_theme.is_empty() {
        s.top_panel_theme.as_str()
    } else {
        s.wallpaper_type.as_str()
    };
    match key {
        "aurora" | "forest" => gdk::RGBA::new(11.0 / 255.0, 46.0 / 255.0, 49.0 / 255.0, alpha),
        "neon" | "cyberpunk" => gdk::RGBA::new(22.0 / 255.0, 27.0 / 255.0, 64.0 / 255.0, alpha),
        "sunset" => gdk::RGBA::new(48.0 / 255.0, 22.0 / 255.0, 56.0 / 255.0, alpha),
        "nebula" => gdk::RGBA::new(24.0 / 255.0, 18.0 / 255.0, 54.0 / 255.0, alpha),
        "carbon" | "obsidian" => gdk::RGBA::new(24.0 / 255.0, 26.0 / 255.0, 33.0 / 255.0, alpha),
        _ => {
            if s.theme_mode == "light" {
                gdk::RGBA::new(244.0 / 255.0, 244.0 / 255.0, 246.0 / 255.0, alpha)
            } else {
                gdk::RGBA::new(18.0 / 255.0, 18.0 / 255.0, 20.0 / 255.0, alpha)
            }
        }
    }
}

fn update_gtk_menu_css(css_provider: &gtk::CssProvider, s: &BrowserSettings) {
    let is_light = s.theme_mode == "light";
    let rgba = compute_surface_rgba(s);
    let surface_css = format!(
        "rgba({}, {}, {}, {:.2})",
        (rgba.red() * 255.0).round() as u8,
        (rgba.green() * 255.0).round() as u8,
        (rgba.blue() * 255.0).round() as u8,
        rgba.alpha()
    );
    let (bg, fg, hover_bg, border) = if is_light {
        ("#ffffff", "#18181b", "#ececf1", "#d4d4d8")
    } else {
        ("#1c1c20", "#f2f2f5", "#2d2c36", "#353440")
    };
    let rad = s.corner_radius.clamp(2, 14);
    let item_rad = rad.saturating_sub(2).max(2);
    let css = format!(
        "
        window, .window, stack {{
            background-color: {surface_css};
        }}
        box {{
            background-color: transparent;
        }}
        menu, popover, popover.background {{
            background-color: {bg};
            background-image: none;
            border: 1px solid {border};
            border-radius: {rad}px;
            padding: 4px;
            box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
        }}
        menuitem, .ferro-pop-btn {{
            color: {fg};
            background-color: transparent;
            background-image: none;
            border: none;
            box-shadow: none;
            font-size: 13px;
            padding: 6px 14px;
            border-radius: {item_rad}px;
        }}
        menuitem:hover, .ferro-pop-btn:hover {{
            background-color: {hover_bg};
            background-image: none;
            color: {fg};
        }}
        separator {{
            background-color: {border};
            min-height: 1px;
            margin: 4px 0;
        }}
        "
    );
    let _ = css_provider.load_from_data(css.as_bytes());
}

enum PopoverEntry {
    Item(String, Box<dyn Fn() + 'static>),
    Separator,
}

fn parse_menu_coords(pass: Option<&str>, default_x: i32, default_y: i32) -> (i32, i32) {
    if let Some(raw) = pass {
        if let Some((xs, ys)) = raw.split_once(',') {
            if let (Ok(x), Ok(y)) = (xs.trim().parse::<i32>(), ys.trim().parse::<i32>()) {
                return (x, y);
            }
        }
    }
    (default_x, default_y)
}

fn show_top_popover(
    state_rc: &Rc<RefCell<BrowserState>>,
    raw_x: i32,
    raw_y: i32,
    entries: Vec<PopoverEntry>,
) {
    let top_box = {
        if let Ok(mut st) = state_rc.try_borrow_mut() {
            if let Some(old) = st.active_popover.take() {
                old.popdown();
            }
            st.top_box.clone()
        } else {
            return;
        }
    };

    let alloc = top_box.allocation();
    let max_x = (alloc.width() - 8).max(12);
    let max_y = (alloc.height() - 2).max(12);
    let cx = raw_x.clamp(8, max_x);
    let cy = raw_y.clamp(8, max_y);

    let popover = gtk::Popover::new(Some(&top_box));
    popover.set_pointing_to(&gdk::Rectangle::new(cx, cy, 2, 2));
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_constrain_to(gtk::PopoverConstraint::Window);
    popover.set_modal(true);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    vbox.set_margin_top(4);
    vbox.set_margin_bottom(4);
    vbox.set_margin_start(4);
    vbox.set_margin_end(4);

    for entry in entries {
        match entry {
            PopoverEntry::Separator => {
                let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
                vbox.pack_start(&sep, false, false, 2);
            }
            PopoverEntry::Item(label, callback) => {
                let btn = gtk::Button::new();
                btn.set_relief(gtk::ReliefStyle::None);
                btn.set_focus_on_click(false);
                btn.style_context().add_class("ferro-pop-btn");
                let lbl = gtk::Label::new(Some(&label));
                lbl.set_xalign(0.0);
                btn.add(&lbl);
                let pop_clone = popover.clone();
                btn.connect_clicked(move |_| {
                    pop_clone.popdown();
                    callback();
                });
                vbox.pack_start(&btn, false, false, 0);
            }
        }
    }

    popover.add(&vbox);
    vbox.show_all();

    if let Ok(mut st) = state_rc.try_borrow_mut() {
        st.active_popover = Some(popover.clone());
    }
    popover.popup();
}

fn render_source_viewer_html(url: &str, raw_html: &str, s: &BrowserSettings) -> String {
    let escaped = raw_html
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let lines_html: String = escaped
        .lines()
        .enumerate()
        .map(|(i, line)| {
            format!(
                r#"<div class="line"><span class="ln">{}</span><span class="lc">{}</span></div>"#,
                i + 1,
                line
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let escaped_url = url.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let is_light = s.theme_mode == "light";
    let (bg, fg, muted, border) = if is_light {
        ("#fafafa", "#18181b", "#71717a", "#e4e4e7")
    } else {
        ("#111113", "#e4e4e7", "#71717a", "#27272a")
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="ru">
<head>
  <meta charset="UTF-8" />
  <title>Исходный код — {escaped_url}</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{ background: {bg}; color: {fg}; font-family: 'JetBrains Mono', ui-monospace, monospace; font-size: 12px; line-height: 1.55; }}
    .hdr {{ position: sticky; top: 0; background: {bg}; border-bottom: 1px solid {border}; padding: 10px 16px; font-size: 12px; color: {muted}; z-index: 2; }}
    .hdr b {{ color: {fg}; }}
    .code {{ padding: 12px 0 40px 0; overflow-x: auto; }}
    .line {{ display: flex; padding: 0 16px; white-space: pre-wrap; word-break: break-all; }}
    .line:hover {{ background: rgba(128,128,128,0.08); }}
    .ln {{ width: 52px; flex-shrink: 0; color: {muted}; text-align: right; padding-right: 16px; user-select: none; }}
    .lc {{ flex: 1; }}
  </style>
</head>
<body>
  <div class="hdr">Исходный код страницы: <b>{escaped_url}</b></div>
  <div class="code">{lines_html}</div>
</body>
</html>"#
    )
}

fn find_active_window_webview() -> Option<Rc<WebView>> {
    let wins = open_windows_snapshot();
    let target = wins
        .iter()
        .find(|w| w.try_borrow().map(|s| s.window.is_active()).unwrap_or(false))
        .or_else(|| wins.last())?;
    target.try_borrow().ok().and_then(|s| {
        s.active_tab_id
            .and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone()))
    })
}

fn attach_download_handler(
    ctx: &webkit2gtk::WebContext,
    downloads_store: Arc<Mutex<DownloadsData>>,
) {
    ctx.connect_download_started(move |_ctx, download| {
        let dl_store_start = downloads_store.clone();
        let dl_id_cell = Rc::new(RefCell::new(0_u64));
        let dl_id_for_dest = dl_id_cell.clone();

        download.connect_decide_destination(move |dl, suggested_filename| {
            let dl_dir = BrowserSettings::load_or_default().download_dir;
            let base_dir = if dl_dir.trim().is_empty() {
                dirs::download_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            } else {
                std::path::PathBuf::from(dl_dir)
            };
            let _ = std::fs::create_dir_all(&base_dir);

            let safe_name = if suggested_filename.trim().is_empty() {
                "download.bin"
            } else {
                suggested_filename
            };
            let mut dest_path = base_dir.join(safe_name);
            if dest_path.exists() {
                let stem = dest_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string());
                let ext = dest_path
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                for n in 1..1000 {
                    let candidate = base_dir.join(format!("{} ({}){}", stem, n, ext));
                    if !candidate.exists() {
                        dest_path = candidate;
                        break;
                    }
                }
            }

            let uri = format!("file://{}", dest_path.to_string_lossy());
            dl.set_destination(&uri);

            let src_url = dl
                .request()
                .and_then(|r| r.uri())
                .map(|u| u.to_string())
                .unwrap_or_default();
            let final_name = dest_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| safe_name.to_string());
            if let Ok(mut guard) = dl_store_start.lock() {
                let new_id = guard.add_started(&final_name, &src_url, &dest_path.to_string_lossy());
                *dl_id_for_dest.borrow_mut() = new_id;
            }

            let saved_display = dest_path.to_string_lossy().replace('\\', "/").replace('\'', "\\'");
            if let Some(wv) = find_active_window_webview() {
                let js = format!(
                    r#"(function(){{
                        const t = document.createElement('div');
                        t.textContent = 'Загрузка начата: {0}';
                        t.style.cssText = 'position:fixed;bottom:18px;right:18px;z-index:2147483647;background:#19191c;color:#f2f2f5;border:1px solid #383842;border-radius:6px;padding:9px 14px;font-family:Inter,sans-serif;font-size:12px;box-shadow:0 8px 24px rgba(0,0,0,0.55);';
                        (document.body||document.documentElement).appendChild(t);
                        setTimeout(() => t.remove(), 2600);
                    }})();"#,
                    saved_display
                );
                let _ = wv.evaluate_script(&js);
            }
            true
        });

        let dl_store_prog = downloads_store.clone();
        let dl_id_for_prog = dl_id_cell.clone();
        download.connect_estimated_progress_notify(move |dl| {
            let id = *dl_id_for_prog.borrow();
            if id != 0 {
                let pct = (dl.estimated_progress() * 100.0).round() as u32;
                if let Ok(mut guard) = dl_store_prog.lock() {
                    guard.update_progress(id, pct);
                }
            }
        });

        let dl_store_fail = downloads_store.clone();
        let dl_id_for_fail = dl_id_cell.clone();
        download.connect_failed(move |_dl, _err| {
            let id = *dl_id_for_fail.borrow();
            if id != 0 {
                if let Ok(mut guard) = dl_store_fail.lock() {
                    guard.mark_failed(id);
                }
            }
        });

        let dl_store_fin = downloads_store.clone();
        let dl_id_for_fin = dl_id_cell;
        download.connect_finished(move |dl| {
            let id = *dl_id_for_fin.borrow();
            if id != 0 {
                if let Ok(mut guard) = dl_store_fin.lock() {
                    guard.mark_finished(id);
                }
            }
            let dest = dl
                .destination()
                .map(|d| d.strip_prefix("file://").unwrap_or(&d).to_string())
                .unwrap_or_default();
            if let Some(wv) = find_active_window_webview() {
                let safe_dest = dest.replace('\\', "/").replace('\'', "\\'");
                let js = format!(
                    r#"(function(){{
                        const t = document.createElement('div');
                        t.textContent = 'Файл сохранён: {0}';
                        t.style.cssText = 'position:fixed;bottom:18px;right:18px;z-index:2147483647;background:#19191c;color:#f2f2f5;border:1px solid #383842;border-radius:6px;padding:9px 14px;font-family:Inter,sans-serif;font-size:12px;box-shadow:0 8px 24px rgba(0,0,0,0.55);';
                        (document.body||document.documentElement).appendChild(t);
                        setTimeout(() => t.remove(), 3200);
                    }})();"#,
                    safe_dest
                );
                let _ = wv.evaluate_script(&js);
            }
        });
    });
}

fn broadcast_settings_from_disk() {
    let loaded = BrowserSettings::load_or_default();
    for win_rc in open_windows_snapshot() {
        if let Ok(mut st) = win_rc.try_borrow_mut() {
            st.theme = loaded.theme_mode.clone();
            st.settings = loaded.clone();
        }
        sync_settings_to_ui(&win_rc);
    }
}

fn broadcast_bookmarks_from_disk() {
    let loaded = BookmarksData::load_or_default();
    for win_rc in open_windows_snapshot() {
        if let Ok(mut st) = win_rc.try_borrow_mut() {
            st.bookmarks = loaded.clone();
        }
        sync_bookmarks_to_ui(&win_rc);
    }
}

fn sync_settings_to_ui(state_rc: &Rc<RefCell<BrowserState>>) {
    let (settings_json, top_wv, tab_wvs) = {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            update_gtk_menu_css(&state.css_provider, &state.settings);
            let top_h = if state.settings.show_bookmarks_bar { 102 } else { 74 };
            state.top_box.set_size_request(-1, top_h);

            let surface_rgba = compute_surface_rgba(&state.settings);
            let mut tab_rgba = surface_rgba;
            tab_rgba.set_alpha(1.0);
            let clamped_zoom = state.settings.default_zoom.clamp(50, 200);
            let zoom_factor = (clamped_zoom as f64) / 100.0;
            let smooth_scrolling = state.settings.smooth_scrolling;
            let enable_webgl = state.settings.enable_webgl;
            let enable_js = state.settings.enable_javascript;
            let adblock_enabled = state.settings.adblock_enabled;
            let native_filter = state.native_filter.borrow().clone();
            state.adblocker.set_enabled(adblock_enabled);

            for t in &mut state.tabs {
                t.inner.set_background_color(&tab_rgba);
                t.inner.set_zoom_level(zoom_factor);
                t.zoom_pct = clamped_zoom;
                if let Some(wk_s) = WebViewExt::settings(&t.inner) {
                    wk_s.set_enable_smooth_scrolling(smooth_scrolling);
                    wk_s.set_enable_webgl(enable_webgl);
                    let is_internal = t.url.starts_with("ferro://") || t.url.is_empty();
                    wk_s.set_enable_javascript(is_internal || enable_js);
                }
                if let Some(ucm) = t.inner.user_content_manager() {
                    if let Some(filter) = &native_filter {
                        if adblock_enabled {
                            attach_to_ucm(&ucm, filter);
                        } else {
                            detach_from_ucm(&ucm, filter);
                        }
                    }
                }
            }

            if let Some(top) = &state.top_webview {
                top.webview().set_background_color(&surface_rgba);
            }

            let mut all_wvs: Vec<Rc<WebView>> = state.tabs.iter().map(|t| t.webview.clone()).collect();
            if let Some(sp) = &state.spare_tab {
                sp.inner.set_background_color(&tab_rgba);
                all_wvs.push(sp.webview.clone());
            }
            if let Some(sh) = &state.side_header_webview {
                sh.webview().set_background_color(&surface_rgba);
                all_wvs.push(sh.clone());
            }
            if let Some(sw) = &state.side_webview {
                sw.webview().set_background_color(&surface_rgba);
                all_wvs.push(sw.clone());
            }
            (
                serde_json::to_string(&state.settings).unwrap_or_else(|_| "{}".to_string()),
                state.top_webview.clone(),
                all_wvs,
            )
        } else {
            ("{}".to_string(), None, Vec::new())
        }
    };
    if let Some(top) = top_wv {
        let js = format!(
            "if (window.ferro && window.ferro.applySettings) {{ window.ferro.applySettings({0}); }} else if (window.applySettings) {{ window.applySettings({0}); }}",
            settings_json
        );
        let _ = top.evaluate_script(&js);
    }
    for tab in tab_wvs {
        let js = format!(
            "window._ferro_settings = {0}; if (window.applySettings) {{ window.applySettings({0}); }}",
            settings_json
        );
        let _ = tab.evaluate_script(&js);
    }
    sync_tabs_to_ui(state_rc);
    sync_adblock_to_ui(state_rc);
}

fn ensure_and_open_side_panel(state_rc: &Rc<RefCell<BrowserState>>, mode: &str, target_url: &str) {
    let init_ctx = state_rc.try_borrow().ok().and_then(|st| {
        if st.side_webview.is_none() {
            Some((
                st.side_box.clone(),
                compute_surface_rgba(&st.settings),
                serde_json::to_string(&st.settings).unwrap_or_else(|_| "{}".to_string()),
                st.web_context.clone(),
            ))
        } else {
            None
        }
    });

    if let Some((side_box, surface_rgba, s_json, web_ctx_rc)) = init_ctx {
        let hdr_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        hdr_box.set_size_request(-1, 36);
        side_box.pack_start(&hdr_box, false, false, 0);

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        side_box.pack_start(&content_box, true, true, 0);

        let hdr_init = format!(
            "window.addEventListener('DOMContentLoaded', function() {{ if (window.applySettings) window.applySettings({}); }});",
            s_json
        );

        let state_for_hdr = state_rc.clone();
        let built_hdr = WebViewBuilder::new()
            .with_transparent(true)
            .with_initialization_script(&hdr_init)
            .with_html(render_side_header_html())
            .with_ipc_handler(move |req| {
                if let Ok(msg) = serde_json::from_str::<IpcMessage>(req.body()) {
                    handle_ipc(&state_for_hdr, msg);
                }
            })
            .build_gtk(&hdr_box)
            .ok()
            .map(|hdr_wv| {
                hdr_wv.webview().set_background_color(&surface_rgba);
                hdr_wv.webview().connect_context_menu(|_, _, _, _| true);
                Rc::new(hdr_wv)
            });

        let state_for_body = state_rc.clone();
        let built_body = {
            let mut web_ctx = web_ctx_rc.borrow_mut();
            WebViewBuilder::new_with_web_context(&mut *web_ctx)
                .with_transparent(true)
                .with_ipc_handler(move |req| {
                    if let Ok(msg) = serde_json::from_str::<IpcMessage>(req.body()) {
                        handle_ipc(&state_for_body, msg);
                    }
                })
                .build_gtk(&content_box)
                .ok()
        }
        .map(|body_wv| {
            let inner = body_wv.webview();
            inner.set_background_color(&surface_rgba);
            if let Some(wk_s) = WebViewExt::settings(&inner) {
                wk_s.set_user_agent(Some(
                    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
                ));
                wk_s.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Always);
                wk_s.set_enable_javascript(true);
            }
            let state_weak = Rc::downgrade(state_rc);
            inner.connect_decide_policy(move |_wv, decision, decision_type| {
                if decision_type == PolicyDecisionType::NewWindowAction {
                    if let Ok(nav) = decision.clone().downcast::<NavigationPolicyDecision>() {
                        #[allow(deprecated)]
                        if let Some(req) = nav.request() {
                            if let Some(uri) = req.uri() {
                                if let Some(s_rc) = state_weak.upgrade() {
                                    create_tab(&s_rc, uri.as_str());
                                }
                                decision.ignore();
                                return true;
                            }
                        }
                    }
                }
                false
            });
            let state_weak_uri = Rc::downgrade(state_rc);
            inner.connect_uri_notify(move |wv| {
                if let Some(uri) = wv.uri() {
                    let u = uri.to_string();
                    if u.starts_with("https://www.google.com/search") || u.contains("vsrid=") {
                        if let Some(s_rc) = state_weak_uri.upgrade() {
                            if let Ok(mut st) = s_rc.try_borrow_mut() {
                                st.last_lens_result_url = Some(u);
                            }
                        }
                    }
                }
            });
            Rc::new(body_wv)
        });

        if let Ok(mut st) = state_rc.try_borrow_mut() {
            st.side_header_webview = built_hdr;
            st.side_webview = built_body;
        }
    }

    let (hdr_wv, body_wv) = {
        if let Ok(mut st) = state_rc.try_borrow_mut() {
            if !st.side_panel_open {
                let (win_w, _) = st.window.size();
                st.hpaned.set_position((win_w - 420).max(340));
                st.side_box.set_size_request(340, -1);
                st.side_box.show_all();
                st.side_panel_open = true;
            }
            (st.side_header_webview.clone(), st.side_webview.clone())
        } else {
            (None, None)
        }
    };

    if let Some(hdr) = hdr_wv {
        let _ = hdr.evaluate_script(&format!(
            "if (window.setSideMode) window.setSideMode({:?});",
            mode
        ));
    }
    if let Some(wv) = body_wv {
        if !target_url.is_empty() {
            if target_url.starts_with("ferro://") {
                let _ = wv.load_url(target_url);
            } else {
                wv.webview().load_uri(target_url);
            }
        }
    }
}

fn switch_tab(state_rc: &Rc<RefCell<BrowserState>>, tab_id: u32) {
    let (tab_name, wake_info) = {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            let mut found = false;
            let mut wake = None;
            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                found = true;
                wake = tab.sleeping_url.take().map(|u| {
                    tab.url = u.clone();
                    (tab.inner.clone(), u)
                });
            }
            if found {
                state.active_tab_id = Some(tab_id);
                (Some((format!("tab_{}", tab_id), state.stack.clone())), wake)
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    };

    if let Some((inner, wake_url)) = wake_info {
        inner.load_uri(&wake_url);
    }
    if let Some((name, stack)) = tab_name {
        stack.set_visible_child_name(&name);
    }
    sync_tabs_to_ui(state_rc);
}

fn close_tab(state_rc: &Rc<RefCell<BrowserState>>, tab_id: u32) {
    let mut next_active_id = None;
    let mut removed_container = None;
    let mut stack_opt = None;
    {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            stack_opt = Some(state.stack.clone());
            if let Some(idx) = state.tabs.iter().position(|t| t.id == tab_id) {
                let removed = state.tabs.remove(idx);
                if !removed.url.is_empty() && removed.url != "about:blank" {
                    state.closed_tabs.push((removed.url.clone(), removed.title.clone()));
                    if state.closed_tabs.len() > 50 {
                        state.closed_tabs.remove(0);
                    }
                }
                removed_container = Some(removed.container);

                if state.active_tab_id == Some(tab_id) {
                    if !state.tabs.is_empty() {
                        let new_idx = if idx > 0 { idx - 1 } else { 0 };
                        let new_id = state.tabs[new_idx].id;
                        next_active_id = Some(new_id);
                        state.active_tab_id = Some(new_id);
                    } else {
                        state.active_tab_id = None;
                    }
                } else {
                    next_active_id = state.active_tab_id;
                }
            }
        }
    }

    if let (Some(stack), Some(container)) = (stack_opt.as_ref(), removed_container) {
        stack.remove(&container);
    }

    if let Some(new_id) = next_active_id {
        if let Some(stack) = stack_opt {
            stack.set_visible_child_name(&format!("tab_{}", new_id));
        }
        sync_tabs_to_ui(state_rc);
    } else if open_windows_count() > 1 {
        if let Ok(state) = state_rc.try_borrow() {
            state.window.close();
        }
    } else {
        create_tab(state_rc, "ferro://newtab");
    }
}

fn schedule_prewarm_tab(state_rc: &Rc<RefCell<BrowserState>>) {
    let state_weak = Rc::downgrade(state_rc);
    glib::timeout_add_local_once(std::time::Duration::from_millis(140), move || {
        if let Some(s_rc) = state_weak.upgrade() {
            let needs_spare = s_rc.try_borrow().map(|st| st.spare_tab.is_none()).unwrap_or(false);
            if needs_spare {
                if let Some(spare) = build_raw_tab_webview(&s_rc, "ferro://newtab") {
                    if let Ok(mut st) = s_rc.try_borrow_mut() {
                        st.spare_tab = Some(spare);
                    }
                }
            }
        }
    });
}

fn create_tab(state_rc: &Rc<RefCell<BrowserState>>, raw_target_url: &str) -> u32 {
    let target_url: &str = match raw_target_url.trim() {
        "https://youtube.com" | "https://youtube.com/" | "http://youtube.com" | "http://youtube.com/" => {
            "https://www.youtube.com/"
        }
        other => other,
    };
    // Fast <1ms path: use pre-warmed spare tab when opening ferro://newtab!
    if target_url == "ferro://newtab" {
        let taken = {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.spare_tab.take().map(|spare| {
                    let s_json = serde_json::to_string(&state.settings).unwrap_or_else(|_| "{}".to_string());
                    let dz = state.settings.default_zoom.clamp(50, 200);
                    let id = spare.id;
                    let t_name = spare.tab_name.clone();
                    let wv = spare.webview.clone();
                    state.tabs.push(Tab {
                        id,
                        url: "ferro://newtab".to_string(),
                        title: "Новая вкладка".to_string(),
                        can_go_back: false,
                        can_go_forward: false,
                        is_loading: false,
                        is_pinned: false,
                        is_muted: false,
                        sleeping_url: None,
                        zoom_pct: dz,
                        last_media_sec: 0,
                        last_scroll_x: 0,
                        last_scroll_y: 0,
                        webview: spare.webview,
                        inner: spare.inner,
                        container: spare.container,
                    });
                    state.active_tab_id = Some(id);
                    state.stack.set_visible_child_name(&t_name);
                    (id, wv, s_json)
                })
            } else {
                None
            }
        };

        if let Some((id, wv, s_json)) = taken {
            let _ = wv.evaluate_script(&format!(
                "window._ferro_settings = {0}; if (window.applySettings) window.applySettings({0});",
                s_json
            ));
            sync_tabs_to_ui(state_rc);
            schedule_prewarm_tab(state_rc);
            return id;
        }
    }

    let Some(built) = build_raw_tab_webview(state_rc, target_url) else {
        return 0;
    };
    let tab_id = built.id;
    let tab_name = built.tab_name;

    {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            let dz = state.settings.default_zoom.clamp(50, 200);
            state.tabs.push(Tab {
                id: tab_id,
                url: target_url.to_string(),
                title: "Новая вкладка".to_string(),
                can_go_back: false,
                can_go_forward: false,
                is_loading: false,
                is_pinned: false,
                is_muted: false,
                sleeping_url: None,
                zoom_pct: dz,
                last_media_sec: 0,
                last_scroll_x: 0,
                last_scroll_y: 0,
                webview: built.webview,
                inner: built.inner,
                container: built.container,
            });
            state.active_tab_id = Some(tab_id);
            state.stack.set_visible_child_name(&tab_name);
        }
    }

    sync_tabs_to_ui(state_rc);
    schedule_prewarm_tab(state_rc);
    tab_id
}

fn create_detached_tab_in_window(
    state_rc: &Rc<RefCell<BrowserState>>,
    spec: &DetachedTabSpec,
) -> Option<webkit2gtk::WebView> {
    let target_url: &str = match spec.url.trim() {
        "https://youtube.com" | "https://youtube.com/" | "http://youtube.com" | "http://youtube.com/" => {
            "https://www.youtube.com/"
        }
        "" => "ferro://newtab",
        other => other,
    };

    let built = build_raw_tab_webview(state_rc, target_url)?;
    let tab_id = built.id;
    let tab_name = built.tab_name;
    let clamped_zoom = spec.zoom_pct.clamp(30, 300);
    built.inner.set_zoom_level((clamped_zoom as f64) / 100.0);
    if spec.is_muted {
        built.inner.set_is_muted(true);
    }

    if spec.media_sec > 1 || spec.scroll_x > 0 || spec.scroll_y > 0 {
        let sec = spec.media_sec;
        let sx = spec.scroll_x;
        let sy = spec.scroll_y;
        let applied = Rc::new(Cell::new(false));
        let wv_for_restore = built.webview.clone();
        built.inner.connect_load_changed(move |_wv, event| {
            if event == LoadEvent::Finished && !applied.get() {
                applied.set(true);
                let js = format!(
                    r#"(function(){{
                        if ({sx} > 0 || {sy} > 0) {{
                            window.scrollTo({sx}, {sy});
                            setTimeout(() => window.scrollTo({sx}, {sy}), 180);
                        }}
                        const seekTo = {sec};
                        if (seekTo > 1) {{
                            let tries = 0;
                            const iv = setInterval(() => {{
                                tries++;
                                const v = document.querySelector('video');
                                if (v && v.readyState >= 1) {{
                                    clearInterval(iv);
                                    if (Math.abs((v.currentTime || 0) - seekTo) > 2) {{
                                        try {{ v.currentTime = seekTo; }} catch(e) {{}}
                                    }}
                                    try {{ v.play(); }} catch(e) {{}}
                                }} else if (tries > 24) {{
                                    clearInterval(iv);
                                }}
                            }}, 120);
                        }}
                    }})();"#,
                    sx = sx,
                    sy = sy,
                    sec = sec
                );
                let _ = wv_for_restore.evaluate_script(&js);
            }
        });
    }

    let ret_inner = built.inner.clone();
    let initial_title = if spec.title.trim().is_empty() {
        "Новая вкладка".to_string()
    } else {
        spec.title.clone()
    };

    if let Ok(mut state) = state_rc.try_borrow_mut() {
        state.tabs.push(Tab {
            id: tab_id,
            url: target_url.to_string(),
            title: initial_title,
            can_go_back: false,
            can_go_forward: false,
            is_loading: !target_url.starts_with("ferro://"),
            is_pinned: false,
            is_muted: spec.is_muted,
            sleeping_url: None,
            zoom_pct: clamped_zoom,
            last_media_sec: spec.media_sec,
            last_scroll_x: spec.scroll_x,
            last_scroll_y: spec.scroll_y,
            webview: built.webview,
            inner: built.inner,
            container: built.container,
        });
        state.active_tab_id = Some(tab_id);
        state.stack.set_visible_child_name(&tab_name);
    }

    sync_tabs_to_ui(state_rc);
    schedule_prewarm_tab(state_rc);
    Some(ret_inner)
}

fn build_raw_tab_webview(state_rc: &Rc<RefCell<BrowserState>>, target_url: &str) -> Option<PrewarmedTab> {
    let (
        tab_id,
        web_ctx_rc,
        adblocker,
        native_filter,
        stack,
        autofill_script,
        dynamic_theme_css,
        settings_snapshot,
        settings_json,
        settings_dirty,
        bookmarks_dirty,
        history_dirty,
        passwords_dirty,
        downloads_store,
        source_cache,
        lens_preview,
        dl_attached_flag,
    ) = {
        if let Ok(state) = state_rc.try_borrow() {
            let id = NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed);
            let autofill = state.passwords.generate_autofill_script();
            let theme_css = state.settings.generate_css_variables();
            let s_snap = state.settings.clone();
            let s_json = serde_json::to_string(&state.settings).unwrap_or_else(|_| "{}".to_string());
            (
                id,
                state.web_context.clone(),
                state.adblocker.clone(),
                state.native_filter.borrow().clone(),
                state.stack.clone(),
                autofill,
                theme_css,
                s_snap,
                s_json,
                state.settings_dirty.clone(),
                state.bookmarks_dirty.clone(),
                state.history_dirty.clone(),
                state.passwords_dirty.clone(),
                state.downloads.clone(),
                state.page_source_cache.clone(),
                state.lens_preview.clone(),
                state.download_handler_attached.clone(),
            )
        } else {
            return None;
        }
    };

    let owner: Rc<RefCell<Weak<RefCell<BrowserState>>>> = Rc::new(RefCell::new(Rc::downgrade(state_rc)));

    let tab_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let tab_name = format!("tab_{}", tab_id);
    stack.add_named(&tab_box, &tab_name);

    let init_script = format!(
        r#"
        try {{ window._ferro_settings = {settings_json}; }} catch(e) {{}}
        (function() {{
            function injectDynamicTheme() {{
                if (window.location.protocol !== 'ferro:' || window.location.href.startsWith('ferro://settings')) return;
                const id = 'ferro-theme-vars';
                if (!document.getElementById(id)) {{
                    const root = document.head || document.documentElement;
                    if (root) {{
                        const style = document.createElement('style');
                        style.id = id;
                        style.textContent = `{dynamic_theme_css}`;
                        root.appendChild(style);
                    }}
                }}
            }}
            injectDynamicTheme();

            function injectCosmeticStyle() {{
                if (window.location.protocol === 'ferro:') return;
                if (/(^|\.)(youtube\.com|youtu\.be|googlevideo\.com)$/i.test(window.location.hostname || '')) return;
                const id = 'ferro-adblock-cosmetic';
                if (!document.getElementById(id)) {{
                    const root = document.head || document.documentElement;
                    if (root) {{
                        const style = document.createElement('style');
                        style.id = id;
                        style.textContent = `{cosmetic_css}`;
                        root.appendChild(style);
                    }}
                }}
            }}
            injectCosmeticStyle();

            if (document.readyState === 'loading') {{
                document.addEventListener('DOMContentLoaded', () => {{
                    injectDynamicTheme();
                    injectCosmeticStyle();
                }}, {{ once: true }});
            }}
        }})();
        if (window.location.protocol !== 'ferro:') {{
            try {{ {adblock_js} }} catch(e) {{}}
            try {{ {autofill_js} }} catch(e) {{}}
        }}
        try {{ {ctx_js} }} catch(e) {{}}
        try {{ {hotkeys_js} }} catch(e) {{}}
        "#,
        settings_json = settings_json,
        dynamic_theme_css = dynamic_theme_css,
        cosmetic_css = ADBLOCK_COSMETIC_CSS,
        adblock_js = ADBLOCK_JS,
        ctx_js = CONTEXT_MENU_JS,
        hotkeys_js = HOTKEYS_JS,
        autofill_js = autofill_script
    );

    let downloads_for_dl_handler = downloads_store.clone();
    let mut web_ctx = web_ctx_rc.borrow_mut();
    let protocol_already_registered = web_ctx.is_custom_protocol_registered("ferro");
    let mut builder = WebViewBuilder::new_with_web_context(&mut *web_ctx);
    if !protocol_already_registered {
        let blocked_counter = adblocker.blocked_counter();
        builder = builder.with_custom_protocol("ferro".into(), move |_id, req| {
            let uri = req.uri().to_string();
            let path_and_query = uri.strip_prefix("ferro://").unwrap_or(&uri);

            let (html, content_type) = if path_and_query.starts_with("tokens") {
                (TOKENS_HTML.to_string(), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("settings") {
                (SETTINGS_HTML.to_string(), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("lens-loading") {
                let b64 = lens_preview.lock().map(|g| g.clone()).unwrap_or_default();
                (render_lens_loading_html(&b64), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("bookmarks-panel") {
                let b = BookmarksData::load_or_default();
                (render_bookmarks_panel_html(&b), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("sleep") {
                let query = path_and_query.split('?').nth(1).unwrap_or("");
                let mut u = String::new();
                let mut t = String::from("Вкладка в спящем режиме");
                for pair in query.split('&') {
                    if let Some((k, v)) = pair.split_once('=') {
                        let dec = percent_encoding::percent_decode_str(v).decode_utf8_lossy().to_string();
                        match k {
                            "url" => u = dec,
                            "title" => t = dec,
                            _ => {}
                        }
                    }
                }
                (render_sleeping_tab_html(&u, &t), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("view-source") {
                let s = BrowserSettings::load_or_default();
                let (src_url, src_html) = source_cache
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_else(|_| ("about:blank".to_string(), String::new()));
                (render_source_viewer_html(&src_url, &src_html, &s), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("api/stats/get") {
                let h = HistoryData::load_or_default();
                let p = PasswordsData::load_or_default();
                let b = BookmarksData::load_or_default();
                let json = serde_json::json!({
                    "blocked_count": blocked_counter.load(Ordering::Relaxed),
                    "history_count": h.items.len(),
                    "passwords_count": p.items.len(),
                    "bookmarks_count": b.items.len(),
                });
                (json.to_string(), "application/json")
            } else if path_and_query.starts_with("api/settings/get") {
                let s = BrowserSettings::load_or_default();
                (serde_json::to_string(&s).unwrap_or_default(), "application/json")
            } else if path_and_query.starts_with("api/settings/save") {
                let body_bytes = req.body();
                if let Ok(json_str) = std::str::from_utf8(body_bytes) {
                    if let Ok(new_s) = serde_json::from_str::<BrowserSettings>(json_str) {
                        let _ = new_s.save();
                        settings_dirty.store(true, Ordering::SeqCst);
                    }
                }
                (r#"{"status":"saved"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/import/detect") {
                let list = BrowserImporter::detect_all();
                (serde_json::to_string(&list).unwrap_or_default(), "application/json")
            } else if path_and_query.starts_with("api/import/run") {
                let browser_id = path_and_query.split("browser=").nth(1).unwrap_or("chrome").split('&').next().unwrap_or("chrome");
                let res = BrowserImporter::execute_import(browser_id);
                bookmarks_dirty.store(true, Ordering::SeqCst);
                history_dirty.store(true, Ordering::SeqCst);
                (serde_json::to_string(&res).unwrap_or_default(), "application/json")
            } else if path_and_query.starts_with("api/import/dismiss") {
                let mut s = BrowserSettings::load_or_default();
                s.has_prompted_import = true;
                let _ = s.save();
                settings_dirty.store(true, Ordering::SeqCst);
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/session/detect") {
                let res = if let Some(session) = SessionData::load() {
                    let count = session
                        .tabs
                        .iter()
                        .filter(|t| t.url != "ferro://newtab")
                        .count();
                    if count > 0 {
                        format!(r#"{{"has_session":true,"tabs_count":{}}}"#, count)
                    } else {
                        r#"{"has_session":false,"tabs_count":0}"#.to_string()
                    }
                } else {
                    r#"{"has_session":false,"tabs_count":0}"#.to_string()
                };
                (res, "application/json")
            } else if path_and_query.starts_with("api/session/dismiss") {
                SessionData::clear();
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("downloads") {
                let html_out = downloads_store
                    .lock()
                    .map(|d| d.render_html())
                    .unwrap_or_default();
                (html_out, "text/html; charset=utf-8")
            } else if path_and_query.starts_with("api/downloads/get") {
                let json_out = downloads_store
                    .lock()
                    .ok()
                    .and_then(|d| serde_json::to_string(&d.items).ok())
                    .unwrap_or_else(|| "[]".to_string());
                (json_out, "application/json")
            } else if path_and_query.starts_with("api/downloads/delete") {
                if let Some(id_str) = path_and_query.split("id=").nth(1) {
                    if let Ok(id) = id_str.split('&').next().unwrap_or("").parse::<u64>() {
                        if let Ok(mut d) = downloads_store.lock() {
                            d.delete(id);
                        }
                    }
                }
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/downloads/clear") {
                if let Ok(mut d) = downloads_store.lock() {
                    d.clear();
                }
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("history") {
                let h = HistoryData::load_or_default();
                (h.render_html(), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("passwords") {
                let p = PasswordsData::load_or_default();
                (p.render_html(), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("api/history/delete") {
                if let Some(id_str) = path_and_query.split("id=").nth(1) {
                    if let Ok(id) = id_str.split('&').next().unwrap_or("").parse::<u64>() {
                        let mut h = HistoryData::load_or_default();
                        h.delete(id);
                        history_dirty.store(true, Ordering::SeqCst);
                    }
                }
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/history/clear") {
                let mut h = HistoryData::load_or_default();
                h.clear();
                history_dirty.store(true, Ordering::SeqCst);
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/passwords/delete") {
                if let Some(id_str) = path_and_query.split("id=").nth(1) {
                    if let Ok(id) = id_str.split('&').next().unwrap_or("").parse::<u64>() {
                        let mut p = PasswordsData::load_or_default();
                        p.delete(id);
                        passwords_dirty.store(true, Ordering::SeqCst);
                    }
                }
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/passwords/update") {
                let query = path_and_query.split('?').nth(1).unwrap_or("");
                let mut id_val: Option<u64> = None;
                let mut origin = String::new();
                let mut user = String::new();
                let mut pass = String::new();
                for pair in query.split('&') {
                    if let Some((k, v)) = pair.split_once('=') {
                        let decoded = percent_encoding::percent_decode_str(v)
                            .decode_utf8_lossy()
                            .to_string();
                        match k {
                            "id" => id_val = decoded.parse::<u64>().ok(),
                            "origin" => origin = decoded,
                            "user" => user = decoded,
                            "pass" => pass = decoded,
                            _ => {}
                        }
                    }
                }
                if let Some(id) = id_val {
                    let mut p = PasswordsData::load_or_default();
                    p.update_by_id(id, &origin, &user, &pass);
                    passwords_dirty.store(true, Ordering::SeqCst);
                }
                (r#"{"status":"updated"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/passwords/save") {
                let query = path_and_query.split('?').nth(1).unwrap_or("");
                let mut origin = String::new();
                let mut user = String::new();
                let mut pass = String::new();
                for pair in query.split('&') {
                    if let Some((k, v)) = pair.split_once('=') {
                        let decoded = percent_encoding::percent_decode_str(v)
                            .decode_utf8_lossy()
                            .to_string();
                        match k {
                            "origin" => origin = decoded,
                            "user" => user = decoded,
                            "pass" => pass = decoded,
                            _ => {}
                        }
                    }
                }
                if !origin.is_empty() && !pass.is_empty() {
                    let mut p = PasswordsData::load_or_default();
                    p.add(&origin, &user, &pass);
                    passwords_dirty.store(true, Ordering::SeqCst);
                }
                (r#"{"status":"saved"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/passwords/import") {
                let body_bytes = req.body();
                let mut imported = 0usize;
                if let Ok(csv_text) = std::str::from_utf8(body_bytes) {
                    let mut p = PasswordsData::load_or_default();
                    imported = p.import_chrome_csv(csv_text);
                    passwords_dirty.store(true, Ordering::SeqCst);
                }
                (format!(r#"{{"status":"ok","imported":{}}}"#, imported), "application/json")
            } else if path_and_query.starts_with("api/passwords/export") {
                let p = PasswordsData::load_or_default();
                (p.export_csv(), "text/csv; charset=utf-8")
            } else {
                (NEWTAB_HTML.to_string(), "text/html; charset=utf-8")
            };

            wry::http::Response::builder()
                .header("content-type", content_type)
                .header("access-control-allow-origin", "*")
                .header("access-control-allow-methods", "GET, POST, OPTIONS")
                .header("access-control-allow-headers", "content-type")
                .body(std::borrow::Cow::Owned(html.into_bytes()))
                .unwrap()
        });
    }

    let owner_for_ipc = owner.clone();
    let builder = builder
        .with_initialization_script(&init_script)
        .with_autoplay(true)
        .with_transparent(false)
        .with_ipc_handler(move |req| {
            match serde_json::from_str::<IpcMessage>(req.body()) {
                Ok(msg) => {
                    let cur_state = owner_for_ipc.borrow().upgrade();
                    if let Some(s_rc) = cur_state {
                        handle_ipc(&s_rc, msg);
                    }
                }
                Err(e) => eprintln!("[Ferro Tab IPC Parse Error] {:?}: {}", e, req.body()),
            }
        });

    let webview = match builder.build_gtk(&tab_box) {
        Ok(wv) => Rc::new(wv),
        Err(e) => {
            eprintln!("Failed to create tab webview: {:?}", e);
            return None;
        }
    };
    drop(web_ctx);

    let inner_wv = webview.webview();

    // Immediately set opaque theme surface background color on the WebKitGTK widget BEFORE loading URI
    // so WebKitGTK uses an opaque backing surface (zero per-frame alpha-blending overhead during 60fps video!)
    let mut tab_bg = compute_surface_rgba(&settings_snapshot);
    tab_bg.set_alpha(1.0);
    inner_wv.set_background_color(&tab_bg);
    inner_wv.set_zoom_level((settings_snapshot.default_zoom.clamp(50, 200) as f64) / 100.0);

    // Attach global WebKit download handler & WebBrowser cache model once on the shared WebContext
    if !dl_attached_flag.get() {
        if let Some(wk_ctx) = inner_wv.context() {
            wk_ctx.set_cache_model(CacheModel::WebBrowser);
            attach_download_handler(&wk_ctx, downloads_for_dl_handler);
            dl_attached_flag.set(true);
        }
    }

    // Set Chrome User-Agent, hardware acceleration, bfcache, DNS prefetch, fullscreen & user engine settings
    if let Some(settings) = WebViewExt::settings(&inner_wv) {
        settings.set_user_agent(Some(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        ));
        settings.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Always);
        settings.set_enable_page_cache(true);
        settings.set_enable_fullscreen(true);
        settings.set_enable_developer_extras(true);
        settings.set_enable_webgl(settings_snapshot.enable_webgl);
        settings.set_enable_smooth_scrolling(settings_snapshot.smooth_scrolling);
        let is_internal = target_url.starts_with("ferro://") || target_url.is_empty();
        settings.set_enable_javascript(is_internal || settings_snapshot.enable_javascript);
        settings.set_enable_javascript_markup(true);
        settings.set_enable_media_stream(true);
        settings.set_enable_mediasource(true);
        settings.set_enable_encrypted_media(true);
        settings.set_enable_webaudio(true);
        settings.set_media_playback_requires_user_gesture(false);
    }

    // Attach native WebKit uBlock content blocking filter and cosmetic user style sheet
    if let Some(ucm) = inner_wv.user_content_manager() {
        add_user_style_sheet(&ucm, ADBLOCK_COSMETIC_CSS);
        if adblocker.is_enabled() {
            if let Some(filter) = &native_filter {
                attach_to_ucm(&ucm, filter);
            }
        }
    }

    // Now load the target URL so that all initial requests are intercepted by filter
    if !target_url.is_empty() {
        inner_wv.load_uri(target_url);
    }

    // Hook WebKitGTK signals via dynamic `owner` so torn-off tabs seamlessly route to their new window
    let owner_load = owner.clone();
    let tab_id_copy = tab_id;
    inner_wv.connect_load_changed(move |wv, event| {
        if matches!(event, LoadEvent::Committed | LoadEvent::Finished) {
            wv.queue_draw();
        }
        let cur_state = owner_load.borrow().upgrade();
        if let Some(state_rc) = cur_state {
            let is_loading = matches!(event, LoadEvent::Started | LoadEvent::Redirected);
            let can_back = wv.can_go_back();
            let can_fwd = wv.can_go_forward();
            let is_finished = matches!(event, LoadEvent::Finished);
            {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_copy) {
                        tab.is_loading = is_loading;
                        tab.can_go_back = can_back;
                        tab.can_go_forward = can_fwd;
                    }
                }
            }
            sync_tab_update_to_ui(&state_rc, tab_id_copy);
            if is_finished {
                sync_adblock_to_ui(&state_rc);
                let uri = wv.uri().map(|u| u.to_string()).unwrap_or_default();
                let title = wv.title().map(|t| t.to_string()).unwrap_or_default();
                if !uri.is_empty() && !uri.starts_with("ferro://") && uri != "about:blank" {
                    let doorhanger_target = {
                        if let Ok(mut state) = state_rc.try_borrow_mut() {
                            state.history.add(&uri, &title);
                            if state.settings.offer_save_passwords {
                                if let Some((orig, usr, pwd)) = state.pending_password.clone() {
                                    if !state.passwords.is_already_saved(&orig, &usr, &pwd) {
                                        let is_update = state.passwords.items.iter().any(|p| {
                                            p.origin.eq_ignore_ascii_case(&orig)
                                                && p.username.eq_ignore_ascii_case(&usr)
                                        });
                                        let tab_wv = state
                                            .tabs
                                            .iter()
                                            .find(|t| t.id == tab_id_copy)
                                            .map(|t| t.webview.clone());
                                        tab_wv.map(|twv| {
                                            (twv, PasswordsData::build_doorhanger_js(&orig, &usr, &pwd, is_update))
                                        })
                                    } else {
                                        state.pending_password = None;
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    if let Some((twv, js)) = doorhanger_target {
                        let _ = twv.evaluate_script(&js);
                    }
                }
            }
        }
    });

    // True HTML5 / YouTube video fullscreen: hide top chrome on enter, restore on leave
    let owner_fs_in = owner.clone();
    inner_wv.connect_enter_fullscreen(move |_wv| {
        let cur_state = owner_fs_in.borrow().upgrade();
        if let Some(state_rc) = cur_state {
            if let Ok(state) = state_rc.try_borrow() {
                state.top_box.hide();
            }
        }
        false
    });
    let owner_fs_out = owner.clone();
    inner_wv.connect_leave_fullscreen(move |_wv| {
        let cur_state = owner_fs_out.borrow().upgrade();
        if let Some(state_rc) = cur_state {
            if let Ok(state) = state_rc.try_borrow() {
                state.top_box.show();
            }
        }
        false
    });

    let owner_uri = owner.clone();
    inner_wv.connect_uri_notify(move |wv| {
        let cur_state = owner_uri.borrow().upgrade();
        if let Some(state_rc) = cur_state {
            let new_uri = wv.uri().map(|u| u.to_string()).unwrap_or_default();
            {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    let allow_js = new_uri.starts_with("ferro://") || new_uri.is_empty() || state.settings.enable_javascript;
                    if let Some(wk_s) = WebViewExt::settings(wv) {
                        wk_s.set_enable_javascript(allow_js);
                    }
                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_copy) {
                        if !new_uri.starts_with("ferro://sleep") && !new_uri.is_empty() {
                            tab.sleeping_url = None;
                        }
                        tab.url = new_uri;
                    }
                }
            }
            sync_tab_update_to_ui(&state_rc, tab_id_copy);
        }
    });

    let owner_title = owner.clone();
    inner_wv.connect_title_notify(move |wv| {
        let cur_state = owner_title.borrow().upgrade();
        if let Some(state_rc) = cur_state {
            let new_title = wv.title().map(|t| t.to_string()).unwrap_or_else(|| "Новая вкладка".to_string());
            {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_copy) {
                        tab.title = new_title;
                    }
                }
            }
            sync_tab_update_to_ui(&state_rc, tab_id_copy);
        }
    });

    // Handle new window requests (target="_blank" or window.open) & block ad popups
    let owner_policy = owner.clone();
    let adblocker_policy = adblocker;
    inner_wv.connect_decide_policy(move |_wv, decision, decision_type| {
        match decision_type {
            PolicyDecisionType::NewWindowAction => {
                if let Ok(nav) = decision.clone().downcast::<NavigationPolicyDecision>() {
                    #[allow(deprecated)]
                    if let Some(req) = nav.request() {
                        if let Some(uri) = req.uri() {
                            let uri_str = uri.as_str();
                            if adblocker_policy.should_block_url(uri_str) {
                                adblocker_policy.increment_blocked();
                                decision.ignore();
                                return true;
                            }
                            let cur_state = owner_policy.borrow().upgrade();
                            if let Some(state_rc) = cur_state {
                                create_tab(&state_rc, uri_str);
                            }
                            decision.ignore();
                            return true;
                        }
                    }
                }
            }
            PolicyDecisionType::NavigationAction => {
                if let Ok(nav) = decision.clone().downcast::<NavigationPolicyDecision>() {
                    #[allow(deprecated)]
                    if let Some(req) = nav.request() {
                        if let Some(uri) = req.uri() {
                            let uri_str = uri.as_str();
                            let is_yt = uri_str.contains("youtube.com") || uri_str.contains("youtu.be") || uri_str.contains("googlevideo.com");
                            if !is_yt && adblocker_policy.should_block_url(uri_str) {
                                adblocker_policy.increment_blocked();
                                decision.ignore();
                                return true;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        false
    });

    // Suppress WebKitGTK native desktop context menu so Ferro custom in-browser menu is used
    inner_wv.connect_context_menu(|_wv, _menu, _event, _hit_test| {
        true
    });

    inner_wv.connect_create(|_wv, _action| None);

    tab_box.show_all();

    Some(PrewarmedTab {
        id: tab_id,
        tab_name,
        webview,
        inner: inner_wv,
        container: tab_box,
    })
}

fn fetch_sponsorblock_segments_json(video_id: &str) -> String {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    static SB_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = SB_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(cached) = guard.get(video_id) {
            return cached.clone();
        }
    }

    let safe_id: String = video_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(20)
        .collect();
    if safe_id.is_empty() {
        return "[]".to_string();
    }

    let url = format!(
        "https://sponsor.ajay.app/api/skipSegments?videoID={}&categories=%5B%22sponsor%22,%22selfpromo%22,%22interaction%22,%22intro%22,%22outro%22,%22preview%22,%22music_offtopic%22,%22filler%22%5D",
        safe_id
    );
    let out = std::process::Command::new("curl")
        .args(["-sS", "--max-time", "5", "-g", &url])
        .output();

    let json = match out {
        Ok(res) if res.status.success() => {
            let body = String::from_utf8_lossy(&res.stdout).trim().to_string();
            if body.starts_with('[') && body.ends_with(']') {
                body
            } else {
                "[]".to_string()
            }
        }
        _ => "[]".to_string(),
    };

    if let Ok(mut guard) = cache.lock() {
        if guard.len() > 256 {
            guard.clear();
        }
        guard.insert(safe_id, json.clone());
    }
    json
}

fn spawn_sponsorblock_fetch(state_rc: &Rc<RefCell<BrowserState>>, video_id: String) {
    let safe_id: String = video_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(20)
        .collect();
    if safe_id.is_empty() {
        return;
    }

    let state_weak = Rc::downgrade(state_rc);
    let id_for_thread = safe_id.clone();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let json = fetch_sponsorblock_segments_json(&id_for_thread);
        let _ = tx.send(json);
    });

    glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
        match rx.try_recv() {
            Ok(segments_json) => {
                if let Some(s_rc) = state_weak.upgrade() {
                    let wvs: Vec<Rc<WebView>> = s_rc
                        .try_borrow()
                        .map(|st| {
                            let mut list: Vec<Rc<WebView>> =
                                st.tabs.iter().map(|t| t.webview.clone()).collect();
                            if let Some((_, pip_wv, _)) = &st.pip_window {
                                list.push(pip_wv.clone());
                            }
                            list
                        })
                        .unwrap_or_default();
                    let js = format!(
                        "if (window._ferroReceiveSponsorBlock) window._ferroReceiveSponsorBlock({:?}, {});",
                        safe_id, segments_json
                    );
                    for wv in wvs {
                        let _ = wv.evaluate_script(&js);
                    }
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}

fn spawn_lens_background_search(
    state_rc: &Rc<RefCell<BrowserState>>,
    b64_img: String,
    fallback_url: String,
) {
    // If we already have base64 image bytes (from Wayland WebKit snapshot or canvas),
    // submit immediately inside side_webview with base_uri = "https://lens.google.com/"
    // so both Origin AND the user's Google Account cookies match 100%!
    let clean = b64_img.split(',').last().unwrap_or(&b64_img).trim().to_string();
    if clean.len() > 24 {
        ensure_and_open_side_panel(state_rc, "lens", "");
        let side_wv = state_rc.try_borrow().ok().and_then(|st| st.side_webview.clone());
        if let Some(wv) = side_wv {
            let html = render_lens_in_session_upload_html(&clean, &fallback_url);
            wv.webview().load_html(&html, Some("https://lens.google.com/"));
        }
        return;
    }

    // External image URL fallback: show loading preview, download image bytes in background,
    // then submit inside side_webview's Google session!
    ensure_and_open_side_panel(state_rc, "lens", "ferro://lens-loading");
    let state_weak = Rc::downgrade(state_rc);
    let fb_copy = fallback_url.clone();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let dl_b64 = download_image_bytes(&fb_copy)
            .map(|bytes| base64_encode(&bytes))
            .unwrap_or_default();
        let _ = tx.send(dl_b64);
    });

    glib::timeout_add_local(std::time::Duration::from_millis(45), move || {
        match rx.try_recv() {
            Ok(dl_b64) => {
                if let Some(s_rc) = state_weak.upgrade() {
                    let side_wv = s_rc.try_borrow().ok().and_then(|st| st.side_webview.clone());
                    if let Some(wv) = side_wv {
                        let html = render_lens_in_session_upload_html(&dl_b64, &fallback_url);
                        wv.webview().load_html(&html, Some("https://lens.google.com/"));
                    }
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}

fn open_floating_pip_window(state_rc: &Rc<RefCell<BrowserState>>, target_url: &str, start_sec: u32) {
    let origin_tab_id = {
        if let Ok(mut st) = state_rc.try_borrow_mut() {
            if let Some((old_win, _, _)) = st.pip_window.take() {
                old_win.close();
            }
            st.active_tab_id
        } else {
            return;
        }
    };

    let pip_win = gtk::Window::new(gtk::WindowType::Toplevel);
    pip_win.set_title("Картинка в картинке — Ferro");
    pip_win.set_default_size(480, 270);
    pip_win.set_decorated(false);
    pip_win.set_keep_above(true);
    pip_win.stick();
    pip_win.set_resizable(true);
    pip_win.set_type_hint(gdk::WindowTypeHint::Utility);

    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pip_win.add(&root_box);

    let pip_init_js = format!(
        r##"
        window._ferro_is_pip_window = true;
        try {{
            const blockedRe = /av01/i;
            if (window.MediaSource && typeof window.MediaSource.isTypeSupported === 'function') {{
                const origSup = window.MediaSource.isTypeSupported.bind(window.MediaSource);
                window.MediaSource.isTypeSupported = function(m) {{
                    if (typeof m === 'string' && blockedRe.test(m)) return false;
                    return origSup(m);
                }};
            }}
            if (window.HTMLMediaElement && window.HTMLMediaElement.prototype.canPlayType) {{
                const origCan = window.HTMLMediaElement.prototype.canPlayType;
                window.HTMLMediaElement.prototype.canPlayType = function(m) {{
                    if (typeof m === 'string' && blockedRe.test(m)) return '';
                    return origCan.call(this, m);
                }};
            }}
        }} catch(e) {{}}

        (function() {{
            const startAt = {start_sec};
            let seekApplied = false;
            let wasInAd = false;

            function installPipUi() {{
                const root = document.head || document.documentElement;
                if (root && !document.getElementById('ferro-pip-full-css')) {{
                    const st = document.createElement('style');
                    st.id = 'ferro-pip-full-css';
                    st.textContent = `
                        html, body {{
                            overflow: hidden !important;
                            background: #000 !important;
                            margin: 0 !important;
                            padding: 0 !important;
                        }}
                        #masthead-container, ytd-masthead, #secondary, #below, #comments, #chat, ytd-merch-shelf-renderer, #cinematics, #cinematics-container {{
                            display: none !important;
                        }}
                        #movie_player, .html5-video-player {{
                            position: fixed !important;
                            inset: 0 !important;
                            width: 100vw !important;
                            height: 100vh !important;
                            z-index: 2147483640 !important;
                            background: #000 !important;
                        }}
                        .html5-video-container, video.html5-main-video, body > video {{
                            position: fixed !important;
                            inset: 0 !important;
                            left: 0 !important;
                            top: 0 !important;
                            width: 100vw !important;
                            height: 100vh !important;
                            object-fit: contain !important;
                            background: #000 !important;
                        }}
                    `;
                    root.appendChild(st);
                }}

                if (document.body && !document.getElementById('ferro-pip-topbar-host')) {{
                    const host = document.createElement('div');
                    host.id = 'ferro-pip-topbar-host';
                    host.style.cssText = 'position:fixed;top:0;left:0;right:0;height:36px;z-index:2147483647;pointer-events:none;';
                    const sh = host.attachShadow({{ mode: 'open' }});
                    sh.innerHTML = `
                        <style>
                            .bar {{
                                display: flex;
                                align-items: center;
                                justify-content: space-between;
                                height: 34px;
                                padding: 0 8px 0 12px;
                                background: linear-gradient(180deg, rgba(12,12,16,0.88) 0%, rgba(12,12,16,0.45) 75%, transparent 100%);
                                color: #f4f4f7;
                                font-family: 'Inter', -apple-system, sans-serif;
                                font-size: 11.5px;
                                opacity: 0;
                                pointer-events: none;
                                transition: opacity 150ms ease;
                                user-select: none;
                            }}
                            .bar.show {{
                                opacity: 1;
                                pointer-events: auto;
                            }}
                            .drag {{
                                flex: 1;
                                height: 100%;
                                display: flex;
                                align-items: center;
                                cursor: grab;
                                font-weight: 500;
                                color: rgba(244,244,247,0.88);
                            }}
                            .btns {{ display: flex; align-items: center; gap: 5px; }}
                            button {{
                                height: 24px;
                                padding: 0 9px;
                                border-radius: 6px;
                                border: 1px solid rgba(255,255,255,0.2);
                                background: rgba(24,24,30,0.82);
                                color: #f4f4f7;
                                font-size: 11px;
                                font-weight: 500;
                                cursor: pointer;
                            }}
                            button:hover {{ background: rgba(59,130,246,0.88); border-color: rgba(147,197,253,0.7); }}
                            button.cls:hover {{ background: #c93b3b; border-color: #ef4444; }}
                        </style>
                        <div class="bar" id="pipBar">
                            <div class="drag" id="pipDrag">Картинка в картинке</div>
                            <div class="btns">
                                <button type="button" id="pipBack">Вернуть во вкладку</button>
                                <button type="button" class="cls" id="pipClose">✕</button>
                            </div>
                        </div>
                    `;
                    const bar = sh.getElementById('pipBar');
                    let hideT = null;
                    window.addEventListener('mousemove', () => {{
                        bar.classList.add('show');
                        clearTimeout(hideT);
                        hideT = setTimeout(() => bar.classList.remove('show'), 1700);
                    }}, {{ passive: true }});

                    let dDown = false, sx = 0, sy = 0;
                    sh.getElementById('pipDrag').addEventListener('mousedown', (e) => {{
                        if (e.button === 0) {{ dDown = true; sx = e.screenX; sy = e.screenY; }}
                    }});
                    window.addEventListener('mousemove', (e) => {{
                        if (dDown && Math.hypot(e.screenX - sx, e.screenY - sy) >= 4) {{
                            dDown = false;
                            if (window.ipc) window.ipc.postMessage(JSON.stringify({{ action: 'pip_drag' }}));
                        }}
                    }});
                    window.addEventListener('mouseup', () => {{ dDown = false; }});

                    sh.getElementById('pipBack').addEventListener('click', () => {{
                        const v = document.querySelector('video');
                        const sec = Math.max(0, Math.floor((v && v.currentTime) || 0));
                        if (window.ipc) window.ipc.postMessage(JSON.stringify({{ action: 'pip_return_to_tab', id: sec }}));
                    }});
                    sh.getElementById('pipClose').addEventListener('click', () => {{
                        if (window.ipc) window.ipc.postMessage(JSON.stringify({{ action: 'pip_close' }}));
                    }});
                    document.body.appendChild(host);
                }}
            }}

            setInterval(() => {{
                installPipUi();
                const player = document.getElementById('movie_player');
                const vid = document.querySelector('video');
                const isAd = Boolean(player && (player.classList.contains('ad-showing') || player.classList.contains('ad-interrupting')));
                if (isAd) {{
                    wasInAd = true;
                    if (vid) vid.muted = true;
                    const skip = document.querySelector('.ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern');
                    if (skip) skip.click();
                    if (vid && Number.isFinite(vid.duration) && vid.duration > 0.5 && vid.duration < 300) {{
                        vid.currentTime = vid.duration;
                    }}
                }} else {{
                    if (wasInAd && vid) {{
                        wasInAd = false;
                        vid.muted = false;
                        if (vid.playbackRate > 2) vid.playbackRate = 1;
                    }}
                    if (vid && !seekApplied && startAt > 1 && vid.readyState >= 1) {{
                        seekApplied = true;
                        if (Math.abs((vid.currentTime || 0) - startAt) > 3) {{
                            try {{ vid.currentTime = startAt; }} catch(e) {{}}
                        }}
                        try {{ vid.play(); }} catch(e) {{}}
                    }}
                }}
            }}, 250);

            if (document.readyState === 'loading') {{
                document.addEventListener('DOMContentLoaded', installPipUi, {{ once: true }});
            }} else {{
                installPipUi();
            }}
        }})();
        "##,
        start_sec = start_sec
    );

    let web_ctx_rc = match state_rc.try_borrow() {
        Ok(s) => s.web_context.clone(),
        Err(_) => return,
    };

    let state_for_pip_ipc = state_rc.clone();
    let pip_wv_res = {
        let mut web_ctx = web_ctx_rc.borrow_mut();
        WebViewBuilder::new_with_web_context(&mut *web_ctx)
            .with_autoplay(true)
            .with_initialization_script(&pip_init_js)
            .with_url(target_url)
            .with_ipc_handler(move |req| {
                if let Ok(msg) = serde_json::from_str::<IpcMessage>(req.body()) {
                    handle_ipc(&state_for_pip_ipc, msg);
                }
            })
            .build_gtk(&root_box)
    };

    let Ok(pip_wv) = pip_wv_res else { return };
    let inner = pip_wv.webview();
    inner.set_background_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
    if let Some(wk_s) = WebViewExt::settings(&inner) {
        wk_s.set_user_agent(Some(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        ));
        wk_s.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Always);
        wk_s.set_enable_javascript(true);
        wk_s.set_enable_mediasource(true);
        wk_s.set_enable_encrypted_media(true);
        wk_s.set_enable_webaudio(true);
        wk_s.set_media_playback_requires_user_gesture(false);
    }

    let pip_rc = Rc::new(pip_wv);
    if let Ok(mut st_mut) = state_rc.try_borrow_mut() {
        st_mut.pip_window = Some((pip_win.clone(), pip_rc, origin_tab_id));
    }

    let state_weak = Rc::downgrade(state_rc);
    pip_win.connect_delete_event(move |_, _| {
        if let Some(s_rc) = state_weak.upgrade() {
            if let Ok(mut st) = s_rc.try_borrow_mut() {
                st.pip_window = None;
            }
        }
        glib::Propagation::Proceed
    });

    pip_win.show_all();
    pip_win.present();
}

fn save_current_session(state_rc: &Rc<RefCell<BrowserState>>) {
    if let Ok(state) = state_rc.try_borrow() {
        let tabs: Vec<SessionTab> = state
            .tabs
            .iter()
            .map(|t| SessionTab {
                url: t.url.clone(),
                title: t.title.clone(),
            })
            .collect();
        let _ = SessionData::save(&tabs);
    }
}

fn handle_ipc(state_rc: &Rc<RefCell<BrowserState>>, msg: IpcMessage) {
    match msg.action.as_str() {
        "ui_ready" => {
            let top_settings = state_rc.try_borrow().ok().map(|st| {
                update_gtk_menu_css(&st.css_provider, &st.settings);
                let top_h = if st.settings.show_bookmarks_bar { 102 } else { 74 };
                st.top_box.set_size_request(-1, top_h);
                let surface_rgba = compute_surface_rgba(&st.settings);
                if let Some(top) = &st.top_webview {
                    top.webview().set_background_color(&surface_rgba);
                }
                (
                    serde_json::to_string(&st.settings).unwrap_or_else(|_| "{}".to_string()),
                    st.top_webview.clone(),
                    st.dark_mode,
                )
            });
            if let Some((s_json, Some(top), dark_mode)) = top_settings {
                let _ = top.evaluate_script(&format!(
                    "if (window.ferro && window.ferro.applySettings) {{ window.ferro.applySettings({0}); }}",
                    s_json
                ));
                let _ = top.evaluate_script(&format!(
                    "if (window.ferro) {{ window.ferro.setDarkModeStatus({}); }}",
                    dark_mode
                ));
            }
            sync_tabs_to_ui(state_rc);
            sync_bookmarks_to_ui(state_rc);
            sync_adblock_to_ui(state_rc);
        }
        "sync_settings" => {
            broadcast_settings_from_disk();
        }
        "save_settings" => {
            if let Some(new_s) = msg.settings {
                let _ = new_s.save();
            }
            broadcast_settings_from_disk();
        }
        "reload_bookmarks" => {
            broadcast_bookmarks_from_disk();
        }
        "new_tab" => {
            let url = msg.url.as_deref().unwrap_or("ferro://newtab");
            create_tab(state_rc, url);
        }
        "new_window" => {
            let core_opt = state_rc.try_borrow().ok().map(|s| s.core.clone());
            if let Some(core) = core_opt {
                glib::idle_add_local_once(move || {
                    let new_win = spawn_browser_window(&core, InitialWindowMode::NewBlankWindow);
                    if let Ok(st) = new_win.try_borrow() {
                        st.window.present();
                    };
                });
            }
        }
        "snapshot_tab_state" => {
            if let Some(tid) = msg.id {
                let wv_opt = state_rc
                    .try_borrow()
                    .ok()
                    .and_then(|s| s.tabs.iter().find(|t| t.id == tid).map(|t| t.webview.clone()));
                if let Some(wv) = wv_opt {
                    let js = format!(
                        r#"(function(){{
                            try {{
                                const v = document.querySelector('video');
                                const sec = v ? Math.max(0, Math.floor(v.currentTime || 0)) : 0;
                                const sx = Math.max(0, Math.round(window.scrollX || 0));
                                const sy = Math.max(0, Math.round(window.scrollY || 0));
                                if (window.ipc) {{
                                    window.ipc.postMessage(JSON.stringify({{
                                        action: 'report_tab_state',
                                        id: {tid},
                                        pass: sec + ',' + sx + ',' + sy
                                    }}));
                                }}
                            }} catch(e) {{}}
                        }})();"#,
                        tid = tid
                    );
                    let _ = wv.evaluate_script(&js);
                }
            }
        }
        "report_tab_state" => {
            if let (Some(tid), Some(raw)) = (msg.id, msg.pass) {
                let mut parts = raw.split(',');
                let sec = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                let sx = parts.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
                let sy = parts.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
                if let Ok(mut st) = state_rc.try_borrow_mut() {
                    if let Some(tab) = st.tabs.iter_mut().find(|t| t.id == tid) {
                        tab.last_media_sec = sec;
                        tab.last_scroll_x = sx;
                        tab.last_scroll_y = sy;
                    }
                }
            }
        }
        "detach_tab_to_window" => {
            if let Some(tab_id) = msg.id {
                let is_drag = msg.pass.as_deref() == Some("drag");
                let detach_payload = {
                    if let Ok(mut state) = state_rc.try_borrow_mut() {
                        if state.tabs.len() <= 1 {
                            None
                        } else if let Some(idx) = state.tabs.iter().position(|t| t.id == tab_id) {
                            let removed = state.tabs.remove(idx);
                            let live_uri = removed.inner.uri().map(|u| u.to_string()).unwrap_or_default();
                            let effective_url = if let Some(su) = &removed.sleeping_url {
                                su.clone()
                            } else if !live_uri.is_empty() && live_uri != "about:blank" {
                                live_uri
                            } else {
                                removed.url.clone()
                            };
                            let live_title = removed.inner.title().map(|t| t.to_string()).unwrap_or_default();
                            let effective_title = if !live_title.is_empty() {
                                live_title
                            } else {
                                removed.title.clone()
                            };

                            let spec = DetachedTabSpec {
                                url: effective_url,
                                title: effective_title,
                                is_muted: removed.is_muted,
                                zoom_pct: removed.zoom_pct,
                                media_sec: removed.last_media_sec,
                                scroll_x: removed.last_scroll_x,
                                scroll_y: removed.last_scroll_y,
                                size: state.window.size(),
                                start_move_drag: is_drag,
                                press_time: state.last_btn1_press_time.get(),
                            };

                            let src_stack = state.stack.clone();
                            let core = state.core.clone();
                            let top_wv = state.top_webview.clone();

                            let next_active = if state.active_tab_id == Some(tab_id) {
                                let new_idx = if idx > 0 { idx - 1 } else { 0 };
                                let new_id = state.tabs[new_idx].id;
                                state.active_tab_id = Some(new_id);
                                Some(new_id)
                            } else {
                                state.active_tab_id
                            };

                            Some((removed.container, spec, src_stack, next_active, core, top_wv))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some((old_container, spec, src_stack, next_active, core, top_wv)) = detach_payload {
                    // Cleanly destroy old window's WebView widget and build a fresh Wayland-safe WebView in the new window
                    src_stack.remove(&old_container);
                    if let Some(top) = top_wv {
                        top.webview().grab_remove();
                    }
                    if let Some(new_id) = next_active {
                        src_stack.set_visible_child_name(&format!("tab_{}", new_id));
                    }
                    sync_tabs_to_ui(state_rc);

                    glib::idle_add_local_once(move || {
                        let new_win = spawn_browser_window(&core, InitialWindowMode::DetachedTab(spec));
                        if let Ok(st) = new_win.try_borrow() {
                            st.window.present();
                        };
                    });
                }
            }
        }
        "move_tab_to_other_window" => {
            if let Some(tab_id) = msg.id {
                let target_win = open_windows_snapshot()
                    .into_iter()
                    .find(|w| !Rc::ptr_eq(w, state_rc));
                if let Some(dest_win) = target_win {
                    let move_payload = {
                        if let Ok(mut state) = state_rc.try_borrow_mut() {
                            if let Some(idx) = state.tabs.iter().position(|t| t.id == tab_id) {
                                let removed = state.tabs.remove(idx);
                                let live_uri = removed.inner.uri().map(|u| u.to_string()).unwrap_or_default();
                                let effective_url = if let Some(su) = &removed.sleeping_url {
                                    su.clone()
                                } else if !live_uri.is_empty() && live_uri != "about:blank" {
                                    live_uri
                                } else {
                                    removed.url.clone()
                                };
                                let live_title = removed.inner.title().map(|t| t.to_string()).unwrap_or_default();
                                let effective_title = if !live_title.is_empty() {
                                    live_title
                                } else {
                                    removed.title.clone()
                                };
                                let spec = DetachedTabSpec {
                                    url: effective_url,
                                    title: effective_title,
                                    is_muted: removed.is_muted,
                                    zoom_pct: removed.zoom_pct,
                                    media_sec: removed.last_media_sec,
                                    scroll_x: removed.last_scroll_x,
                                    scroll_y: removed.last_scroll_y,
                                    size: state.window.size(),
                                    start_move_drag: false,
                                    press_time: 0,
                                };
                                let src_stack = state.stack.clone();
                                let src_empty = state.tabs.is_empty();
                                let next_active = if !src_empty {
                                    let new_idx = if idx > 0 { idx - 1 } else { 0 };
                                    let new_id = state.tabs[new_idx].id;
                                    state.active_tab_id = Some(new_id);
                                    Some(new_id)
                                } else {
                                    state.active_tab_id = None;
                                    None
                                };
                                Some((removed.container, spec, src_stack, src_empty, next_active))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    if let Some((old_container, spec, src_stack, src_empty, next_active)) = move_payload {
                        src_stack.remove(&old_container);
                        let _ = create_detached_tab_in_window(&dest_win, &spec);
                        if let Ok(dst) = dest_win.try_borrow() {
                            dst.window.present();
                        }
                        if src_empty {
                            if let Ok(src_st) = state_rc.try_borrow() {
                                src_st.window.close();
                            }
                        } else {
                            if let Some(new_id) = next_active {
                                src_stack.set_visible_child_name(&format!("tab_{}", new_id));
                            }
                            sync_tabs_to_ui(state_rc);
                        }
                    }
                }
            }
        }
        "close_tab" => {
            if let Some(id) = msg.id {
                close_tab(state_rc, id);
            }
        }
        "close_active_tab" => {
            let active_id = state_rc.try_borrow().ok().and_then(|s| s.active_tab_id);
            if let Some(id) = active_id {
                close_tab(state_rc, id);
            }
        }
        "reopen_closed_tab" => {
            let last_closed = {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    state.closed_tabs.pop()
                } else {
                    None
                }
            };
            if let Some((url, _title)) = last_closed {
                create_tab(state_rc, &url);
            } else if let Some(session) = SessionData::load() {
                if let Some(tab) = session.tabs.last() {
                    create_tab(state_rc, &tab.url);
                }
            }
        }
        "prev_tab" => {
            let target_id = {
                if let Ok(state) = state_rc.try_borrow() {
                    if state.tabs.len() > 1 {
                        state.active_tab_id.and_then(|active_id| {
                            state.tabs.iter().position(|t| t.id == active_id).map(|pos| {
                                let prev_idx = if pos == 0 { state.tabs.len() - 1 } else { pos - 1 };
                                state.tabs[prev_idx].id
                            })
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(id) = target_id {
                switch_tab(state_rc, id);
            }
        }
        "next_tab" => {
            let target_id = {
                if let Ok(state) = state_rc.try_borrow() {
                    if state.tabs.len() > 1 {
                        state.active_tab_id.and_then(|active_id| {
                            state.tabs.iter().position(|t| t.id == active_id).map(|pos| {
                                let next_idx = (pos + 1) % state.tabs.len();
                                state.tabs[next_idx].id
                            })
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(id) = target_id {
                switch_tab(state_rc, id);
            }
        }
        "switch_tab_index" => {
            if let Some(idx_1based) = msg.id {
                let target_id = {
                    if let Ok(state) = state_rc.try_borrow() {
                        let idx = (idx_1based as usize).saturating_sub(1);
                        state.tabs.get(idx).map(|t| t.id)
                    } else {
                        None
                    }
                };
                if let Some(id) = target_id {
                    switch_tab(state_rc, id);
                }
            }
        }
        "focus_omnibox" => {
            send_to_ui(
                state_rc,
                "const inp = document.getElementById('omniboxInput'); if (inp) { inp.focus(); inp.select(); }",
            );
        }
        "toggle_bookmark_active" => {
            let active = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| (t.url.clone(), t.title.clone()))
                    })
                } else {
                    None
                }
            };
            if let Some((url, title)) = active {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    state.bookmarks.toggle(&url, &title);
                    state.bookmarks.save();
                }
                broadcast_bookmarks_from_disk();
            }
        }
        "restore_session" => {
            if let Some(session) = SessionData::load() {
                let single_blank_id = {
                    if let Ok(state) = state_rc.try_borrow() {
                        if state.tabs.len() == 1 && state.tabs[0].url == "ferro://newtab" {
                            Some(state.tabs[0].id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                for tab in session.tabs {
                    create_tab(state_rc, &tab.url);
                }
                if let Some(blank_id) = single_blank_id {
                    close_tab(state_rc, blank_id);
                }
                SessionData::clear();
            }
        }
        "dismiss_session" => {
            SessionData::clear();
        }
        "switch_tab" => {
            if let Some(id) = msg.id {
                switch_tab(state_rc, id);
            }
        }
        "navigate" => {
            if let Some(raw_url) = msg.url {
                let url = match raw_url.trim() {
                    "https://youtube.com" | "https://youtube.com/" | "http://youtube.com" | "http://youtube.com/" => {
                        "https://www.youtube.com/".to_string()
                    }
                    other => other.to_string(),
                };
                let mut target_tab_id = None;
                let active = {
                    if let Ok(mut state) = state_rc.try_borrow_mut() {
                        if let Some(id) = state.active_tab_id {
                            if let Some(t) = state.tabs.iter_mut().find(|t| t.id == id) {
                                t.url = url.clone();
                                t.is_loading = !url.starts_with("ferro://");
                                t.sleeping_url = None;
                                target_tab_id = Some(id);
                                Some((t.webview.clone(), t.inner.clone(), t.container.clone(), state.stack.clone()))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some(tid) = target_tab_id {
                    sync_tab_update_to_ui(state_rc, tid);
                }
                if let Some((wry_wv, inner, container, stack)) = active {
                    glib::idle_add_local_once(move || {
                        container.show_all();
                        if let Some(tid) = target_tab_id {
                            stack.set_visible_child_name(&format!("tab_{}", tid));
                        }
                        let _ = wry_wv.load_url(&url);
                        if !url.starts_with("ferro://") {
                            inner.grab_focus();
                        }
                        inner.queue_draw();
                    });
                }
            }
        }
        "go_back" => {
            let inner = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                    })
                } else {
                    None
                }
            };
            if let Some(inner) = inner {
                if inner.can_go_back() {
                    inner.go_back();
                }
            }
        }
        "go_forward" => {
            let inner = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                    })
                } else {
                    None
                }
            };
            if let Some(inner) = inner {
                if inner.can_go_forward() {
                    inner.go_forward();
                }
            }
        }
        "reload" => {
            let inner = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                    })
                } else {
                    None
                }
            };
            if let Some(inner) = inner {
                inner.reload();
            }
        }
        "go_home" => {
            let home_target = {
                if let Ok(state) = state_rc.try_borrow() {
                    let h = state.settings.homepage_url.clone();
                    let target_url = if h.trim().is_empty() { "ferro://newtab".to_string() } else { h };
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| (target_url, t.webview.clone(), t.inner.clone()))
                    })
                } else {
                    None
                }
            };
            if let Some((home_url, wry_wv, inner)) = home_target {
                if home_url.starts_with("ferro://") {
                    let _ = wry_wv.load_url(&home_url);
                } else {
                    inner.load_uri(&home_url);
                }
            }
        }
        "toggle_bookmark" => {
            if let Some(url) = msg.url {
                let title = msg.title.unwrap_or_else(|| url.clone());
                {
                    if let Ok(mut state) = state_rc.try_borrow_mut() {
                        state.bookmarks.toggle(&url, &title);
                    }
                }
                broadcast_bookmarks_from_disk();
            }
        }
        "translate" => {
            let wry_wv = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone())
                    })
                } else {
                    None
                }
            };
            if let Some(wry_wv) = wry_wv {
                let _ = wry_wv.evaluate_script(TRANSLATE_PAGE_JS);
            }
        }
        "toggle_dark_mode" => {
            let (new_dark_state, wry_wv) = {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    state.dark_mode = !state.dark_mode;
                    let active = state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone())
                    });
                    (Some(state.dark_mode), active)
                } else {
                    (None, None)
                }
            };
            if let Some(wry_wv) = wry_wv {
                let _ = wry_wv.evaluate_script(TOGGLE_DARK_MODE_JS);
            }
            if let Some(new_dark_state) = new_dark_state {
                send_to_ui(
                    state_rc,
                    &format!("if (window.ferro) {{ window.ferro.setDarkModeStatus({}); }}", new_dark_state),
                );
            }
        }
        "toggle_adblock" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                let enabled = state.adblocker.toggle_enabled();
                state.settings.adblock_enabled = enabled;
                let _ = state.settings.save();
            }
            broadcast_settings_from_disk();
        }
        "toggle_bookmarks_bar" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.settings.show_bookmarks_bar = !state.settings.show_bookmarks_bar;
                let _ = state.settings.save();
            }
            broadcast_settings_from_disk();
        }
        "zoom_step" => {
            let step_up = msg.id.unwrap_or(1) == 1;
            let mut updated_id = None;
            {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    if let Some(active_id) = state.active_tab_id {
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == active_id) {
                            let cur = tab.inner.zoom_level();
                            let next = if step_up { (cur + 0.1).min(3.0) } else { (cur - 0.1).max(0.3) };
                            tab.inner.set_zoom_level(next);
                            tab.zoom_pct = (next * 100.0).round() as u32;
                            updated_id = Some(active_id);
                        }
                    }
                }
            }
            if let Some(tid) = updated_id {
                sync_tab_update_to_ui(state_rc, tid);
            }
        }
        "zoom_reset" => {
            let mut updated_id = None;
            {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    let clamped = state.settings.default_zoom.clamp(50, 200);
                    let dz = (clamped as f64) / 100.0;
                    if let Some(active_id) = state.active_tab_id {
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == active_id) {
                            tab.inner.set_zoom_level(dz);
                            tab.zoom_pct = clamped;
                            updated_id = Some(active_id);
                        }
                    }
                }
            }
            if let Some(tid) = updated_id {
                sync_tab_update_to_ui(state_rc, tid);
            }
        }
        "stage_password" => {
            if let (Some(origin), Some(pass)) = (msg.url, msg.pass) {
                let user = msg.title.unwrap_or_default();
                if !origin.is_empty() && !pass.is_empty() {
                    let doorhanger = {
                        if let Ok(mut state) = state_rc.try_borrow_mut() {
                            if state.settings.offer_save_passwords
                                && !state.passwords.is_already_saved(&origin, &user, &pass)
                            {
                                let is_update = state.passwords.items.iter().any(|p| {
                                    p.origin.eq_ignore_ascii_case(&origin)
                                        && p.username.eq_ignore_ascii_case(&user)
                                });
                                state.pending_password = Some((origin.clone(), user.clone(), pass.clone()));
                                let js = PasswordsData::build_doorhanger_js(&origin, &user, &pass, is_update);
                                state.active_tab_id.and_then(|id| {
                                    state.tabs.iter().find(|t| t.id == id).map(|t| (t.webview.clone(), js))
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    if let Some((wv, js)) = doorhanger {
                        let _ = wv.evaluate_script(&js);
                    }
                }
            }
        }
        "dismiss_pending_password" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.pending_password = None;
            }
        }
        "save_password" => {
            if let (Some(origin), Some(pass)) = (msg.url, msg.pass) {
                let user = msg.title.unwrap_or_default();
                if !origin.is_empty() && !pass.is_empty() {
                    if let Ok(mut state) = state_rc.try_borrow_mut() {
                        state.passwords.add(&origin, &user, &pass);
                        state.pending_password = None;
                    }
                }
            }
        }
        "open_find" => {
            let wry_wv = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone()))
            });
            if let Some(wv) = wry_wv {
                let _ = wv.evaluate_script("if (window._ferroOpenFindBar) window._ferroOpenFindBar();");
            }
        }
        "find_text" => {
            let query = msg.url.unwrap_or_default();
            let inner = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone()))
            });
            if let Some(inner) = inner {
                if let Some(fc) = inner.find_controller() {
                    if query.is_empty() {
                        fc.search_finish();
                    } else {
                        let opts = (FindOptions::CASE_INSENSITIVE | FindOptions::WRAP_AROUND).bits();
                        fc.search(&query, opts, 1000);
                    }
                }
            }
        }
        "find_next" => {
            let inner = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone()))
            });
            if let Some(inner) = inner {
                if let Some(fc) = inner.find_controller() {
                    fc.search_next();
                }
            }
        }
        "find_prev" => {
            let inner = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone()))
            });
            if let Some(inner) = inner {
                if let Some(fc) = inner.find_controller() {
                    fc.search_previous();
                }
            }
        }
        "find_clear" => {
            let inner = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone()))
            });
            if let Some(inner) = inner {
                if let Some(fc) = inner.find_controller() {
                    fc.search_finish();
                }
            }
        }
        "print_page" => {
            let target = state_rc.try_borrow().ok().and_then(|s| {
                let win = s.window.clone();
                s.active_tab_id.and_then(|id| {
                    s.tabs.iter().find(|t| t.id == id).map(|t| (t.inner.clone(), win))
                })
            });
            if let Some((inner, win)) = target {
                glib::idle_add_local_once(move || {
                    let print_op = PrintOperation::new(&inner);
                    let _ = print_op.run_dialog(Some(&win));
                });
            }
        }
        "toggle_pin_tab" => {
            if let Some(target_id) = msg.id {
                {
                    if let Ok(mut state) = state_rc.try_borrow_mut() {
                        if let Some(idx) = state.tabs.iter().position(|t| t.id == target_id) {
                            let new_pinned = !state.tabs[idx].is_pinned;
                            state.tabs[idx].is_pinned = new_pinned;
                            let tab = state.tabs.remove(idx);
                            if new_pinned {
                                let insert_pos = state.tabs.iter().take_while(|t| t.is_pinned).count();
                                state.tabs.insert(insert_pos, tab);
                            } else {
                                let insert_pos = state.tabs.iter().take_while(|t| t.is_pinned).count();
                                state.tabs.insert(insert_pos, tab);
                            }
                        }
                    }
                }
                sync_tabs_to_ui(state_rc);
            }
        }
        "toggle_mute_tab" => {
            if let Some(target_id) = msg.id {
                {
                    if let Ok(mut state) = state_rc.try_borrow_mut() {
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == target_id) {
                            tab.is_muted = !tab.is_muted;
                            tab.inner.set_is_muted(tab.is_muted);
                        }
                    }
                }
                sync_tab_update_to_ui(state_rc, target_id);
            }
        }
        "open_path" => {
            if let Some(path) = msg.url {
                if !path.trim().is_empty() {
                    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
                }
            }
        }
        "open_downloads_folder" => {
            let dir = state_rc
                .try_borrow()
                .ok()
                .map(|s| s.settings.download_dir.clone())
                .unwrap_or_default();
            let folder = if !dir.trim().is_empty() {
                std::path::PathBuf::from(dir)
            } else {
                dirs::download_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            };
            let _ = std::fs::create_dir_all(&folder);
            let _ = std::process::Command::new("xdg-open").arg(folder).spawn();
        }
        "clear_downloads" => {
            if let Ok(state) = state_rc.try_borrow() {
                if let Ok(mut dl) = state.downloads.lock() {
                    dl.clear();
                }
            }
        }
        "open_lens_overlay" => {
            let wry_wv = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone()))
            });
            if let Some(wv) = wry_wv {
                let _ = wv.evaluate_script("if (window._ferroStartLensOverlay) window._ferroStartLensOverlay();");
            }
        }
        "lens_capture_rect" => {
            if let Some(json_str) = msg.url {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let rx = val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let ry = val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let rw = val.get("w").and_then(|v| v.as_f64()).unwrap_or(100.0);
                    let rh = val.get("h").and_then(|v| v.as_f64()).unwrap_or(100.0);
                    let vw = val.get("vw").and_then(|v| v.as_f64()).unwrap_or(1000.0).max(1.0);
                    let vh = val.get("vh").and_then(|v| v.as_f64()).unwrap_or(700.0).max(1.0);
                    let fallback = val
                        .get("fallback")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let active_inner = state_rc.try_borrow().ok().and_then(|st| {
                        st.active_tab_id
                            .and_then(|id| st.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone()))
                    });

                    if let Some(inner) = active_inner {
                        let alloc = inner.allocation();
                        let aw = alloc.width().max(1);
                        let ah = alloc.height().max(1);
                        let sx = (aw as f64) / vw;
                        let sy = (ah as f64) / vh;
                        let cx = ((rx * sx).round() as i32).clamp(0, (aw - 1).max(0));
                        let cy = ((ry * sy).round() as i32).clamp(0, (ah - 1).max(0));
                        let cw = ((rw * sx).round() as i32).max(12).min((aw - cx).max(1));
                        let ch = ((rh * sy).round() as i32).max(12).min((ah - cy).max(1));

                        let state_weak = Rc::downgrade(state_rc);
                        // Wayland-native WebKit snapshot API (works 100% on Wayland/Niri & X11!)
                        inner.snapshot(
                            SnapshotRegion::Visible,
                            SnapshotOptions::NONE,
                            gtk::gio::Cancellable::NONE,
                            move |res| {
                                let mut b64_png = String::new();
                                if let Ok(surface) = res {
                                    if let Some(pixbuf) = gdk::pixbuf_get_from_surface(&surface, cx, cy, cw, ch) {
                                        if let Ok(png_bytes) = pixbuf.save_to_bufferv("png", &[]) {
                                            b64_png = base64_encode(&png_bytes);
                                        }
                                    }
                                }
                                if let Some(s_rc) = state_weak.upgrade() {
                                    spawn_lens_background_search(&s_rc, b64_png, fallback);
                                }
                            },
                        );
                    }
                }
            }
        }
        "lens_image_data" => {
            let b64_data = msg.title.unwrap_or_default();
            let fallback = msg.url.unwrap_or_default();
            spawn_lens_background_search(state_rc, b64_data, fallback);
        }
        "lens_url" => {
            if let Some(img_url) = msg.url {
                spawn_lens_background_search(state_rc, String::new(), img_url);
            }
        }
        "open_side_panel" => {
            let mode = msg.url.as_deref().unwrap_or("bookmarks");
            match mode {
                "lens" => {
                    let prev_url = state_rc
                        .try_borrow()
                        .ok()
                        .and_then(|s| s.last_lens_result_url.clone());
                    if let Some(u) = prev_url {
                        ensure_and_open_side_panel(state_rc, "lens", &u);
                    } else {
                        let wry_wv = state_rc.try_borrow().ok().and_then(|s| {
                            s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone()))
                        });
                        if let Some(wv) = wry_wv {
                            let _ = wv.evaluate_script("if (window._ferroStartLensOverlay) window._ferroStartLensOverlay();");
                        }
                    }
                }
                "history" => ensure_and_open_side_panel(state_rc, "history", "ferro://history"),
                "downloads" => ensure_and_open_side_panel(state_rc, "downloads", "ferro://downloads"),
                _ => ensure_and_open_side_panel(state_rc, "bookmarks", "ferro://bookmarks-panel"),
            }
        }
        "toggle_side_panel" => {
            let currently_open = state_rc.try_borrow().map(|s| s.side_panel_open).unwrap_or(false);
            if currently_open {
                if let Ok(mut st) = state_rc.try_borrow_mut() {
                    st.side_box.hide();
                    st.side_panel_open = false;
                }
            } else {
                ensure_and_open_side_panel(state_rc, "bookmarks", "ferro://bookmarks-panel");
            }
        }
        "close_side_panel" => {
            if let Ok(mut st) = state_rc.try_borrow_mut() {
                st.side_box.hide();
                st.side_panel_open = false;
            }
        }
        "side_popout" => {
            let cur_uri = state_rc.try_borrow().ok().and_then(|s| {
                s.side_webview
                    .as_ref()
                    .and_then(|wv| wv.webview().uri().map(|u| u.to_string()))
            });
            if let Some(uri) = cur_uri {
                if !uri.is_empty() && uri != "ferro://lens-loading" {
                    let target = if uri == "ferro://bookmarks-panel" { "ferro://newtab" } else { &uri };
                    create_tab(state_rc, target);
                    if let Ok(mut st) = state_rc.try_borrow_mut() {
                        st.side_box.hide();
                        st.side_panel_open = false;
                    }
                }
            }
        }
        "import_detect" => {
            let state_weak = Rc::downgrade(state_rc);
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            std::thread::spawn(move || {
                let list = BrowserImporter::detect_all();
                let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string());
                let _ = tx.send(json);
            });
            glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
                match rx.try_recv() {
                    Ok(json) => {
                        if let Some(s_rc) = state_weak.upgrade() {
                            let wvs: Vec<Rc<WebView>> = s_rc
                                .try_borrow()
                                .map(|st| st.tabs.iter().map(|t| t.webview.clone()).collect())
                                .unwrap_or_default();
                            let js = format!(
                                "window._ferro_detected_browsers = {0}; if (window.onImportDetected) window.onImportDetected({0});",
                                json
                            );
                            for wv in wvs {
                                let _ = wv.evaluate_script(&js);
                            }
                        }
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                }
            });
        }
        "import_run" => {
            let browser_id = msg.url.unwrap_or_else(|| "chrome:Default".to_string());
            let state_weak = Rc::downgrade(state_rc);
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            std::thread::spawn(move || {
                let res = BrowserImporter::execute_import(&browser_id);
                let json = serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string());
                let _ = tx.send(json);
            });
            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                match rx.try_recv() {
                    Ok(res_json) => {
                        if let Some(s_rc) = state_weak.upgrade() {
                            let wvs: Vec<Rc<WebView>> = {
                                if let Ok(mut st) = s_rc.try_borrow_mut() {
                                    st.bookmarks = BookmarksData::load_or_default();
                                    st.history = HistoryData::load_or_default();
                                    st.settings = BrowserSettings::load_or_default();
                                    st.tabs.iter().map(|t| t.webview.clone()).collect()
                                } else {
                                    Vec::new()
                                }
                            };
                            sync_bookmarks_to_ui(&s_rc);
                            let js = format!("if (window.onImportCompleted) window.onImportCompleted({});", res_json);
                            for wv in wvs {
                                let _ = wv.evaluate_script(&js);
                            }
                        }
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                }
            });
        }
        "import_dismiss" => {
            if let Ok(mut st) = state_rc.try_borrow_mut() {
                st.settings.has_prompted_import = true;
                let _ = st.settings.save();
            }
        }
        "toggle_reader" => {
            let wry_wv = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone()))
            });
            if let Some(wv) = wry_wv {
                let _ = wv.evaluate_script("if (window._ferroToggleReaderMode) window._ferroToggleReaderMode();");
            }
        }
        "show_qr" => {
            let wry_wv = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone()))
            });
            if let Some(wv) = wry_wv {
                let _ = wv.evaluate_script("if (window._ferroShowQrModal) window._ferroShowQrModal();");
            }
        }
        "toggle_pip" => {
            let wry_wv = state_rc.try_borrow().ok().and_then(|s| {
                s.active_tab_id.and_then(|id| s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone()))
            });
            if let Some(wv) = wry_wv {
                let _ = wv.evaluate_script("if (window._ferroToggleVideoPiP) window._ferroToggleVideoPiP();");
            }
        }
        "open_pip_window" => {
            if let Some(pip_url) = msg.url {
                let sec = msg.id.unwrap_or(0);
                let state_weak = Rc::downgrade(state_rc);
                glib::idle_add_local_once(move || {
                    if let Some(s_rc) = state_weak.upgrade() {
                        open_floating_pip_window(&s_rc, &pip_url, sec);
                    }
                });
            }
        }
        "pip_drag" => {
            if let Ok(state) = state_rc.try_borrow() {
                if let Some((pip_win, _, _)) = &state.pip_window {
                    if let Some(display) = gdk::Display::default() {
                        if let Some(seat) = display.default_seat() {
                            if let Some(pointer) = seat.pointer() {
                                let (_, x, y) = pointer.position();
                                pip_win.begin_move_drag(1, x, y, gtk::current_event_time());
                            }
                        }
                    }
                }
            }
        }
        "pip_close" => {
            let old = state_rc.try_borrow_mut().ok().and_then(|mut st| st.pip_window.take());
            if let Some((pip_win, _, _)) = old {
                pip_win.close();
            }
        }
        "pip_return_to_tab" => {
            let resume_sec = msg.id.unwrap_or(0);
            let old = state_rc.try_borrow_mut().ok().and_then(|mut st| st.pip_window.take());
            if let Some((pip_win, _, origin_tid)) = old {
                pip_win.close();
                if let Some(tid) = origin_tid {
                    switch_tab(state_rc, tid);
                    let target_wv = state_rc.try_borrow().ok().and_then(|st| {
                        st.window.present();
                        st.tabs.iter().find(|t| t.id == tid).map(|t| t.webview.clone())
                    });
                    if let Some(wv) = target_wv {
                        let _ = wv.evaluate_script(&format!(
                            "if (window._ferroResumeFromPip) window._ferroResumeFromPip({});",
                            resume_sec
                        ));
                    }
                }
            }
        }
        "fetch_sponsorblock" => {
            if let Some(vid_id) = msg.url {
                spawn_sponsorblock_fetch(state_rc, vid_id);
            }
        }
        "toggle_sponsorblock" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.settings.sponsorblock_enabled = !state.settings.sponsorblock_enabled;
                let _ = state.settings.save();
            }
            broadcast_settings_from_disk();
        }
        "reorder_tabs" => {
            if let Some(order_str) = msg.url {
                let ids: Vec<u32> = order_str
                    .split(',')
                    .filter_map(|s| s.trim().parse::<u32>().ok())
                    .collect();
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    if ids.len() == state.tabs.len() {
                        let mut reordered = Vec::with_capacity(state.tabs.len());
                        for id in ids {
                            if let Some(pos) = state.tabs.iter().position(|t| t.id == id) {
                                reordered.push(state.tabs.remove(pos));
                            }
                        }
                        reordered.append(&mut state.tabs);
                        state.tabs = reordered;
                    }
                }
            }
        }
        "sleep_tab" => {
            if let Some(target_id) = msg.id {
                let sleep_nav = {
                    if let Ok(mut state) = state_rc.try_borrow_mut() {
                        if state.active_tab_id != Some(target_id) {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == target_id) {
                                if tab.sleeping_url.is_none() && !tab.url.starts_with("ferro://") && !tab.url.is_empty() {
                                    let real_url = tab.url.clone();
                                    let enc_u = percent_encoding::utf8_percent_encode(&real_url, percent_encoding::NON_ALPHANUMERIC).to_string();
                                    let enc_t = percent_encoding::utf8_percent_encode(&tab.title, percent_encoding::NON_ALPHANUMERIC).to_string();
                                    tab.sleeping_url = Some(real_url);
                                    Some((tab.webview.clone(), format!("ferro://sleep?url={}&title={}", enc_u, enc_t)))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some((wv, sleep_uri)) = sleep_nav {
                    let _ = wv.load_url(&sleep_uri);
                    sync_tab_update_to_ui(state_rc, target_id);
                }
            }
        }
        "close_tabs_right" => {
            if let Some(target_id) = msg.id {
                let right_ids: Vec<u32> = state_rc
                    .try_borrow()
                    .ok()
                    .and_then(|st| {
                        st.tabs.iter().position(|t| t.id == target_id).map(|pos| {
                            st.tabs
                                .iter()
                                .skip(pos + 1)
                                .filter(|t| !t.is_pinned)
                                .map(|t| t.id)
                                .collect()
                        })
                    })
                    .unwrap_or_default();
                for rid in right_ids {
                    close_tab(state_rc, rid);
                }
            }
        }
        "clear_history" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.history.clear();
            }
        }
        "clear_passwords" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.passwords.clear();
            }
        }
        "clear_cache" => {
            let wk_ctx = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.tabs.first().and_then(|t| t.inner.context())
                } else {
                    None
                }
            };
            if let Some(ctx) = wk_ctx {
                ctx.clear_cache();
                if let Some(mgr) = ctx.website_data_manager() {
                    mgr.clear(
                        WebsiteDataTypes::DISK_CACHE | WebsiteDataTypes::MEMORY_CACHE,
                        glib::TimeSpan::from_seconds(0),
                        gtk::gio::Cancellable::NONE,
                        |_| {},
                    );
                }
            }
        }
        "clear_site_data" => {
            let wk_ctx = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.tabs.first().and_then(|t| t.inner.context())
                } else {
                    None
                }
            };
            if let Some(ctx) = wk_ctx {
                ctx.clear_cache();
                if let Some(mgr) = ctx.website_data_manager() {
                    mgr.clear(
                        WebsiteDataTypes::ALL,
                        glib::TimeSpan::from_seconds(0),
                        gtk::gio::Cancellable::NONE,
                        |_| {},
                    );
                }
            }
        }
        "pick_download_dir" => {
            let win_opt = state_rc.try_borrow().ok().map(|s| s.window.clone());
            let state_weak = Rc::downgrade(state_rc);
            glib::idle_add_local_once(move || {
                let dialog = gtk::FileChooserNative::new(
                    Some("Выбрать папку для загрузок"),
                    win_opt.as_ref(),
                    gtk::FileChooserAction::SelectFolder,
                    Some("Выбрать"),
                    Some("Отмена"),
                );
                if dialog.run() == gtk::ResponseType::Accept {
                    if let Some(folder) = dialog.filename() {
                        if let Some(state_rc) = state_weak.upgrade() {
                            if let Ok(mut st) = state_rc.try_borrow_mut() {
                                st.settings.download_dir = folder.to_string_lossy().to_string();
                                let _ = st.settings.save();
                            }
                            sync_settings_to_ui(&state_rc);
                        }
                    }
                }
            });
        }
        "toggle_theme" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                let next_mode = if state.settings.theme_mode == "light" {
                    "dark".to_string()
                } else {
                    "light".to_string()
                };
                state.theme = next_mode.clone();
                state.settings.theme_mode = next_mode;
                let _ = state.settings.save();
            }
            broadcast_settings_from_disk();
        }
        "edit_cmd" => {
            let inner = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                    })
                } else {
                    None
                }
            };
            if let (Some(cmd), Some(inner)) = (msg.url.as_deref(), inner) {
                match cmd {
                    "cut" => inner.execute_editing_command("Cut"),
                    "copy" => inner.execute_editing_command("Copy"),
                    "paste" => inner.execute_editing_command("Paste"),
                    "select_all" => inner.execute_editing_command("SelectAll"),
                    _ => {}
                }
            }
        }
        "inspect_element" => {
            let inner = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                    })
                } else {
                    None
                }
            };
            if let Some(inner) = inner {
                if let Some(inspector) = inner.inspector() {
                    inspector.show();
                }
            }
        }
        "open_source_html" => {
            let cache = state_rc.try_borrow().ok().map(|s| s.page_source_cache.clone());
            if let Some(cache) = cache {
                if let Ok(mut guard) = cache.lock() {
                    *guard = (
                        msg.url.unwrap_or_else(|| "about:blank".to_string()),
                        msg.title.unwrap_or_default(),
                    );
                }
            }
            create_tab(state_rc, "ferro://view-source");
        }
        "save_page_html" => {
            let html_content = msg.title.unwrap_or_default();
            let win_opt = state_rc.try_borrow().ok().map(|s| s.window.clone());
            glib::idle_add_local_once(move || {
                let dialog = gtk::FileChooserNative::new(
                    Some("Сохранить страницу как..."),
                    win_opt.as_ref(),
                    gtk::FileChooserAction::Save,
                    Some("Сохранить"),
                    Some("Отмена"),
                );
                if let Some(dl_dir) = dirs::download_dir().or_else(dirs::home_dir) {
                    let _ = dialog.set_current_folder(&dl_dir);
                }
                dialog.set_current_name("page.html");
                if dialog.run() == gtk::ResponseType::Accept {
                    if let Some(path) = dialog.filename() {
                        let _ = std::fs::write(path, html_content.as_bytes());
                    }
                }
            });
        }
        "download_url" => {
            if let Some(url) = msg.url {
                let inner = state_rc.try_borrow().ok().and_then(|s| {
                    s.active_tab_id.and_then(|id| {
                        s.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                    })
                });
                if let Some(inner) = inner {
                    let _ = inner.download_uri(&url);
                }
            }
        }
        "take_screenshot" => {
            let info = state_rc.try_borrow().ok().and_then(|state| {
                let dl_dir = state.settings.download_dir.clone();
                state.active_tab_id.and_then(|id| {
                    state
                        .tabs
                        .iter()
                        .find(|t| t.id == id)
                        .map(|t| (t.inner.clone(), t.webview.clone(), dl_dir))
                })
            });
            if let Some((inner, active_wv, dl_dir)) = info {
                let alloc = inner.allocation();
                let aw = alloc.width().max(1);
                let ah = alloc.height().max(1);
                inner.snapshot(
                    SnapshotRegion::Visible,
                    SnapshotOptions::NONE,
                    gtk::gio::Cancellable::NONE,
                    move |res| {
                        if let Ok(surface) = res {
                            if let Some(pixbuf) = gdk::pixbuf_get_from_surface(&surface, 0, 0, aw, ah) {
                                let dir = if !dl_dir.trim().is_empty() {
                                    std::path::PathBuf::from(&dl_dir)
                                } else {
                                    dirs::picture_dir()
                                        .or_else(dirs::download_dir)
                                        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                                };
                                let _ = std::fs::create_dir_all(&dir);
                                let ts = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs();
                                let path = dir.join(format!("ferro-screenshot-{}.png", ts));
                                if pixbuf.savev(&path, "png", &[]).is_ok() {
                                    let path_str = path.to_string_lossy().replace('\\', "/").replace('\'', "\\'");
                                    let toast_js = format!(
                                        r#"(function(){{
                                            const t = document.createElement('div');
                                            t.textContent = 'Снимок экрана сохранён: {0}';
                                            t.style.cssText = 'position:fixed;bottom:18px;right:18px;z-index:2147483647;background:#1c1c20;color:#f2f2f5;border:1px solid #3c3c46;border-radius:6px;padding:9px 14px;font-family:Inter,sans-serif;font-size:12px;box-shadow:0 8px 24px rgba(0,0,0,0.6);';
                                            (document.body||document.documentElement).appendChild(t);
                                            setTimeout(() => t.remove(), 3200);
                                        }})();"#,
                                        path_str
                                    );
                                    let _ = active_wv.evaluate_script(&toast_js);
                                }
                            }
                        }
                    },
                );
            }
        }
        "tab_context_menu" => {
            if let Some(tab_id) = msg.id {
                let tab_url = msg.url.unwrap_or_else(|| "ferro://newtab".to_string());
                let (mx, my) = parse_menu_coords(msg.pass.as_deref(), 120, 32);
                let state_weak = Rc::downgrade(state_rc);
                glib::idle_add_local_once(move || {
                    if let Some(state_rc) = state_weak.upgrade() {
                        let (is_pinned, is_muted, is_active, tabs_len) = state_rc
                            .try_borrow()
                            .ok()
                            .and_then(|st| {
                                st.tabs
                                    .iter()
                                    .find(|t| t.id == tab_id)
                                    .map(|t| (t.is_pinned, t.is_muted, st.active_tab_id == Some(tab_id), st.tabs.len()))
                            })
                            .unwrap_or((false, false, false, 1));

                        let mut entries: Vec<PopoverEntry> = Vec::new();

                        {
                            let s_rc = state_rc.clone();
                            entries.push(PopoverEntry::Item(
                                "Новая вкладка                   Ctrl+T".to_string(),
                                Box::new(move || { create_tab(&s_rc, "ferro://newtab"); }),
                            ));
                        }
                        {
                            let s_rc = state_rc.clone();
                            entries.push(PopoverEntry::Item(
                                "Обновить вкладку            Ctrl+R".to_string(),
                                Box::new(move || {
                                    if let Ok(state) = s_rc.try_borrow() {
                                        if let Some(t) = state.tabs.iter().find(|t| t.id == tab_id) {
                                            t.inner.reload();
                                        }
                                    }
                                }),
                            ));
                        }
                        {
                            let s_rc = state_rc.clone();
                            let dup_url = tab_url.clone();
                            entries.push(PopoverEntry::Item(
                                "Дублировать вкладку".to_string(),
                                Box::new(move || { create_tab(&s_rc, &dup_url); }),
                            ));
                        }
                        if tabs_len > 1 {
                            let s_rc = state_rc.clone();
                            entries.push(PopoverEntry::Item(
                                "Переместить вкладку в новое окно".to_string(),
                                Box::new(move || {
                                    handle_ipc(&s_rc, IpcMessage {
                                        action: "detach_tab_to_window".to_string(),
                                        url: None,
                                        id: Some(tab_id),
                                        title: None,
                                        pass: None,
                                        settings: None,
                                    });
                                }),
                            ));
                        }
                        if open_windows_count() > 1 {
                            let s_rc = state_rc.clone();
                            entries.push(PopoverEntry::Item(
                                "Переместить вкладку в другое окно".to_string(),
                                Box::new(move || {
                                    handle_ipc(&s_rc, IpcMessage {
                                        action: "move_tab_to_other_window".to_string(),
                                        url: None,
                                        id: Some(tab_id),
                                        title: None,
                                        pass: None,
                                        settings: None,
                                    });
                                }),
                            ));
                        }
                        {
                            let s_rc = state_rc.clone();
                            let pin_lbl = if is_pinned { "Открепить вкладку" } else { "Закрепить вкладку" };
                            entries.push(PopoverEntry::Item(
                                pin_lbl.to_string(),
                                Box::new(move || {
                                    handle_ipc(&s_rc, IpcMessage {
                                        action: "toggle_pin_tab".to_string(),
                                        url: None,
                                        id: Some(tab_id),
                                        title: None,
                                        pass: None,
                                        settings: None,
                                    });
                                }),
                            ));
                        }
                        {
                            let s_rc = state_rc.clone();
                            let mute_lbl = if is_muted { "Включить звук вкладки" } else { "Выключить звук вкладки" };
                            entries.push(PopoverEntry::Item(
                                mute_lbl.to_string(),
                                Box::new(move || {
                                    handle_ipc(&s_rc, IpcMessage {
                                        action: "toggle_mute_tab".to_string(),
                                        url: None,
                                        id: Some(tab_id),
                                        title: None,
                                        pass: None,
                                        settings: None,
                                    });
                                }),
                            ));
                        }
                        if !is_active {
                            let s_rc = state_rc.clone();
                            entries.push(PopoverEntry::Item(
                                "Усыпить вкладку (освободить ОЗУ)".to_string(),
                                Box::new(move || {
                                    handle_ipc(&s_rc, IpcMessage {
                                        action: "sleep_tab".to_string(),
                                        url: None,
                                        id: Some(tab_id),
                                        title: None,
                                        pass: None,
                                        settings: None,
                                    });
                                }),
                            ));
                        }

                        entries.push(PopoverEntry::Separator);

                        {
                            let s_rc = state_rc.clone();
                            entries.push(PopoverEntry::Item(
                                "Закрыть вкладки справа".to_string(),
                                Box::new(move || {
                                    handle_ipc(&s_rc, IpcMessage {
                                        action: "close_tabs_right".to_string(),
                                        url: None,
                                        id: Some(tab_id),
                                        title: None,
                                        pass: None,
                                        settings: None,
                                    });
                                }),
                            ));
                        }
                        {
                            let s_rc = state_rc.clone();
                            entries.push(PopoverEntry::Item(
                                "Закрыть другие вкладки".to_string(),
                                Box::new(move || {
                                    let other_ids: Vec<u32> = s_rc
                                        .try_borrow()
                                        .map(|st| {
                                            st.tabs
                                                .iter()
                                                .filter(|t| t.id != tab_id && !t.is_pinned)
                                                .map(|t| t.id)
                                                .collect()
                                        })
                                        .unwrap_or_default();
                                    for oid in other_ids {
                                        close_tab(&s_rc, oid);
                                    }
                                }),
                            ));
                        }
                        {
                            let s_rc = state_rc.clone();
                            entries.push(PopoverEntry::Item(
                                "Закрыть вкладку              Ctrl+W".to_string(),
                                Box::new(move || { close_tab(&s_rc, tab_id); }),
                            ));
                        }

                        show_top_popover(&state_rc, mx, my, entries);
                    }
                });
            }
        }
        "bookmark_context_menu" => {
            if let Some(bm_url) = msg.url {
                let bm_title = msg.title.unwrap_or_else(|| bm_url.clone());
                let (mx, my) = parse_menu_coords(msg.pass.as_deref(), 80, 96);
                let state_weak = Rc::downgrade(state_rc);
                glib::idle_add_local_once(move || {
                    if let Some(state_rc) = state_weak.upgrade() {
                        let mut entries: Vec<PopoverEntry> = Vec::new();
                        {
                            let s_rc = state_rc.clone();
                            let u1 = bm_url.clone();
                            entries.push(PopoverEntry::Item(
                                "Открыть".to_string(),
                                Box::new(move || {
                                    handle_ipc(&s_rc, IpcMessage {
                                        action: "navigate".to_string(),
                                        url: Some(u1.clone()),
                                        id: None,
                                        title: None,
                                        pass: None,
                                        settings: None,
                                    });
                                }),
                            ));
                        }
                        {
                            let s_rc = state_rc.clone();
                            let u2 = bm_url.clone();
                            entries.push(PopoverEntry::Item(
                                "Открыть в новой вкладке".to_string(),
                                Box::new(move || { create_tab(&s_rc, &u2); }),
                            ));
                        }
                        {
                            let u_copy = bm_url.clone();
                            entries.push(PopoverEntry::Item(
                                "Копировать адрес закладки".to_string(),
                                Box::new(move || {
                                    let cb = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);
                                    cb.set_text(&u_copy);
                                }),
                            ));
                        }
                        entries.push(PopoverEntry::Separator);
                        {
                            let s_rc = state_rc.clone();
                            let u3 = bm_url.clone();
                            let t3 = bm_title.clone();
                            entries.push(PopoverEntry::Item(
                                "Удалить из закладок".to_string(),
                                Box::new(move || {
                                    if let Ok(mut state) = s_rc.try_borrow_mut() {
                                        state.bookmarks.toggle(&u3, &t3);
                                    }
                                    broadcast_bookmarks_from_disk();
                                }),
                            ));
                        }

                        show_top_popover(&state_rc, mx, my, entries);
                    }
                });
            }
        }
        "bookmarks_overflow_menu" => {
            let start_idx = msg.id.unwrap_or(0) as usize;
            let win_w = state_rc.try_borrow().map(|st| st.window.size().0).unwrap_or(1180);
            let (mx, my) = parse_menu_coords(msg.pass.as_deref(), (win_w - 20).max(20), 98);
            let state_weak = Rc::downgrade(state_rc);
            glib::idle_add_local_once(move || {
                if let Some(state_rc) = state_weak.upgrade() {
                    let overflow_items: Vec<(String, String)> = state_rc
                        .try_borrow()
                        .map(|st| {
                            st.bookmarks
                                .items
                                .iter()
                                .skip(start_idx)
                                .take(32)
                                .map(|b| (b.title.clone(), b.url.clone()))
                                .collect()
                        })
                        .unwrap_or_default();

                    if overflow_items.is_empty() {
                        return;
                    }

                    let mut entries: Vec<PopoverEntry> = Vec::new();
                    for (title, url) in overflow_items {
                        let raw_label = if title.trim().is_empty() { &url } else { &title };
                        let truncated: String = if raw_label.chars().count() > 42 {
                            let s: String = raw_label.chars().take(41).collect();
                            format!("{}…", s)
                        } else {
                            raw_label.to_string()
                        };
                        let s_rc = state_rc.clone();
                        entries.push(PopoverEntry::Item(
                            truncated,
                            Box::new(move || {
                                handle_ipc(&s_rc, IpcMessage {
                                    action: "navigate".to_string(),
                                    url: Some(url.clone()),
                                    id: None,
                                    title: None,
                                    pass: None,
                                    settings: None,
                                });
                            }),
                        ));
                    }

                    show_top_popover(&state_rc, mx, my, entries);
                }
            });
        }
        "window_drag" => {
            if let Ok(state) = state_rc.try_borrow() {
                if let Some(display) = gdk::Display::default() {
                    if let Some(seat) = display.default_seat() {
                        if let Some(pointer) = seat.pointer() {
                            let (_, x, y) = pointer.position();
                            state.window.begin_move_drag(1, x, y, gtk::current_event_time());
                        }
                    }
                }
            }
        }
        "open_menu" => {
            let win_w = state_rc.try_borrow().map(|st| st.window.size().0).unwrap_or(1180);
            let (mx, my) = parse_menu_coords(msg.pass.as_deref(), (win_w - 22).max(20), 68);
            let state_weak = Rc::downgrade(state_rc);
            glib::idle_add_local_once(move || {
                if let Some(state_rc) = state_weak.upgrade() {
                    let (ab_label, sb_label, th_label) = {
                        if let Ok(state) = state_rc.try_borrow() {
                            let ab = if state.adblocker.is_enabled() {
                                format!("Блокировщик рекламы: Вкл ({})", state.adblocker.blocked_count())
                            } else {
                                "Блокировщик рекламы: Выкл".to_string()
                            };
                            let sb = if state.settings.sponsorblock_enabled {
                                "SponsorBlock (YouTube): Вкл".to_string()
                            } else {
                                "SponsorBlock (YouTube): Выкл".to_string()
                            };
                            let th = if state.theme == "light" {
                                "Тема: Светлая (переключить)".to_string()
                            } else {
                                "Тема: Тёмная (переключить)".to_string()
                            };
                            (ab, sb, th)
                        } else {
                            (
                                "Блокировщик рекламы".to_string(),
                                "SponsorBlock (YouTube)".to_string(),
                                "Тема оформления".to_string(),
                            )
                        }
                    };

                    let ipc_item = |label: &str, act: &'static str| -> PopoverEntry {
                        let s_rc = state_rc.clone();
                        PopoverEntry::Item(
                            label.to_string(),
                            Box::new(move || {
                                handle_ipc(&s_rc, IpcMessage {
                                    action: act.to_string(),
                                    url: None,
                                    id: None,
                                    title: None,
                                    pass: None,
                                    settings: None,
                                });
                            }),
                        )
                    };
                    let tab_item = |label: &str, url: &'static str| -> PopoverEntry {
                        let s_rc = state_rc.clone();
                        PopoverEntry::Item(
                            label.to_string(),
                            Box::new(move || { create_tab(&s_rc, url); }),
                        )
                    };

                    let entries = vec![
                        tab_item("Новая вкладка                   Ctrl+T", "ferro://newtab"),
                        ipc_item("Новое окно                         Ctrl+N", "new_window"),
                        ipc_item("Восстановить вкладку   Ctrl+Shift+T", "reopen_closed_tab"),
                        PopoverEntry::Separator,
                        tab_item("История просмотров         Ctrl+H", "ferro://history"),
                        tab_item("Загрузки                             Ctrl+J", "ferro://downloads"),
                        tab_item("Менеджер паролей", "ferro://passwords"),
                        ipc_item("Боковая панель закладок", "toggle_side_panel"),
                        PopoverEntry::Separator,
                        ipc_item("Поиск с Google Объективом", "open_lens_overlay"),
                        ipc_item("Режим чтения                     Alt+R", "toggle_reader"),
                        ipc_item("Картинка в картинке (PiP)", "toggle_pip"),
                        ipc_item("QR-код страницы", "show_qr"),
                        ipc_item("Найти на странице            Ctrl+F", "open_find"),
                        ipc_item("Печать / Сохранить PDF   Ctrl+P", "print_page"),
                        ipc_item("Снимок экрана", "take_screenshot"),
                        PopoverEntry::Separator,
                        ipc_item(&ab_label, "toggle_adblock"),
                        ipc_item(&sb_label, "toggle_sponsorblock"),
                        ipc_item(&th_label, "toggle_theme"),
                    ];

                    show_top_popover(&state_rc, mx, my, entries);
                }
            });
        }
        "set_top_height" => {
            // No-op: toolbar remains fixed 102px without shifting content
        }
        "window_min" => {
            if let Ok(state) = state_rc.try_borrow() {
                state.window.iconify();
            }
        }
        "window_max" => {
            let toggle_info = {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    if state.win_fullscreen {
                        state.win_fullscreen = false;
                        state.window.unfullscreen();
                        state.window.unmaximize();
                        None
                    } else if state.window.is_maximized() {
                        state.window.unmaximize();
                        None
                    } else {
                        let old_size = state.window.size();
                        state.window.maximize();
                        Some((state.window.clone(), old_size))
                    }
                } else {
                    None
                }
            };
            // On scrollable/tiling Wayland compositors (such as Niri) where `maximize()` may leave
            // column gaps or be ignored depending on WM rules, check after 75ms and promote to
            // edge-to-edge fullscreen if size didn't expand!
            if let Some((win, old_size)) = toggle_info {
                let state_weak = Rc::downgrade(state_rc);
                glib::timeout_add_local_once(std::time::Duration::from_millis(75), move || {
                    if !win.is_maximized() && win.size() == old_size {
                        win.fullscreen();
                        if let Some(s_rc) = state_weak.upgrade() {
                            if let Ok(mut st) = s_rc.try_borrow_mut() {
                                st.win_fullscreen = true;
                            }
                        }
                    }
                });
            }
        }
        "window_close" => {
            if open_windows_count() <= 1 {
                save_current_session(state_rc);
            }
            if let Ok(state) = state_rc.try_borrow() {
                state.window.close();
            }
        }
        _ => {}
    }
}

fn clean_stale_profile_locks(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir.join("hsts-storage.sqlite-shm"));
    let _ = std::fs::remove_file(dir.join("hsts-storage.sqlite-wal"));
}

fn current_binary_stamp() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| format!("{}:{}", d.as_secs(), d.subsec_nanos()))
        .unwrap_or_default()
}

fn ensure_single_instance() -> Option<std::os::unix::net::UnixListener> {
    use std::io::Write;
    use std::os::unix::net::{UnixListener, UnixStream};

    let ferro_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("ferro");
    let _ = std::fs::create_dir_all(&ferro_dir);

    let sock_path = ferro_dir.join("instance.sock");
    let meta_path = ferro_dir.join("instance.meta");
    let my_stamp = current_binary_stamp();
    let force_replace = std::env::args().any(|a| a == "--replace" || a == "-r");

    let write_meta = || {
        let _ = std::fs::write(&meta_path, format!("{}\n{}", std::process::id(), my_stamp));
    };

    match UnixListener::bind(&sock_path) {
        Ok(listener) => {
            let _ = listener.set_nonblocking(true);
            write_meta();
            Some(listener)
        }
        Err(_) => {
            // Check if running instance is an older compiled binary
            let mut should_replace = force_replace;
            let mut old_pid: Option<u32> = None;
            if let Ok(raw) = std::fs::read_to_string(&meta_path) {
                let mut lines = raw.lines();
                old_pid = lines.next().and_then(|s| s.trim().parse::<u32>().ok());
                let old_stamp = lines.next().unwrap_or("").trim();
                if !my_stamp.is_empty() && old_stamp != my_stamp {
                    should_replace = true;
                }
            } else {
                // Pre-upgrade instance without instance.meta -> replace it with the new build!
                should_replace = true;
            }

            if should_replace {
                println!("[Ferro] Detected updated build — restarting older Ferro instance...");
                if let Ok(mut stream) = UnixStream::connect(&sock_path) {
                    let _ = stream.write_all(b"QUIT\n");
                }
                if let Some(pid) = old_pid {
                    let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
                }
                std::thread::sleep(std::time::Duration::from_millis(220));
                let _ = std::fs::remove_file(&sock_path);
                if let Ok(listener) = UnixListener::bind(&sock_path) {
                    let _ = listener.set_nonblocking(true);
                    write_meta();
                    return Some(listener);
                }
            }

            if let Ok(mut stream) = UnixStream::connect(&sock_path) {
                println!("[Ferro] Already running. Focusing existing window.");
                let _ = stream.write_all(b"FOCUS\n");
                None
            } else {
                let _ = std::fs::remove_file(&sock_path);
                if let Ok(listener) = UnixListener::bind(&sock_path) {
                    let _ = listener.set_nonblocking(true);
                    write_meta();
                    Some(listener)
                } else {
                    None
                }
            }
        }
    }
}

fn spawn_browser_window(
    core: &SharedBrowserCore,
    initial: InitialWindowMode,
) -> Rc<RefCell<BrowserState>> {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Ferro");
    let (def_w, def_h, detach_drag_info) = match &initial {
        InitialWindowMode::DetachedTab(spec) => (
            spec.size.0.max(640),
            spec.size.1.max(480),
            Some((spec.start_move_drag, spec.press_time)),
        ),
        _ => (1180, 780, None),
    };
    window.set_default_size(def_w, def_h);
    window.set_decorated(false);
    if detach_drag_info.is_some() {
        if let Some(display) = gdk::Display::default() {
            if let Some(seat) = display.default_seat() {
                if let Some(pointer) = seat.pointer() {
                    let (_, px, py) = pointer.position();
                    window.move_((px - 120).max(0), (py - 18).max(0));
                }
            }
        }
    } else {
        window.set_position(gtk::WindowPosition::Center);
    }

    if let Some(screen) = gdk::Screen::default() {
        if let Some(visual) = screen.rgba_visual() {
            window.set_visual(Some(&visual));
        }
    }
    window.set_app_paintable(true);

    const LOGO_PNG: &[u8] = include_bytes!("ui/logo.png");
    if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(LOGO_PNG)) {
        window.set_icon(Some(&pixbuf));
    }

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.add(&vbox);

    let top_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let bookmarks = BookmarksData::load_or_default();
    let history = HistoryData::load_or_default();
    let passwords = PasswordsData::load_or_default();
    let settings = BrowserSettings::load_or_default();
    core.adblocker.set_enabled(settings.adblock_enabled);
    let initial_top_h = if settings.show_bookmarks_bar { 102 } else { 74 };
    top_box.set_size_request(-1, initial_top_h);
    vbox.pack_start(&top_box, false, false, 0);

    let hpaned = gtk::Paned::new(gtk::Orientation::Horizontal);
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::None);
    stack.set_transition_duration(0);
    let side_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    hpaned.pack1(&stack, true, false);
    hpaned.pack2(&side_box, false, false);
    vbox.pack_start(&hpaned, true, true, 0);

    let theme_mode = settings.theme_mode.clone();
    let initial_settings_json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".to_string());
    let initial_surface_rgba = compute_surface_rgba(&settings);
    let last_btn1_press_time = Rc::new(Cell::new(0u32));

    let state = Rc::new(RefCell::new(BrowserState {
        core: core.clone(),
        window: window.clone(),
        top_box: top_box.clone(),
        hpaned,
        stack: stack.clone(),
        side_box: side_box.clone(),
        side_header_webview: None,
        side_webview: None,
        side_panel_open: false,
        lens_preview: core.lens_preview.clone(),
        last_lens_result_url: None,
        spare_tab: None,
        active_popover: None,
        pip_window: None,
        win_fullscreen: false,
        last_btn1_press_time: last_btn1_press_time.clone(),
        css_provider: core.css_provider.clone(),
        top_webview: None,
        tabs: Vec::new(),
        closed_tabs: Vec::new(),
        active_tab_id: None,
        bookmarks,
        history,
        passwords,
        downloads: core.downloads.clone(),
        pending_password: None,
        settings,
        web_context: core.web_context.clone(),
        adblocker: core.adblocker.clone(),
        native_filter: core.native_filter.clone(),
        dark_mode: false,
        theme: theme_mode,
        download_handler_attached: core.download_handler_attached.clone(),
        settings_dirty: core.settings_dirty.clone(),
        bookmarks_dirty: core.bookmarks_dirty.clone(),
        history_dirty: core.history_dirty.clone(),
        passwords_dirty: core.passwords_dirty.clone(),
        page_source_cache: core.page_source_cache.clone(),
    }));

    ALL_WINDOWS.with(|wins| wins.borrow_mut().push(state.clone()));

    // Explicitly clear top_box RGBA window backing strip only when top region is in clip_extents,
    // avoiding full-window Cairo CPU fills during 60fps video playback in the tab stack below!
    let first_frame_drawn = Rc::new(Cell::new(false));
    let drawn_flag_cb = first_frame_drawn.clone();
    let state_for_draw = state.clone();
    window.connect_draw(move |win, cr| {
        drawn_flag_cb.set(true);
        let top_only = cr.clip_extents().map(|(_, y1, _, _)| y1 < 108.0).unwrap_or(true);
        if top_only {
            let rgba = state_for_draw
                .try_borrow()
                .map(|st| compute_surface_rgba(&st.settings))
                .unwrap_or_else(|_| gdk::RGBA::new(0.08, 0.08, 0.08, 1.0));
            let w = win.allocated_width().max(1) as f64;
            let _ = cr.save();
            cr.rectangle(0.0, 0.0, w, 108.0);
            cr.set_operator(gtk::cairo::Operator::Source);
            cr.set_source_rgba(rgba.red(), rgba.green(), rgba.blue(), rgba.alpha());
            let _ = cr.fill();
            let _ = cr.restore();
        }
        glib::Propagation::Proceed
    });

    // Setup Top UI WebView using the shared WebContext so WebKitGTK uses a single WebKitNetworkProcess!
    let state_for_ipc = state.clone();
    let top_init_js = format!(
        "window.addEventListener('DOMContentLoaded', function() {{ if (window.ferro && window.ferro.applySettings) {{ window.ferro.applySettings({}); }} }});",
        initial_settings_json
    );
    let top_webview = {
        let mut web_ctx = core.web_context.borrow_mut();
        let top_builder = WebViewBuilder::new_with_web_context(&mut *web_ctx)
            .with_transparent(true)
            .with_initialization_script(&top_init_js)
            .with_html(SHELL_HTML)
            .with_ipc_handler(move |req| {
                match serde_json::from_str::<IpcMessage>(req.body()) {
                    Ok(msg) => handle_ipc(&state_for_ipc, msg),
                    Err(e) => eprintln!("[Ferro Shell IPC Parse Error] {:?}: {}", e, req.body()),
                }
            });
        match top_builder.build_gtk(&top_box) {
            Ok(wv) => Rc::new(wv),
            Err(e) => {
                panic!("Не удалось создать верхний UI Webview: {:?}", e);
            }
        }
    };
    let top_inner = top_webview.webview();
    top_inner.set_background_color(&initial_surface_rgba);
    top_inner.connect_context_menu(|_wv, _menu, _event, _hit_test| true);
    top_inner.add_events(gdk::EventMask::BUTTON_PRESS_MASK);
    top_inner.connect_button_press_event(move |_w, ev| {
        if ev.button() == 1 {
            last_btn1_press_time.set(ev.time());
        }
        glib::Propagation::Proceed
    });

    state.borrow_mut().top_webview = Some(top_webview);

    let mut detached_focus_inner: Option<webkit2gtk::WebView> = None;
    match initial {
        InitialWindowMode::Startup => {
            let restore_mode = state.borrow().settings.restore_session_mode.clone();
            if restore_mode == "always" {
                if let Some(session) = SessionData::load() {
                    for tab in session.tabs {
                        create_tab(&state, &tab.url);
                    }
                    SessionData::clear();
                } else {
                    create_tab(&state, "ferro://newtab");
                }
            } else {
                create_tab(&state, "ferro://newtab");
            }
        }
        InitialWindowMode::NewBlankWindow => {
            create_tab(&state, "ferro://newtab");
        }
        InitialWindowMode::DetachedTab(spec) => {
            detached_focus_inner = create_detached_tab_in_window(&state, &spec);
        }
    }
    schedule_prewarm_tab(&state);

    // Save session when closing the last remaining window
    let state_for_delete = state.clone();
    window.connect_delete_event(move |_win, _event| {
        if open_windows_count() <= 1 {
            save_current_session(&state_for_delete);
        }
        glib::Propagation::Proceed
    });

    // Global keyboard shortcuts
    let state_for_keys = state.clone();
    window.connect_key_press_event(move |_win, event| {
        let key = event.keyval();
        let modifier = event.state();
        let ctrl = modifier.contains(gdk::ModifierType::CONTROL_MASK);
        let alt = modifier.contains(gdk::ModifierType::MOD1_MASK);
        let shift = modifier.contains(gdk::ModifierType::SHIFT_MASK);

        if ctrl && shift {
            if key == gdk::keys::constants::t || key == gdk::keys::constants::T || key == gdk::keys::constants::Cyrillic_ie || key == gdk::keys::constants::Cyrillic_IE {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "reopen_closed_tab".to_string(),
                    url: None,
                    id: None,
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::b || key == gdk::keys::constants::B || key == gdk::keys::constants::Cyrillic_i || key == gdk::keys::constants::Cyrillic_I {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "toggle_bookmarks_bar".to_string(),
                    url: None,
                    id: None,
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            }
        }

        if ctrl {
            if (key == gdk::keys::constants::t || key == gdk::keys::constants::T || key == gdk::keys::constants::Cyrillic_ie || key == gdk::keys::constants::Cyrillic_IE) && !shift {
                create_tab(&state_for_keys, "ferro://newtab");
                return glib::Propagation::Stop;
            } else if (key == gdk::keys::constants::n || key == gdk::keys::constants::N || key == gdk::keys::constants::Cyrillic_te || key == gdk::keys::constants::Cyrillic_TE) && !shift {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "new_window".to_string(),
                    url: None,
                    id: None,
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::w || key == gdk::keys::constants::W || key == gdk::keys::constants::Cyrillic_tse || key == gdk::keys::constants::Cyrillic_TSE {
                let active_id = state_for_keys.try_borrow().ok().and_then(|s| s.active_tab_id);
                if let Some(id) = active_id {
                    close_tab(&state_for_keys, id);
                }
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::r || key == gdk::keys::constants::R || key == gdk::keys::constants::Cyrillic_ka || key == gdk::keys::constants::Cyrillic_KA {
                let inner = {
                    if let Ok(state) = state_for_keys.try_borrow() {
                        state.active_tab_id.and_then(|id| {
                            state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                        })
                    } else {
                        None
                    }
                };
                if let Some(inner) = inner {
                    inner.reload();
                }
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::l || key == gdk::keys::constants::L || key == gdk::keys::constants::Cyrillic_de || key == gdk::keys::constants::Cyrillic_DE {
                send_to_ui(
                    &state_for_keys,
                    "const inp = document.getElementById('omniboxInput'); if(inp){ inp.focus(); inp.select(); }",
                );
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::h || key == gdk::keys::constants::H || key == gdk::keys::constants::Cyrillic_er || key == gdk::keys::constants::Cyrillic_ER {
                create_tab(&state_for_keys, "ferro://history");
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::j || key == gdk::keys::constants::J || key == gdk::keys::constants::Cyrillic_o || key == gdk::keys::constants::Cyrillic_O {
                create_tab(&state_for_keys, "ferro://downloads");
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::f || key == gdk::keys::constants::F || key == gdk::keys::constants::Cyrillic_a || key == gdk::keys::constants::Cyrillic_A {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "open_find".to_string(),
                    url: None,
                    id: None,
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::p || key == gdk::keys::constants::P || key == gdk::keys::constants::Cyrillic_ze || key == gdk::keys::constants::Cyrillic_ZE {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "print_page".to_string(),
                    url: None,
                    id: None,
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::d || key == gdk::keys::constants::D || key == gdk::keys::constants::Cyrillic_ve || key == gdk::keys::constants::Cyrillic_VE {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "toggle_bookmark_active".to_string(),
                    url: None,
                    id: None,
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::Tab || key == gdk::keys::constants::ISO_Left_Tab {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: if shift { "prev_tab".to_string() } else { "next_tab".to_string() },
                    url: None,
                    id: None,
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            } else if let Some(digit) = key.to_unicode().and_then(|c| c.to_digit(10)) {
                if digit >= 1 && digit <= 9 {
                    handle_ipc(&state_for_keys, IpcMessage {
                        action: "switch_tab_index".to_string(),
                        url: None,
                        id: Some(digit),
                        title: None,
                        pass: None,
                        settings: None,
                    });
                    return glib::Propagation::Stop;
                } else if digit == 0 {
                    handle_ipc(&state_for_keys, IpcMessage {
                        action: "zoom_reset".to_string(),
                        url: None,
                        id: None,
                        title: None,
                        pass: None,
                        settings: None,
                    });
                    return glib::Propagation::Stop;
                }
            } else if key == gdk::keys::constants::plus || key == gdk::keys::constants::equal {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "zoom_step".to_string(),
                    url: None,
                    id: Some(1),
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::minus || key == gdk::keys::constants::underscore {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "zoom_step".to_string(),
                    url: None,
                    id: Some(0),
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            }
        } else if alt {
            if key == gdk::keys::constants::Left {
                let inner = {
                    if let Ok(state) = state_for_keys.try_borrow() {
                        state.active_tab_id.and_then(|id| {
                            state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                        })
                    } else {
                        None
                    }
                };
                if let Some(inner) = inner {
                    if inner.can_go_back() {
                        inner.go_back();
                    }
                }
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::Right {
                let inner = {
                    if let Ok(state) = state_for_keys.try_borrow() {
                        state.active_tab_id.and_then(|id| {
                            state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                        })
                    } else {
                        None
                    }
                };
                if let Some(inner) = inner {
                    if inner.can_go_forward() {
                        inner.go_forward();
                    }
                }
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::d || key == gdk::keys::constants::D {
                send_to_ui(
                    &state_for_keys,
                    "const inp = document.getElementById('omniboxInput'); if(inp){ inp.focus(); inp.select(); }",
                );
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::r || key == gdk::keys::constants::R || key == gdk::keys::constants::Cyrillic_ka || key == gdk::keys::constants::Cyrillic_KA {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "toggle_reader".to_string(),
                    url: None,
                    id: None,
                    title: None,
                    pass: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            }
        } else if key == gdk::keys::constants::F5 {
            let inner = {
                if let Ok(state) = state_for_keys.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.inner.clone())
                    })
                } else {
                    None
                }
            };
            if let Some(inner) = inner {
                inner.reload();
            }
            return glib::Propagation::Stop;
        } else if key == gdk::keys::constants::F12 {
            handle_ipc(&state_for_keys, IpcMessage {
                action: "inspect_element".to_string(),
                url: None,
                id: None,
                title: None,
                pass: None,
                settings: None,
            });
            return glib::Propagation::Stop;
        } else if key == gdk::keys::constants::F11 {
            handle_ipc(&state_for_keys, IpcMessage {
                action: "window_max".to_string(),
                url: None,
                id: None,
                title: None,
                pass: None,
                settings: None,
            });
            return glib::Propagation::Stop;
        }

        glib::Propagation::Proceed
    });

    let state_for_destroy = state.clone();
    window.connect_destroy(move |_| {
        if let Ok(mut st) = state_for_destroy.try_borrow_mut() {
            if let Some((pip_win, _, _)) = st.pip_window.take() {
                pip_win.close();
            }
            st.active_popover = None;
            st.spare_tab = None;
            st.tabs.clear();
            st.top_webview = None;
            st.side_header_webview = None;
            st.side_webview = None;
        }
        let remaining = ALL_WINDOWS.with(|wins| {
            let mut list = wins.borrow_mut();
            list.retain(|w| !Rc::ptr_eq(w, &state_for_destroy));
            list.len()
        });
        if remaining == 0 {
            gtk::main_quit();
        }
    });

    window.show_all();
    side_box.hide();
    if let Some(inner) = detached_focus_inner {
        inner.grab_focus();
        inner.queue_draw();
    }

    // Chrome-style live drag handoff: once the torn-off window commits its first frame on Wayland/X11,
    // immediately attach it to the user's held mouse cursor (`xdg_toplevel.move` / `_NET_WM_MOVERESIZE`)!
    if let Some((true, press_time)) = detach_drag_info {
        let win_for_move = window.clone();
        let mut attempts = 0u8;
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            attempts += 1;
            if !first_frame_drawn.get() && attempts < 10 {
                return glib::ControlFlow::Continue;
            }
            if let Some(display) = gdk::Display::default() {
                if let Some(seat) = display.default_seat() {
                    if let Some(pointer) = seat.pointer() {
                        let (_, rx, ry) = pointer.position();
                        let still_pressed = win_for_move
                            .window()
                            .map(|gw| gw.device_position(&pointer).3.contains(gdk::ModifierType::BUTTON1_MASK))
                            .unwrap_or(true);
                        if still_pressed {
                            win_for_move.present();
                            win_for_move.begin_move_drag(1, rx, ry, press_time);
                        }
                    }
                }
            }
            glib::ControlFlow::Break
        });
    }

    state
}

fn ensure_desktop_shortcut() {
    use std::os::unix::fs::PermissionsExt;

    let Some(home) = dirs::home_dir() else { return };
    const LOGO_PNG: &[u8] = include_bytes!("ui/logo.png");

    let icon_512_dir = home.join(".local/share/icons/hicolor/512x512/apps");
    let icon_256_dir = home.join(".local/share/icons/hicolor/256x256/apps");
    let _ = std::fs::create_dir_all(&icon_512_dir);
    let _ = std::fs::create_dir_all(&icon_256_dir);

    let icon_512_path = icon_512_dir.join("ferro.png");
    let icon_256_path = icon_256_dir.join("ferro.png");
    let _ = std::fs::write(&icon_512_path, LOGO_PNG);
    let _ = std::fs::write(&icon_256_path, LOGO_PNG);

    let repo_dir = env!("CARGO_MANIFEST_DIR");
    let release_bin = std::path::PathBuf::from(repo_dir).join("target/release/ferro");
    let exec_bin = std::env::current_exe()
        .ok()
        .filter(|p| p.exists())
        .unwrap_or_else(|| release_bin.clone());
    let target_bin = if release_bin.exists() { &release_bin } else { &exec_bin };

    let bin_dir = home.join(".local/bin");
    let _ = std::fs::create_dir_all(&bin_dir);
    let launcher_path = bin_dir.join("ferro");
    let _ = std::fs::remove_file(&launcher_path);
    let _ = std::os::unix::fs::symlink(target_bin, &launcher_path);

    let apps_dir = home.join(".local/share/applications");
    let _ = std::fs::create_dir_all(&apps_dir);
    let desktop_path = apps_dir.join("ferro.desktop");
    let desktop_content = format!(
        "[Desktop Entry]\n\
Type=Application\n\
Name=Ferro\n\
GenericName=Web Browser\n\
GenericName[ru]=Веб-браузер\n\
Comment=Быстрый минималистичный браузер Ferro\n\
Exec={bin} %U\n\
TryExec={bin}\n\
Path={repo}\n\
Icon={icon}\n\
Terminal=false\n\
Categories=Network;WebBrowser;\n\
StartupWMClass=ferro\n\
StartupNotify=true\n\
X-GNOME-UsesNotifications=true\n\
MimeType=text/html;text/xml;application/xhtml+xml;x-scheme-handler/http;x-scheme-handler/https;\n",
        bin = target_bin.to_string_lossy(),
        repo = repo_dir,
        icon = icon_512_path.to_string_lossy()
    );
    if std::fs::write(&desktop_path, desktop_content).is_ok() {
        let _ = std::fs::set_permissions(&desktop_path, std::fs::Permissions::from_mode(0o755));
    }
}

/// Places Ferro's main process into its own XDG systemd user application scope (`app-ferro-<pid>.scope`
/// under `app.slice`) BEFORE spawning WebKitGTK child processes (`WebKitNetworkProcess`, `WebKitWebProcess`,
/// `bwrap`, `xdg-dbus-proxy`). Task managers (Mission Center, KDE System Monitor, GNOME System Monitor, Resources)
/// automatically group all processes in `app-ferro-<pid>.scope` into a single "Ferro" entry under "Applications" ("Приложения")!
fn ensure_systemd_app_scope() {
    if let Ok(cg) = std::fs::read_to_string("/proc/self/cgroup") {
        if cg.contains("/app-ferro-") {
            return;
        }
    }
    let pid = std::process::id().to_string();
    let unit = format!("app-ferro-{}.scope", pid);
    let _ = std::process::Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
            "ssa(sv)a(sa(sv))",
            &unit,
            "fail",
            "3",
            "Description",
            "s",
            "Ferro",
            "Slice",
            "s",
            "app.slice",
            "PIDs",
            "au",
            "1",
            &pid,
            "0",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    for _ in 0..16 {
        if let Ok(cg) = std::fs::read_to_string("/proc/self/cgroup") {
            if cg.contains("/app-ferro-") {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn main() {
    ensure_desktop_shortcut();

    let single_instance = ensure_single_instance();
    if single_instance.is_none() {
        return;
    }

    ensure_systemd_app_scope();

    // Ensure GStreamer uses hardware Intel VA-API VP9 (`vavp9dec:MAX`) & H.264 (`vah264dec:MAX`) + GL sink
    // for 1080p60 / 1440p60 / 4K60 playback while disabling slow single-threaded `vp9dec` / `av1dec`!
    if std::env::var_os("WEBKIT_GST_DMABUF_SINK_DISABLED").is_none() {
        std::env::set_var("WEBKIT_GST_DMABUF_SINK_DISABLED", "1");
    }
    if std::env::var_os("GST_PLUGIN_FEATURE_RANK").is_none() {
        std::env::set_var(
            "GST_PLUGIN_FEATURE_RANK",
            "av1dec:NONE,vaav1dec:NONE,vp9dec:NONE,vavp9dec:MAX,vah264dec:MAX,vavp8dec:MAX,avdec_vp9:HIGH,avdec_h264:HIGH",
        );
    }

    glib::set_prgname(Some("ferro"));
    glib::set_application_name("Ferro");
    gtk::init().expect("Не удалось инициализировать GTK");

    let initial_settings = BrowserSettings::load_or_default();
    let adblocker = AdBlocker::new();
    adblocker.set_enabled(initial_settings.adblock_enabled);

    let profile_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("ferro")
        .join("profile");
    let _ = std::fs::create_dir_all(&profile_dir);
    clean_stale_profile_locks(&profile_dir);
    let web_context = Rc::new(RefCell::new(wry::WebContext::new(Some(profile_dir))));

    let css_provider = gtk::CssProvider::new();
    update_gtk_menu_css(&css_provider, &initial_settings);
    gtk::StyleContext::add_provider_for_screen(
        &gdk::Screen::default().expect("Error initializing screen"),
        &css_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let settings_dirty = Arc::new(AtomicBool::new(false));
    let bookmarks_dirty = Arc::new(AtomicBool::new(false));
    let history_dirty = Arc::new(AtomicBool::new(false));
    let passwords_dirty = Arc::new(AtomicBool::new(false));
    let downloads = Arc::new(Mutex::new(DownloadsData::load_or_default()));
    let lens_preview = Arc::new(Mutex::new(String::new()));
    let page_source_cache = Arc::new(Mutex::new((String::new(), String::new())));
    let native_filter: Rc<RefCell<Option<WebKitFilter>>> = Rc::new(RefCell::new(None));

    let core = SharedBrowserCore {
        web_context,
        adblocker,
        native_filter: native_filter.clone(),
        css_provider,
        downloads,
        lens_preview,
        page_source_cache,
        settings_dirty: settings_dirty.clone(),
        bookmarks_dirty: bookmarks_dirty.clone(),
        history_dirty: history_dirty.clone(),
        passwords_dirty: passwords_dirty.clone(),
        download_handler_attached: Rc::new(Cell::new(false)),
    };

    if let Some(listener) = single_instance {
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::Read;
                let mut buf = [0u8; 64];
                if let Ok(n) = stream.read(&mut buf) {
                    let msg = String::from_utf8_lossy(&buf[..n]);
                    if msg.contains("QUIT") {
                        let wins = open_windows_snapshot();
                        if let Some(first) = wins.first() {
                            save_current_session(first);
                        }
                        for w in wins {
                            if let Ok(st) = w.try_borrow() {
                                st.window.close();
                            }
                        }
                    } else if msg.contains("FOCUS") {
                        let wins = open_windows_snapshot();
                        if let Some(last) = wins.last() {
                            if let Ok(st) = last.try_borrow() {
                                st.window.deiconify();
                                st.window.present();
                            }
                        }
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // Background watcher for settings/bookmarks/history/passwords saved via ferro://api/* custom protocol
    glib::timeout_add_local(std::time::Duration::from_millis(180), move || {
        if settings_dirty.swap(false, Ordering::SeqCst) {
            broadcast_settings_from_disk();
        }
        if bookmarks_dirty.swap(false, Ordering::SeqCst) {
            broadcast_bookmarks_from_disk();
        }
        if history_dirty.swap(false, Ordering::SeqCst) {
            let loaded = HistoryData::load_or_default();
            for win_rc in open_windows_snapshot() {
                if let Ok(mut st) = win_rc.try_borrow_mut() {
                    st.history = loaded.clone();
                }
            }
        }
        if passwords_dirty.swap(false, Ordering::SeqCst) {
            let loaded = PasswordsData::load_or_default();
            for win_rc in open_windows_snapshot() {
                if let Ok(mut st) = win_rc.try_borrow_mut() {
                    st.passwords = loaded.clone();
                }
            }
        }
        glib::ControlFlow::Continue
    });

    // Fast-path cached WebKit filter load; background compilation ONLY when cache is missing/outdated!
    let store_dir = AdBlocker::filter_store_dir();
    let store_dir_str = store_dir.to_string_lossy().to_string();
    let stamp_path = store_dir.join("rules.stamp");
    const FILTER_STAMP: &str = "ferro-webkit-rules-v4";
    let stamp_ok = std::fs::read_to_string(&stamp_path)
        .map(|s| s.trim() == FILTER_STAMP)
        .unwrap_or(false);

    let filter_slot = native_filter;
    let store_dir_for_load = store_dir_str.clone();
    let stamp_path_clone = stamp_path.clone();
    load_filter(&store_dir_str, "ferro_adblock", move |filter_opt| {
        if stamp_ok {
            if let Some(filter) = filter_opt {
                println!("[Ferro AdBlock] Cached native WebKit filter attached immediately!");
                let filter_slot_ok = filter_slot.clone();
                glib::idle_add_local_once(move || {
                    *filter_slot_ok.borrow_mut() = Some(filter.clone());
                    for win_rc in open_windows_snapshot() {
                        if let Ok(st) = win_rc.try_borrow() {
                            if st.adblocker.is_enabled() {
                                for tab in &st.tabs {
                                    if let Some(ucm) = tab.inner.user_content_manager() {
                                        attach_to_ucm(&ucm, &filter);
                                    }
                                }
                            }
                        }
                    }
                });
                return;
            }
        }

        // Compile rules JSON on a background worker thread so GTK UI startup is never blocked
        let filter_slot_bg = filter_slot.clone();
        let store_dir_bg = store_dir_for_load.clone();
        let stamp_bg = stamp_path_clone.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let compiled = AdBlocker::compile_webkit_rules_json();
            let _ = tx.send(compiled);
        });
        glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
            match rx.try_recv() {
                Ok(rules_json) => {
                    let filter_slot_done = filter_slot_bg.clone();
                    let stamp_done = stamp_bg.clone();
                    save_filter(&store_dir_bg, "ferro_adblock", &rules_json, move |filter| {
                        let _ = std::fs::write(&stamp_done, FILTER_STAMP);
                        println!("[Ferro AdBlock] Background WebKit uBlock filter compiled and cached!");
                        let filter_slot_apply = filter_slot_done.clone();
                        glib::idle_add_local_once(move || {
                            *filter_slot_apply.borrow_mut() = Some(filter.clone());
                            for win_rc in open_windows_snapshot() {
                                if let Ok(st) = win_rc.try_borrow() {
                                    if st.adblocker.is_enabled() {
                                        for tab in &st.tabs {
                                            if let Some(ucm) = tab.inner.user_content_manager() {
                                                attach_to_ucm(&ucm, &filter);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    });
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    });

    spawn_browser_window(&core, InitialWindowMode::Startup);
    gtk::main();
}
