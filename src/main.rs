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
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use webkit2gtk::{
    CacheModel, DownloadExt, FindControllerExt, FindOptions, HardwareAccelerationPolicy, LoadEvent,
    NavigationPolicyDecision, NavigationPolicyDecisionExt, PolicyDecisionExt, PolicyDecisionType,
    PrintOperation, PrintOperationExt, SettingsExt, URIRequestExt, WebContextExt, WebInspectorExt,
    WebViewExt, WebsiteDataManagerExtManual, WebsiteDataTypes,
};
use wry::{WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

mod session;

use adblocker::AdBlocker;
use bookmarks::BookmarksData;
use downloads::DownloadsData;
use history::HistoryData;
use importer::BrowserImporter;
use lens::{
    base64_decode, base64_encode, render_bookmarks_panel_html, render_lens_loading_html,
    render_side_header_html, render_sleeping_tab_html, upload_png_to_google_lens,
    upload_url_to_google_lens,
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
    webview: Rc<WebView>,
    inner: webkit2gtk::WebView,
    container: gtk::Box,
}

struct BrowserState {
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
    css_provider: gtk::CssProvider,
    top_webview: Option<Rc<WebView>>,
    tabs: Vec<Tab>,
    closed_tabs: Vec<(String, String)>,
    active_tab_id: Option<u32>,
    next_tab_id: u32,
    bookmarks: BookmarksData,
    history: HistoryData,
    passwords: PasswordsData,
    downloads: Arc<Mutex<DownloadsData>>,
    pending_password: Option<(String, String, String)>,
    settings: BrowserSettings,
    web_context: wry::WebContext,
    adblocker: Arc<AdBlocker>,
    native_filter: Option<WebKitFilter>,
    dark_mode: bool,
    theme: String,
    download_handler_attached: bool,
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
    if s.theme_mode == "light" {
        gdk::RGBA::new(244.0 / 255.0, 244.0 / 255.0, 246.0 / 255.0, alpha)
    } else {
        gdk::RGBA::new(20.0 / 255.0, 20.0 / 255.0, 20.0 / 255.0, alpha)
    }
}

fn update_gtk_menu_css(css_provider: &gtk::CssProvider, s: &BrowserSettings) {
    let is_light = s.theme_mode == "light";
    let alpha = ((100 - s.ui_transparency.min(60)) as f64) / 100.0;
    let surface_css = if is_light {
        format!("rgba(244, 244, 246, {:.2})", alpha)
    } else {
        format!("rgba(20, 20, 20, {:.2})", alpha)
    };
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
        menu {{
            background-color: {bg};
            border: 1px solid {border};
            border-radius: {rad}px;
            padding: 4px;
        }}
        menuitem {{
            color: {fg};
            font-size: 13px;
            padding: 6px 14px;
            border-radius: {item_rad}px;
        }}
        menuitem:hover {{
            background-color: {hover_bg};
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

fn attach_download_handler(
    ctx: &webkit2gtk::WebContext,
    state_weak: Weak<RefCell<BrowserState>>,
    downloads_store: Arc<Mutex<DownloadsData>>,
) {
    ctx.connect_download_started(move |_ctx, download| {
        let state_weak_dest = state_weak.clone();
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
            if let Some(state_rc) = state_weak_dest.upgrade() {
                let active_wv = state_rc.try_borrow().ok().and_then(|s| {
                    s.active_tab_id.and_then(|id| {
                        s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone())
                    })
                });
                if let Some(wv) = active_wv {
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

        let state_weak_fin = state_weak.clone();
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
            if let Some(state_rc) = state_weak_fin.upgrade() {
                let active_wv = state_rc.try_borrow().ok().and_then(|s| {
                    s.active_tab_id.and_then(|id| {
                        s.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone())
                    })
                });
                if let Some(wv) = active_wv {
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
            }
        });
    });
}

fn sync_settings_to_ui(state_rc: &Rc<RefCell<BrowserState>>) {
    let (settings_json, top_wv, tab_wvs) = {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            update_gtk_menu_css(&state.css_provider, &state.settings);
            let top_h = if state.settings.show_bookmarks_bar { 102 } else { 74 };
            state.top_box.set_size_request(-1, top_h);

            let surface_rgba = compute_surface_rgba(&state.settings);
            let clamped_zoom = state.settings.default_zoom.clamp(50, 200);
            let zoom_factor = (clamped_zoom as f64) / 100.0;
            let smooth_scrolling = state.settings.smooth_scrolling;
            let enable_webgl = state.settings.enable_webgl;
            let enable_js = state.settings.enable_javascript;
            let adblock_enabled = state.settings.adblock_enabled;
            let native_filter = state.native_filter.clone();
            state.adblocker.set_enabled(adblock_enabled);

            for t in &mut state.tabs {
                t.inner.set_background_color(&surface_rgba);
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
                sp.inner.set_background_color(&surface_rgba);
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
    let needs_init = state_rc.try_borrow().map(|s| s.side_webview.is_none()).unwrap_or(false);
    if needs_init {
        if let Ok(mut st) = state_rc.try_borrow_mut() {
            let hdr_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            hdr_box.set_size_request(-1, 36);
            st.side_box.pack_start(&hdr_box, false, false, 0);

            let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            st.side_box.pack_start(&content_box, true, true, 0);

            let surface_rgba = compute_surface_rgba(&st.settings);
            let s_json = serde_json::to_string(&st.settings).unwrap_or_else(|_| "{}".to_string());
            let hdr_init = format!(
                "window.addEventListener('DOMContentLoaded', function() {{ if (window.applySettings) window.applySettings({}); }});",
                s_json
            );

            let state_for_hdr = state_rc.clone();
            if let Ok(hdr_wv) = WebViewBuilder::new()
                .with_transparent(true)
                .with_initialization_script(&hdr_init)
                .with_html(render_side_header_html())
                .with_ipc_handler(move |req| {
                    if let Ok(msg) = serde_json::from_str::<IpcMessage>(req.body()) {
                        handle_ipc(&state_for_hdr, msg);
                    }
                })
                .build_gtk(&hdr_box)
            {
                hdr_wv.webview().set_background_color(&surface_rgba);
                hdr_wv.webview().connect_context_menu(|_, _, _, _| true);
                st.side_header_webview = Some(Rc::new(hdr_wv));
            }

            let state_for_body = state_rc.clone();
            if let Ok(body_wv) = WebViewBuilder::new_with_web_context(&mut st.web_context)
                .with_transparent(true)
                .with_ipc_handler(move |req| {
                    if let Ok(msg) = serde_json::from_str::<IpcMessage>(req.body()) {
                        handle_ipc(&state_for_body, msg);
                    }
                })
                .build_gtk(&content_box)
            {
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
                st.side_webview = Some(Rc::new(body_wv));
            }
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

fn create_tab(state_rc: &Rc<RefCell<BrowserState>>, target_url: &str) -> u32 {
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

fn build_raw_tab_webview(state_rc: &Rc<RefCell<BrowserState>>, target_url: &str) -> Option<PrewarmedTab> {
    let (
        tab_id,
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
    ) = {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            let id = state.next_tab_id;
            state.next_tab_id += 1;
            let autofill = state.passwords.generate_autofill_script();
            let theme_css = state.settings.generate_css_variables();
            let s_snap = state.settings.clone();
            let s_json = serde_json::to_string(&state.settings).unwrap_or_else(|_| "{}".to_string());
            (
                id,
                state.adblocker.clone(),
                state.native_filter.clone(),
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
            )
        } else {
            return None;
        }
    };

    let tab_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let tab_name = format!("tab_{}", tab_id);
    stack.add_named(&tab_box, &tab_name);

    let init_script = format!(
        r#"
        try {{ window._ferro_settings = {settings_json}; }} catch(e) {{}}
        (function() {{
            if (window.location.href.startsWith('ferro://settings')) return;
            function injectDynamicTheme() {{
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

    let mut state_mut = match state_rc.try_borrow_mut() {
        Ok(s) => s,
        Err(_) => return None,
    };

    let protocol_already_registered = state_mut.web_context.is_custom_protocol_registered("ferro");
    let mut builder = WebViewBuilder::new_with_web_context(&mut state_mut.web_context);
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

    let state_for_tab_ipc = state_rc.clone();
    let builder = builder
        .with_initialization_script(&init_script)
        .with_transparent(true)
        .with_ipc_handler(move |req| {
            match serde_json::from_str::<IpcMessage>(req.body()) {
                Ok(msg) => handle_ipc(&state_for_tab_ipc, msg),
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
    drop(state_mut);

    let inner_wv = webview.webview();

    // Immediately set theme surface background color on the WebKitGTK widget BEFORE loading URI
    // so there is NEVER a transparent see-through desktop flash when opening a new tab!
    inner_wv.set_background_color(&compute_surface_rgba(&settings_snapshot));
    inner_wv.set_zoom_level((settings_snapshot.default_zoom.clamp(50, 200) as f64) / 100.0);

    // Attach global WebKit download handler & WebBrowser cache model once on the shared WebContext
    {
        if let Ok(mut st) = state_rc.try_borrow_mut() {
            if !st.download_handler_attached {
                if let Some(wk_ctx) = inner_wv.context() {
                    wk_ctx.set_cache_model(CacheModel::WebBrowser);
                    attach_download_handler(&wk_ctx, Rc::downgrade(state_rc), st.downloads.clone());
                    st.download_handler_attached = true;
                }
            }
        }
    }

    // Set Chrome User-Agent, hardware acceleration, bfcache, DNS prefetch, fullscreen & user engine settings
    if let Some(settings) = WebViewExt::settings(&inner_wv) {
        settings.set_user_agent(Some(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        ));
        settings.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Always);
        settings.set_enable_page_cache(true);
        settings.set_enable_dns_prefetching(true);
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

    // Hook WebKitGTK signals
    let state_weak: Weak<RefCell<BrowserState>> = Rc::downgrade(state_rc);
    let tab_id_copy = tab_id;
    inner_wv.connect_load_changed(move |wv, event| {
        if let Some(state_rc) = state_weak.upgrade() {
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
    let state_weak_fs_in = Rc::downgrade(state_rc);
    inner_wv.connect_enter_fullscreen(move |_wv| {
        if let Some(state_rc) = state_weak_fs_in.upgrade() {
            if let Ok(state) = state_rc.try_borrow() {
                state.top_box.hide();
            }
        }
        false
    });
    let state_weak_fs_out = Rc::downgrade(state_rc);
    inner_wv.connect_leave_fullscreen(move |_wv| {
        if let Some(state_rc) = state_weak_fs_out.upgrade() {
            if let Ok(state) = state_rc.try_borrow() {
                state.top_box.show();
            }
        }
        false
    });

    let state_weak = Rc::downgrade(state_rc);
    inner_wv.connect_uri_notify(move |wv| {
        if let Some(state_rc) = state_weak.upgrade() {
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

    let state_weak = Rc::downgrade(state_rc);
    inner_wv.connect_title_notify(move |wv| {
        if let Some(state_rc) = state_weak.upgrade() {
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
    let state_weak = Rc::downgrade(state_rc);
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
                            if let Some(state_rc) = state_weak.upgrade() {
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
                            if adblocker_policy.should_block_url(uri_str) {
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

fn spawn_lens_background_search(
    state_rc: &Rc<RefCell<BrowserState>>,
    png_bytes: Option<Vec<u8>>,
    fallback_url: String,
    preview_b64: String,
) {
    if let Ok(st) = state_rc.try_borrow() {
        if let Ok(mut guard) = st.lens_preview.lock() {
            *guard = preview_b64;
        }
    }
    ensure_and_open_side_panel(state_rc, "lens", "ferro://lens-loading");

    let state_weak = Rc::downgrade(state_rc);
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    std::thread::spawn(move || {
        let mut resolved = None;
        if let Some(bytes) = png_bytes {
            resolved = upload_png_to_google_lens(&bytes);
        }
        if resolved.is_none() && !fallback_url.is_empty() {
            resolved = upload_url_to_google_lens(&fallback_url);
            if resolved.is_none() && (fallback_url.starts_with("http://") || fallback_url.starts_with("https://")) {
                let enc = percent_encoding::utf8_percent_encode(&fallback_url, percent_encoding::NON_ALPHANUMERIC);
                resolved = Some(format!("https://lens.google.com/uploadbyurl?hl=ru&url={}", enc));
            }
        }
        let _ = tx.send(resolved);
    });

    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        match rx.try_recv() {
            Ok(Some(lens_result_url)) => {
                if let Some(s_rc) = state_weak.upgrade() {
                    let side_wv = {
                        if let Ok(mut st) = s_rc.try_borrow_mut() {
                            st.last_lens_result_url = Some(lens_result_url.clone());
                            st.side_webview.clone()
                        } else {
                            None
                        }
                    };
                    if let Some(wv) = side_wv {
                        wv.webview().load_uri(&lens_result_url);
                    }
                }
                glib::ControlFlow::Break
            }
            Ok(None) => glib::ControlFlow::Break,
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
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
            sync_tabs_to_ui(state_rc);
            sync_bookmarks_to_ui(state_rc);
            sync_adblock_to_ui(state_rc);
            sync_settings_to_ui(state_rc);
            let dark_mode = state_rc.try_borrow().map(|s| s.dark_mode).unwrap_or(false);
            send_to_ui(state_rc, &format!("if (window.ferro) {{ window.ferro.setDarkModeStatus({}); }}", dark_mode));
        }
        "sync_settings" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.settings = BrowserSettings::load_or_default();
            }
            sync_settings_to_ui(state_rc);
        }
        "save_settings" => {
            if let Some(new_s) = msg.settings {
                let _ = new_s.save();
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    state.theme = new_s.theme_mode.clone();
                    state.settings = new_s;
                }
            } else if let Ok(mut state) = state_rc.try_borrow_mut() {
                let loaded = BrowserSettings::load_or_default();
                state.theme = loaded.theme_mode.clone();
                state.settings = loaded;
            }
            sync_settings_to_ui(state_rc);
        }
        "reload_bookmarks" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.bookmarks = BookmarksData::load_or_default();
            }
            sync_bookmarks_to_ui(state_rc);
        }
        "new_tab" => {
            let url = msg.url.as_deref().unwrap_or("ferro://newtab");
            create_tab(state_rc, url);
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
                sync_bookmarks_to_ui(state_rc);
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
            let active = {
                if let Ok(state) = state_rc.try_borrow() {
                    state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| (t.webview.clone(), t.inner.clone()))
                    })
                } else {
                    None
                }
            };
            if let (Some(url), Some((wry_wv, inner))) = (msg.url, active) {
                if url.starts_with("ferro://") {
                    let _ = wry_wv.load_url(&url);
                } else {
                    inner.load_uri(&url);
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
                sync_bookmarks_to_ui(state_rc);
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
            let info = {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    let enabled = state.adblocker.toggle_enabled();
                    state.settings.adblock_enabled = enabled;
                    let _ = state.settings.save();
                    let ucms: Vec<_> = state.tabs.iter().filter_map(|t| t.inner.user_content_manager()).collect();
                    Some((enabled, state.native_filter.clone(), ucms))
                } else {
                    None
                }
            };

            if let Some((is_enabled, filter, tabs_ucm)) = info {
                for ucm in tabs_ucm {
                    if let Some(filter) = &filter {
                        if is_enabled {
                            attach_to_ucm(&ucm, filter);
                        } else {
                            detach_from_ucm(&ucm, filter);
                        }
                    }
                }
            }
            sync_settings_to_ui(state_rc);
        }
        "toggle_bookmarks_bar" => {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                state.settings.show_bookmarks_bar = !state.settings.show_bookmarks_bar;
                let _ = state.settings.save();
            }
            sync_settings_to_ui(state_rc);
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

                    let cap_info = state_rc.try_borrow().ok().and_then(|st| {
                        let alloc = st.stack.allocation();
                        st.window.window().map(|gw| (gw, alloc))
                    });

                    if let Some((gdk_win, alloc)) = cap_info {
                        let sx = (alloc.width() as f64) / vw;
                        let sy = (alloc.height() as f64) / vh;
                        let px = (alloc.x() + (rx * sx).round() as i32).max(0);
                        let py = (alloc.y() + (ry * sy).round() as i32).max(0);
                        let pw = ((rw * sx).round() as i32).max(12).min((alloc.width() - (px - alloc.x())).max(12));
                        let ph = ((rh * sy).round() as i32).max(12).min((alloc.height() - (py - alloc.y())).max(12));

                        let mut png_bytes_opt = None;
                        let mut b64_png = String::new();
                        if let Some(pixbuf) = gdk_win.pixbuf(px, py, pw, ph) {
                            if let Ok(png_bytes) = pixbuf.save_to_bufferv("png", &[]) {
                                b64_png = base64_encode(&png_bytes);
                                png_bytes_opt = Some(png_bytes);
                            }
                        }

                        spawn_lens_background_search(state_rc, png_bytes_opt, fallback, b64_png);
                    }
                }
            }
        }
        "lens_image_data" => {
            let b64_data = msg.title.unwrap_or_default();
            let fallback = msg.url.unwrap_or_default();
            let png_bytes_opt = base64_decode(&b64_data);
            spawn_lens_background_search(state_rc, png_bytes_opt, fallback, b64_data);
        }
        "lens_url" => {
            if let Some(img_url) = msg.url {
                spawn_lens_background_search(state_rc, None, img_url, String::new());
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
            sync_settings_to_ui(state_rc);
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
            let info = {
                if let Ok(state) = state_rc.try_borrow() {
                    let (w, h) = state.window.size();
                    let top_h = if state.settings.show_bookmarks_bar { 102 } else { 74 };
                    let dl_dir = state.settings.download_dir.clone();
                    let active_wv = state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone())
                    });
                    state.window.window().map(|gw| (gw, w, h, top_h, dl_dir, active_wv))
                } else {
                    None
                }
            };
            if let Some((gdk_win, w, h, top_h, dl_dir, active_wv)) = info {
                let cap_h = (h - top_h).max(1);
                if let Some(pixbuf) = gdk_win.pixbuf(0, top_h, w, cap_h) {
                    let dir = if !dl_dir.trim().is_empty() {
                        std::path::PathBuf::from(dl_dir)
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
                        if let Some(wv) = active_wv {
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
                            let _ = wv.evaluate_script(&toast_js);
                        }
                    }
                }
            }
        }
        "tab_context_menu" => {
            if let Some(tab_id) = msg.id {
                let tab_url = msg.url.unwrap_or_else(|| "ferro://newtab".to_string());
                let state_weak = Rc::downgrade(state_rc);
                glib::idle_add_local_once(move || {
                    if let Some(state_rc) = state_weak.upgrade() {
                        let (is_pinned, is_muted) = state_rc
                            .try_borrow()
                            .ok()
                            .and_then(|st| {
                                st.tabs
                                    .iter()
                                    .find(|t| t.id == tab_id)
                                    .map(|t| (t.is_pinned, t.is_muted))
                            })
                            .unwrap_or((false, false));

                        let menu = gtk::Menu::new();

                        let item_new = gtk::MenuItem::with_label("Новая вкладка                   Ctrl+T");
                        let s_rc = state_rc.clone();
                        item_new.connect_activate(move |_| {
                            create_tab(&s_rc, "ferro://newtab");
                        });
                        menu.append(&item_new);

                        let item_reload = gtk::MenuItem::with_label("Обновить вкладку            Ctrl+R");
                        let s_rc = state_rc.clone();
                        item_reload.connect_activate(move |_| {
                            if let Ok(state) = s_rc.try_borrow() {
                                if let Some(t) = state.tabs.iter().find(|t| t.id == tab_id) {
                                    t.inner.reload();
                                }
                            }
                        });
                        menu.append(&item_reload);

                        let item_dup = gtk::MenuItem::with_label("Дублировать вкладку");
                        let s_rc = state_rc.clone();
                        let dup_url = tab_url.clone();
                        item_dup.connect_activate(move |_| {
                            create_tab(&s_rc, &dup_url);
                        });
                        menu.append(&item_dup);

                        let pin_lbl = if is_pinned {
                            "Открепить вкладку"
                        } else {
                            "Закрепить вкладку"
                        };
                        let item_pin = gtk::MenuItem::with_label(pin_lbl);
                        let s_rc = state_rc.clone();
                        item_pin.connect_activate(move |_| {
                            handle_ipc(&s_rc, IpcMessage {
                                action: "toggle_pin_tab".to_string(),
                                url: None,
                                id: Some(tab_id),
                                title: None,
                                pass: None,
                                settings: None,
                            });
                        });
                        menu.append(&item_pin);

                        let mute_lbl = if is_muted {
                            "Включить звук вкладки"
                        } else {
                            "Выключить звук вкладки"
                        };
                        let item_mute = gtk::MenuItem::with_label(mute_lbl);
                        let s_rc = state_rc.clone();
                        item_mute.connect_activate(move |_| {
                            handle_ipc(&s_rc, IpcMessage {
                                action: "toggle_mute_tab".to_string(),
                                url: None,
                                id: Some(tab_id),
                                title: None,
                                pass: None,
                                settings: None,
                            });
                        });
                        menu.append(&item_mute);

                        let is_active = state_rc
                            .try_borrow()
                            .ok()
                            .map(|st| st.active_tab_id == Some(tab_id))
                            .unwrap_or(false);
                        if !is_active {
                            let item_sleep = gtk::MenuItem::with_label("Усыпить вкладку (освободить ОЗУ)");
                            let s_rc = state_rc.clone();
                            item_sleep.connect_activate(move |_| {
                                handle_ipc(&s_rc, IpcMessage {
                                    action: "sleep_tab".to_string(),
                                    url: None,
                                    id: Some(tab_id),
                                    title: None,
                                    pass: None,
                                    settings: None,
                                });
                            });
                            menu.append(&item_sleep);
                        }

                        menu.append(&gtk::SeparatorMenuItem::new());

                        let item_close_right = gtk::MenuItem::with_label("Закрыть вкладки справа");
                        let s_rc = state_rc.clone();
                        item_close_right.connect_activate(move |_| {
                            handle_ipc(&s_rc, IpcMessage {
                                action: "close_tabs_right".to_string(),
                                url: None,
                                id: Some(tab_id),
                                title: None,
                                pass: None,
                                settings: None,
                            });
                        });
                        menu.append(&item_close_right);

                        let item_close_others = gtk::MenuItem::with_label("Закрыть другие вкладки");
                        let s_rc = state_rc.clone();
                        item_close_others.connect_activate(move |_| {
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
                        });
                        menu.append(&item_close_others);

                        let item_close = gtk::MenuItem::with_label("Закрыть вкладку              Ctrl+W");
                        let s_rc = state_rc.clone();
                        item_close.connect_activate(move |_| {
                            close_tab(&s_rc, tab_id);
                        });
                        menu.append(&item_close);

                        menu.show_all();
                        menu.popup_easy(3, gtk::current_event_time());
                    }
                });
            }
        }
        "bookmark_context_menu" => {
            if let Some(bm_url) = msg.url {
                let bm_title = msg.title.unwrap_or_else(|| bm_url.clone());
                let state_weak = Rc::downgrade(state_rc);
                glib::idle_add_local_once(move || {
                    if let Some(state_rc) = state_weak.upgrade() {
                        let menu = gtk::Menu::new();

                        let item_open = gtk::MenuItem::with_label("Открыть");
                        let s_rc = state_rc.clone();
                        let u1 = bm_url.clone();
                        item_open.connect_activate(move |_| {
                            handle_ipc(&s_rc, IpcMessage {
                                action: "navigate".to_string(),
                                url: Some(u1.clone()),
                                id: None,
                                title: None,
                                pass: None,
                                settings: None,
                            });
                        });
                        menu.append(&item_open);

                        let item_new_tab = gtk::MenuItem::with_label("Открыть в новой вкладке");
                        let s_rc = state_rc.clone();
                        let u2 = bm_url.clone();
                        item_new_tab.connect_activate(move |_| {
                            create_tab(&s_rc, &u2);
                        });
                        menu.append(&item_new_tab);

                        menu.append(&gtk::SeparatorMenuItem::new());

                        let item_del = gtk::MenuItem::with_label("Удалить из закладок");
                        let s_rc = state_rc.clone();
                        let u3 = bm_url.clone();
                        let t3 = bm_title.clone();
                        item_del.connect_activate(move |_| {
                            if let Ok(mut state) = s_rc.try_borrow_mut() {
                                state.bookmarks.toggle(&u3, &t3);
                            }
                            sync_bookmarks_to_ui(&s_rc);
                        });
                        menu.append(&item_del);

                        menu.show_all();
                        menu.popup_easy(3, gtk::current_event_time());
                    }
                });
            }
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
            let state_weak = Rc::downgrade(state_rc);
            glib::idle_add_local_once(move || {
                if let Some(state_rc) = state_weak.upgrade() {
                    let menu = gtk::Menu::new();

                    let item_new_tab = gtk::MenuItem::with_label("Новая вкладка                   Ctrl+T");
                    let s_rc = state_rc.clone();
                    item_new_tab.connect_activate(move |_| {
                        create_tab(&s_rc, "ferro://newtab");
                    });
                    menu.append(&item_new_tab);

                    let item_reopen = gtk::MenuItem::with_label("Восстановить вкладку   Ctrl+Shift+T");
                    let s_rc = state_rc.clone();
                    item_reopen.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "reopen_closed_tab".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_reopen);

                    menu.append(&gtk::SeparatorMenuItem::new());

                    let item_history = gtk::MenuItem::with_label("История просмотров         Ctrl+H");
                    let s_rc = state_rc.clone();
                    item_history.connect_activate(move |_| {
                        create_tab(&s_rc, "ferro://history");
                    });
                    menu.append(&item_history);

                    let item_downloads = gtk::MenuItem::with_label("Загрузки                             Ctrl+J");
                    let s_rc = state_rc.clone();
                    item_downloads.connect_activate(move |_| {
                        create_tab(&s_rc, "ferro://downloads");
                    });
                    menu.append(&item_downloads);

                    let item_passwords = gtk::MenuItem::with_label("Менеджер паролей");
                    let s_rc = state_rc.clone();
                    item_passwords.connect_activate(move |_| {
                        create_tab(&s_rc, "ferro://passwords");
                    });
                    menu.append(&item_passwords);

                    let item_side = gtk::MenuItem::with_label("Боковая панель закладок");
                    let s_rc = state_rc.clone();
                    item_side.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "toggle_side_panel".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_side);

                    menu.append(&gtk::SeparatorMenuItem::new());

                    let item_lens = gtk::MenuItem::with_label("Поиск с Google Объективом");
                    let s_rc = state_rc.clone();
                    item_lens.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "open_lens_overlay".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_lens);

                    let item_reader = gtk::MenuItem::with_label("Режим чтения                     Alt+R");
                    let s_rc = state_rc.clone();
                    item_reader.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "toggle_reader".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_reader);

                    let item_pip = gtk::MenuItem::with_label("Картинка в картинке (PiP)");
                    let s_rc = state_rc.clone();
                    item_pip.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "toggle_pip".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_pip);

                    let item_qr = gtk::MenuItem::with_label("QR-код страницы");
                    let s_rc = state_rc.clone();
                    item_qr.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "show_qr".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_qr);

                    let item_find = gtk::MenuItem::with_label("Найти на странице            Ctrl+F");
                    let s_rc = state_rc.clone();
                    item_find.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "open_find".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_find);

                    let item_print = gtk::MenuItem::with_label("Печать / Сохранить PDF   Ctrl+P");
                    let s_rc = state_rc.clone();
                    item_print.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "print_page".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_print);

                    let item_shot = gtk::MenuItem::with_label("Снимок экрана");
                    let s_rc = state_rc.clone();
                    item_shot.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "take_screenshot".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_shot);

                    menu.append(&gtk::SeparatorMenuItem::new());

                    let (ab_label, th_label) = {
                        if let Ok(state) = state_rc.try_borrow() {
                            let ab = if state.adblocker.is_enabled() {
                                format!("Блокировщик рекламы: Вкл ({})", state.adblocker.blocked_count())
                            } else {
                                "Блокировщик рекламы: Выкл".to_string()
                            };
                            let th = if state.theme == "light" {
                                "Тема: Светлая (переключить)".to_string()
                            } else {
                                "Тема: Тёмная (переключить)".to_string()
                            };
                            (ab, th)
                        } else {
                            ("Блокировщик рекламы".to_string(), "Тема оформления".to_string())
                        }
                    };

                    let item_adblock = gtk::MenuItem::with_label(&ab_label);
                    let s_rc = state_rc.clone();
                    item_adblock.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "toggle_adblock".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_adblock);

                    let item_theme = gtk::MenuItem::with_label(&th_label);
                    let s_rc = state_rc.clone();
                    item_theme.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "toggle_theme".to_string(), url: None, id: None, title: None, pass: None, settings: None });
                    });
                    menu.append(&item_theme);

                    menu.show_all();

                    let win_info = {
                        if let Ok(state) = state_rc.try_borrow() {
                            let (w, _) = state.window.size();
                            state.window.window().map(|gw| (gw, w))
                        } else {
                            None
                        }
                    };

                    if let Some((gdk_win, win_w)) = win_info {
                        let menu_x = (win_w - 255).max(10);
                        let rect = gtk::gdk::Rectangle::new(menu_x, 74, 245, 1);
                        menu.popup_at_rect(
                            &gdk_win,
                            &rect,
                            gtk::gdk::Gravity::SouthWest,
                            gtk::gdk::Gravity::NorthWest,
                            None,
                        );
                    } else {
                        menu.popup_easy(3, gtk::current_event_time());
                    }
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
            if let Ok(state) = state_rc.try_borrow() {
                if state.window.is_maximized() {
                    state.window.unmaximize();
                } else {
                    state.window.maximize();
                }
            }
        }
        "window_close" => {
            save_current_session(state_rc);
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

fn main() {
    let single_instance = ensure_single_instance();
    if single_instance.is_none() {
        return;
    }

    gtk::init().expect("Не удалось инициализировать GTK");

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Ferro");
    window.set_default_size(1180, 780);
    window.set_decorated(false); // Obsidian / Chrome borderless 0px geometry
    window.set_position(gtk::WindowPosition::Center);

    if let Some(screen) = gdk::Screen::default() {
        if let Some(visual) = screen.rgba_visual() {
            window.set_visual(Some(&visual));
        }
    }
    window.set_app_paintable(true);

    if let Some(listener) = single_instance {
        let win_clone = window.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::Read;
                let mut buf = [0u8; 64];
                if let Ok(n) = stream.read(&mut buf) {
                    let msg = String::from_utf8_lossy(&buf[..n]);
                    if msg.contains("QUIT") {
                        win_clone.close();
                    } else if msg.contains("FOCUS") {
                        win_clone.deiconify();
                        win_clone.present();
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

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
    let adblocker = AdBlocker::new();
    let settings = BrowserSettings::load_or_default();
    adblocker.set_enabled(settings.adblock_enabled);
    let initial_top_h = if settings.show_bookmarks_bar { 102 } else { 74 };
    top_box.set_size_request(-1, initial_top_h);
    vbox.pack_start(&top_box, false, false, 0);

    let hpaned = gtk::Paned::new(gtk::Orientation::Horizontal);
    let stack = gtk::Stack::new();
    let side_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    hpaned.pack1(&stack, true, false);
    hpaned.pack2(&side_box, false, false);
    vbox.pack_start(&hpaned, true, true, 0);

    let theme_mode = settings.theme_mode.clone();

    let profile_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("ferro")
        .join("profile");
    let _ = std::fs::create_dir_all(&profile_dir);
    clean_stale_profile_locks(&profile_dir);
    let web_context = wry::WebContext::new(Some(profile_dir));

    let css_provider = gtk::CssProvider::new();
    update_gtk_menu_css(&css_provider, &settings);
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
    let initial_settings_json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".to_string());
    let initial_surface_rgba = compute_surface_rgba(&settings);

    let state = Rc::new(RefCell::new(BrowserState {
        window: window.clone(),
        top_box: top_box.clone(),
        hpaned,
        stack,
        side_box: side_box.clone(),
        side_header_webview: None,
        side_webview: None,
        side_panel_open: false,
        lens_preview,
        last_lens_result_url: None,
        spare_tab: None,
        css_provider,
        top_webview: None,
        tabs: Vec::new(),
        closed_tabs: Vec::new(),
        active_tab_id: None,
        next_tab_id: 1,
        bookmarks,
        history,
        passwords,
        downloads,
        settings,
        web_context,
        adblocker,
        native_filter: None,
        dark_mode: false,
        theme: theme_mode,
        download_handler_attached: false,
        pending_password: None,
        settings_dirty: settings_dirty.clone(),
        bookmarks_dirty: bookmarks_dirty.clone(),
        history_dirty: history_dirty.clone(),
        passwords_dirty: passwords_dirty.clone(),
        page_source_cache,
    }));

    // Background watcher for settings/bookmarks/history/passwords saved via ferro://api/* custom protocol
    let state_for_watcher = state.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(180), move || {
        if settings_dirty.swap(false, Ordering::SeqCst) {
            if let Ok(mut st) = state_for_watcher.try_borrow_mut() {
                let loaded = BrowserSettings::load_or_default();
                st.theme = loaded.theme_mode.clone();
                st.settings = loaded;
            }
            sync_settings_to_ui(&state_for_watcher);
        }
        if bookmarks_dirty.swap(false, Ordering::SeqCst) {
            if let Ok(mut st) = state_for_watcher.try_borrow_mut() {
                st.bookmarks = BookmarksData::load_or_default();
            }
            sync_bookmarks_to_ui(&state_for_watcher);
        }
        if history_dirty.swap(false, Ordering::SeqCst) {
            if let Ok(mut st) = state_for_watcher.try_borrow_mut() {
                st.history = HistoryData::load_or_default();
            }
        }
        if passwords_dirty.swap(false, Ordering::SeqCst) {
            if let Ok(mut st) = state_for_watcher.try_borrow_mut() {
                st.passwords = PasswordsData::load_or_default();
            }
        }
        glib::ControlFlow::Continue
    });

    // Fast-path cached WebKit filter load; background compilation ONLY when cache is missing/outdated!
    let store_dir = AdBlocker::filter_store_dir();
    let store_dir_str = store_dir.to_string_lossy().to_string();
    let stamp_path = store_dir.join("rules.stamp");
    const FILTER_STAMP: &str = "ferro-webkit-rules-v2";
    let stamp_ok = std::fs::read_to_string(&stamp_path)
        .map(|s| s.trim() == FILTER_STAMP)
        .unwrap_or(false);

    let state_for_load = state.clone();
    let store_dir_for_load = store_dir_str.clone();
    let stamp_path_clone = stamp_path.clone();
    load_filter(&store_dir_str, "ferro_adblock", move |filter_opt| {
        if stamp_ok {
            if let Some(filter) = filter_opt {
                println!("[Ferro AdBlock] Cached native WebKit filter attached immediately!");
                glib::idle_add_local_once(move || {
                    if let Ok(mut st) = state_for_load.try_borrow_mut() {
                        if st.adblocker.is_enabled() {
                            for tab in &st.tabs {
                                if let Some(ucm) = tab.inner.user_content_manager() {
                                    attach_to_ucm(&ucm, &filter);
                                }
                            }
                        }
                        st.native_filter = Some(filter);
                    }
                });
                return;
            }
        }

        // Compile rules JSON on a background worker thread so GTK UI startup is never blocked
        let state_weak = Rc::downgrade(&state_for_load);
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
                    let state_weak_inner = state_weak.clone();
                    let stamp_done = stamp_bg.clone();
                    save_filter(&store_dir_bg, "ferro_adblock", &rules_json, move |filter| {
                        let _ = std::fs::write(&stamp_done, FILTER_STAMP);
                        println!("[Ferro AdBlock] Background WebKit uBlock filter compiled and cached!");
                        if let Some(state_rc) = state_weak_inner.upgrade() {
                            glib::idle_add_local_once(move || {
                                if let Ok(mut st) = state_rc.try_borrow_mut() {
                                    if st.adblocker.is_enabled() {
                                        for tab in &st.tabs {
                                            if let Some(ucm) = tab.inner.user_content_manager() {
                                                attach_to_ucm(&ucm, &filter);
                                            }
                                        }
                                    }
                                    st.native_filter = Some(filter);
                                }
                            });
                        }
                    });
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    });

    // Setup Top UI WebView
    let state_for_ipc = state.clone();
    let top_init_js = format!(
        "window.addEventListener('DOMContentLoaded', function() {{ if (window.ferro && window.ferro.applySettings) {{ window.ferro.applySettings({}); }} }});",
        initial_settings_json
    );
    let top_builder = WebViewBuilder::new()
        .with_transparent(true)
        .with_initialization_script(&top_init_js)
        .with_html(SHELL_HTML)
        .with_ipc_handler(move |req| {
            match serde_json::from_str::<IpcMessage>(req.body()) {
                Ok(msg) => handle_ipc(&state_for_ipc, msg),
                Err(e) => eprintln!("[Ferro Shell IPC Parse Error] {:?}: {}", e, req.body()),
            }
        });

    let top_webview = match top_builder.build_gtk(&top_box) {
        Ok(wv) => Rc::new(wv),
        Err(e) => {
            panic!("Не удалось создать верхний UI Webview: {:?}", e);
        }
    };
    top_webview.webview().set_background_color(&initial_surface_rgba);
    top_webview.webview().connect_context_menu(|_wv, _menu, _event, _hit_test| true);

    state.borrow_mut().top_webview = Some(top_webview);

    // Initial tab or session restore
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
    schedule_prewarm_tab(&state);

    // Save session on window close
    let state_for_delete = state.clone();
    window.connect_delete_event(move |_win, _event| {
        save_current_session(&state_for_delete);
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
            if let Ok(state) = state_for_keys.try_borrow() {
                if state.window.is_maximized() {
                    state.window.unmaximize();
                } else {
                    state.window.maximize();
                }
            }
            return glib::Propagation::Stop;
        }

        glib::Propagation::Proceed
    });

    window.connect_destroy(|_| {
        gtk::main_quit();
    });

    window.show_all();
    side_box.hide();
    gtk::main();
}
