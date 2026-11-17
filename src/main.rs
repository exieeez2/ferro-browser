mod adblocker;
mod bookmarks;
mod history;
mod importer;
mod injector;
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
    LoadEvent, NavigationPolicyDecision, NavigationPolicyDecisionExt, PolicyDecisionExt,
    PolicyDecisionType, SettingsExt, URIRequestExt, WebInspectorExt, WebViewExt,
};
use wry::{WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

mod session;

use adblocker::AdBlocker;
use bookmarks::BookmarksData;
use history::HistoryData;
use importer::BrowserImporter;
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

struct Tab {
    id: u32,
    url: String,
    title: String,
    can_go_back: bool,
    can_go_forward: bool,
    is_loading: bool,
    webview: Rc<WebView>,
    inner: webkit2gtk::WebView,
    container: gtk::Box,
}

struct BrowserState {
    window: gtk::Window,
    stack: gtk::Stack,
    css_provider: gtk::CssProvider,
    top_webview: Option<Rc<WebView>>,
    tabs: Vec<Tab>,
    closed_tabs: Vec<(String, String)>,
    active_tab_id: Option<u32>,
    next_tab_id: u32,
    bookmarks: BookmarksData,
    history: HistoryData,
    passwords: PasswordsData,
    settings: BrowserSettings,
    web_context: wry::WebContext,
    adblocker: Arc<AdBlocker>,
    native_filter: Option<WebKitFilter>,
    dark_mode: bool,
    theme: String,
    settings_dirty: Arc<AtomicBool>,
    bookmarks_dirty: Arc<AtomicBool>,
    page_source_cache: Arc<Mutex<(String, String)>>,
}

#[derive(Deserialize, Debug)]
struct IpcMessage {
    action: String,
    url: Option<String>,
    id: Option<u32>,
    title: Option<String>,
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
                    serde_json::json!({
                        "id": t.id,
                        "url": t.url,
                        "title": t.title,
                        "canGoBack": t.can_go_back,
                        "canGoForward": t.can_go_forward,
                        "isLoading": t.is_loading,
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
                serde_json::json!({
                    "url": t.url,
                    "title": t.title,
                    "canGoBack": t.can_go_back,
                    "canGoForward": t.can_go_forward,
                    "isLoading": t.is_loading,
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

fn update_gtk_menu_css(css_provider: &gtk::CssProvider, s: &BrowserSettings) {
    let is_light = s.theme_mode == "light";
    let (bg, fg, hover_bg, border) = if is_light {
        ("#ffffff", "#18181b", "#ececf1", "#d4d4d8")
    } else {
        ("#1c1c20", "#f2f2f5", "#2d2c36", "#353440")
    };
    let rad = s.corner_radius.clamp(2, 14);
    let item_rad = rad.saturating_sub(2).max(2);
    let css = format!(
        "
        window, .window, box, stack {{
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

fn sync_settings_to_ui(state_rc: &Rc<RefCell<BrowserState>>) {
    let (settings_json, top_wv, tab_wvs) = {
        if let Ok(state) = state_rc.try_borrow() {
            update_gtk_menu_css(&state.css_provider, &state.settings);
            let tabs: Vec<Rc<WebView>> = state.tabs.iter().map(|t| t.webview.clone()).collect();
            (
                serde_json::to_string(&state.settings).unwrap_or_else(|_| "{}".to_string()),
                state.top_webview.clone(),
                tabs,
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
}

fn switch_tab(state_rc: &Rc<RefCell<BrowserState>>, tab_id: u32) {
    let tab_name = {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            if state.tabs.iter().any(|t| t.id == tab_id) {
                state.active_tab_id = Some(tab_id);
                Some((format!("tab_{}", tab_id), state.stack.clone()))
            } else {
                None
            }
        } else {
            None
        }
    };

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

fn create_tab(state_rc: &Rc<RefCell<BrowserState>>, target_url: &str) -> u32 {
    let (tab_id, adblocker, native_filter, stack, autofill_script, dynamic_theme_css, settings_json, settings_dirty, bookmarks_dirty, source_cache) = {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            let id = state.next_tab_id;
            state.next_tab_id += 1;
            let autofill = state.passwords.generate_autofill_script();
            let theme_css = state.settings.generate_css_variables();
            let s_json = serde_json::to_string(&state.settings).unwrap_or_else(|_| "{}".to_string());
            (
                id,
                state.adblocker.clone(),
                state.native_filter.clone(),
                state.stack.clone(),
                autofill,
                theme_css,
                s_json,
                state.settings_dirty.clone(),
                state.bookmarks_dirty.clone(),
                state.page_source_cache.clone(),
            )
        } else {
            return 0;
        }
    };

    let tab_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let tab_name = format!("tab_{}", tab_id);
    stack.add_named(&tab_box, &tab_name);

    let init_script = format!(
        r#"
        try {{ window._ferro_settings = {settings_json}; }} catch(e) {{}}
        (function() {{
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
        try {{ {adblock_js} }} catch(e) {{}}
        try {{ {ctx_js} }} catch(e) {{}}
        try {{ {hotkeys_js} }} catch(e) {{}}
        try {{ {autofill_js} }} catch(e) {{}}
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
        Err(_) => return 0,
    };

    let protocol_already_registered = state_mut.web_context.is_custom_protocol_registered("ferro");
    let mut builder = WebViewBuilder::new_with_web_context(&mut state_mut.web_context);
    if !protocol_already_registered {
        builder = builder.with_custom_protocol("ferro".into(), move |_id, req| {
            let uri = req.uri().to_string();
            let path_and_query = uri.strip_prefix("ferro://").unwrap_or(&uri);

            let (html, content_type) = if path_and_query.starts_with("tokens") {
                (TOKENS_HTML.to_string(), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("settings") {
                (SETTINGS_HTML.to_string(), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("view-source") {
                let s = BrowserSettings::load_or_default();
                let (src_url, src_html) = source_cache
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_else(|_| ("about:blank".to_string(), String::new()));
                (render_source_viewer_html(&src_url, &src_html, &s), "text/html; charset=utf-8")
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
                (serde_json::to_string(&res).unwrap_or_default(), "application/json")
            } else if path_and_query.starts_with("api/import/dismiss") {
                let mut s = BrowserSettings::load_or_default();
                s.has_prompted_import = true;
                let _ = s.save();
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/session/detect") {
                let res = if let Some(session) = SessionData::load() {
                    let count = session.tabs.len();
                    format!(r#"{{"has_session":true,"tabs_count":{}}}"#, count)
                } else {
                    r#"{"has_session":false,"tabs_count":0}"#.to_string()
                };
                (res, "application/json")
            } else if path_and_query.starts_with("api/session/dismiss") {
                SessionData::clear();
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
                    }
                }
                (r#"<script>window.location.href="ferro://history";</script>"#.to_string(), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("api/history/clear") {
                let mut h = HistoryData::load_or_default();
                h.clear();
                (r#"<script>window.location.href="ferro://history";</script>"#.to_string(), "text/html; charset=utf-8")
            } else if path_and_query.starts_with("api/passwords/delete") {
                if let Some(id_str) = path_and_query.split("id=").nth(1) {
                    if let Ok(id) = id_str.split('&').next().unwrap_or("").parse::<u64>() {
                        let mut p = PasswordsData::load_or_default();
                        p.delete(id);
                    }
                }
                (r#"<script>window.location.href="ferro://passwords";</script>"#.to_string(), "text/html; charset=utf-8")
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
                }
                (r#"{"status":"saved"}"#.to_string(), "application/json")
            } else if path_and_query.starts_with("api/passwords/import") {
                let body_bytes = req.body();
                if let Ok(csv_text) = std::str::from_utf8(body_bytes) {
                    let mut p = PasswordsData::load_or_default();
                    p.import_chrome_csv(csv_text);
                }
                (r#"{"status":"ok"}"#.to_string(), "application/json")
            } else {
                (NEWTAB_HTML.to_string(), "text/html; charset=utf-8")
            };

            wry::http::Response::builder()
                .header("content-type", content_type)
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
            return 0;
        }
    };
    drop(state_mut);

    let inner_wv = webview.webview();

    // Set Chrome User-Agent for Google accounts login, enable WebInspector & modern web APIs
    if let Some(settings) = WebViewExt::settings(&inner_wv) {
        settings.set_user_agent(Some(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        ));
        settings.set_enable_developer_extras(true);
        settings.set_enable_webgl(true);
        settings.set_enable_smooth_scrolling(true);
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
                    if let Ok(mut state) = state_rc.try_borrow_mut() {
                        state.history.add(&uri, &title);
                    }
                }
            }
        }
    });

    let state_weak = Rc::downgrade(state_rc);
    inner_wv.connect_uri_notify(move |wv| {
        if let Some(state_rc) = state_weak.upgrade() {
            let new_uri = wv.uri().map(|u| u.to_string()).unwrap_or_default();
            {
                if let Ok(mut state) = state_rc.try_borrow_mut() {
                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_copy) {
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

    {
        if let Ok(mut state) = state_rc.try_borrow_mut() {
            state.tabs.push(Tab {
                id: tab_id,
                url: target_url.to_string(),
                title: "Новая вкладка".to_string(),
                can_go_back: false,
                can_go_forward: false,
                is_loading: false,
                webview,
                inner: inner_wv,
                container: tab_box,
            });
            state.active_tab_id = Some(tab_id);
            state.stack.set_visible_child_name(&tab_name);
        }
    }

    sync_tabs_to_ui(state_rc);
    tab_id
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
                for tab in session.tabs {
                    create_tab(state_rc, &tab.url);
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
                let _ = wry_wv.load_url("ferro://newtab");
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
                if let Ok(state) = state_rc.try_borrow() {
                    let enabled = state.adblocker.toggle_enabled();
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
            sync_adblock_to_ui(state_rc);
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
                    let active_wv = state.active_tab_id.and_then(|id| {
                        state.tabs.iter().find(|t| t.id == id).map(|t| t.webview.clone())
                    });
                    state.window.window().map(|gw| (gw, w, h, active_wv))
                } else {
                    None
                }
            };
            if let Some((gdk_win, w, h, active_wv)) = info {
                let cap_h = (h - 102).max(1);
                if let Some(pixbuf) = gdk_win.pixbuf(0, 102, w, cap_h) {
                    let dir = dirs::picture_dir()
                        .or_else(dirs::download_dir)
                        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
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

                        menu.append(&gtk::SeparatorMenuItem::new());

                        let item_close_others = gtk::MenuItem::with_label("Закрыть другие вкладки");
                        let s_rc = state_rc.clone();
                        item_close_others.connect_activate(move |_| {
                            let other_ids: Vec<u32> = s_rc
                                .try_borrow()
                                .map(|st| st.tabs.iter().filter(|t| t.id != tab_id).map(|t| t.id).collect())
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

                    let item_history = gtk::MenuItem::with_label("История просмотров         Ctrl+H");
                    let s_rc = state_rc.clone();
                    item_history.connect_activate(move |_| {
                        create_tab(&s_rc, "ferro://history");
                    });
                    menu.append(&item_history);

                    let item_passwords = gtk::MenuItem::with_label("Пароли и Google");
                    let s_rc = state_rc.clone();
                    item_passwords.connect_activate(move |_| {
                        create_tab(&s_rc, "ferro://passwords");
                    });
                    menu.append(&item_passwords);

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
                        handle_ipc(&s_rc, IpcMessage { action: "toggle_adblock".to_string(), url: None, id: None, title: None, settings: None });
                    });
                    menu.append(&item_adblock);

                    let item_theme = gtk::MenuItem::with_label(&th_label);
                    let s_rc = state_rc.clone();
                    item_theme.connect_activate(move |_| {
                        handle_ipc(&s_rc, IpcMessage { action: "toggle_theme".to_string(), url: None, id: None, title: None, settings: None });
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
                        let menu_x = (win_w - 240).max(10);
                        let rect = gtk::gdk::Rectangle::new(menu_x, 74, 230, 1);
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

fn ensure_single_instance() -> Option<std::os::unix::net::UnixListener> {
    use std::io::Write;
    use std::os::unix::net::{UnixListener, UnixStream};

    let sock_path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("ferro")
        .join("instance.sock");

    if let Some(parent) = sock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match UnixListener::bind(&sock_path) {
        Ok(listener) => {
            let _ = listener.set_nonblocking(true);
            Some(listener)
        }
        Err(_) => {
            if let Ok(mut stream) = UnixStream::connect(&sock_path) {
                println!("[Ferro] Already running. Focusing existing window.");
                let _ = stream.write_all(b"FOCUS\n");
                None
            } else {
                let _ = std::fs::remove_file(&sock_path);
                if let Ok(listener) = UnixListener::bind(&sock_path) {
                    let _ = listener.set_nonblocking(true);
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
                    if msg.contains("FOCUS") {
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
    top_box.set_size_request(-1, 102); // 36px tabs + 38px toolbar + 28px bookmarks = 102px
    vbox.pack_start(&top_box, false, false, 0);

    let stack = gtk::Stack::new();
    vbox.pack_start(&stack, true, true, 0);

    let bookmarks = BookmarksData::load_or_default();
    let history = HistoryData::load_or_default();
    let passwords = PasswordsData::load_or_default();
    let adblocker = AdBlocker::new();
    let settings = BrowserSettings::load_or_default();
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
    let page_source_cache = Arc::new(Mutex::new((String::new(), String::new())));
    let initial_settings_json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".to_string());

    let state = Rc::new(RefCell::new(BrowserState {
        window: window.clone(),
        stack,
        css_provider,
        top_webview: None,
        tabs: Vec::new(),
        closed_tabs: Vec::new(),
        active_tab_id: None,
        next_tab_id: 1,
        bookmarks,
        history,
        passwords,
        settings,
        web_context,
        adblocker,
        native_filter: None,
        dark_mode: false,
        theme: theme_mode,
        settings_dirty: settings_dirty.clone(),
        bookmarks_dirty: bookmarks_dirty.clone(),
        page_source_cache,
    }));

    // Background watcher for settings/bookmarks saved via ferro://api/* custom protocol
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
        glib::ControlFlow::Continue
    });

    // Compile native WebKit content blocking rules (uBlock / EasyList format)
    let state_for_filter = state.clone();
    let store_dir = AdBlocker::filter_store_dir();
    let store_dir_str = store_dir.to_string_lossy().to_string();
    let rules_json = AdBlocker::compile_webkit_rules_json();

    // Fast load pre-existing filter if available
    let state_for_load = state.clone();
    let store_dir_str_clone = store_dir_str.clone();
    load_filter(&store_dir_str_clone, "ferro_adblock", move |filter_opt| {
        if let Some(filter) = filter_opt {
            println!("[Ferro AdBlock] Pre-compiled native WebKit filter loaded!");
            glib::idle_add_local_once(move || {
                if let Ok(mut state) = state_for_load.try_borrow_mut() {
                    if state.adblocker.is_enabled() {
                        for tab in &state.tabs {
                            if let Some(ucm) = tab.inner.user_content_manager() {
                                attach_to_ucm(&ucm, &filter);
                            }
                        }
                    }
                    state.native_filter = Some(filter);
                }
            });
        }
    });

    save_filter(&store_dir_str, "ferro_adblock", &rules_json, move |filter| {
        println!("[Ferro AdBlock] Native WebKit uBlock filter updated and active!");
        glib::idle_add_local_once(move || {
            if let Ok(mut state) = state_for_filter.try_borrow_mut() {
                if state.adblocker.is_enabled() {
                    for tab in &state.tabs {
                        if let Some(ucm) = tab.inner.user_content_manager() {
                            attach_to_ucm(&ucm, &filter);
                        }
                    }
                }
                state.native_filter = Some(filter);
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
            } else if key == gdk::keys::constants::d || key == gdk::keys::constants::D || key == gdk::keys::constants::Cyrillic_ve || key == gdk::keys::constants::Cyrillic_VE {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: "toggle_bookmark_active".to_string(),
                    url: None,
                    id: None,
                    title: None,
                    settings: None,
                });
                return glib::Propagation::Stop;
            } else if key == gdk::keys::constants::Tab || key == gdk::keys::constants::ISO_Left_Tab {
                handle_ipc(&state_for_keys, IpcMessage {
                    action: if shift { "prev_tab".to_string() } else { "next_tab".to_string() },
                    url: None,
                    id: None,
                    title: None,
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
                        settings: None,
                    });
                    return glib::Propagation::Stop;
                }
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
    gtk::main();
}
