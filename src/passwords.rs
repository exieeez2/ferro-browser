use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_PWD_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPassword {
    pub id: u64,
    pub origin: String,
    pub username: String,
    pub password: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PasswordsData {
    pub items: Vec<SavedPassword>,
}

fn parse_csv_row(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            fields.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

impl PasswordsData {
    pub fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("ferro");
        let _ = fs::create_dir_all(&path);
        path.push("passwords.json");
        path
    }

    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(data) = serde_json::from_str::<PasswordsData>(&content) {
                    return data;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn is_already_saved(&self, origin: &str, username: &str, password: &str) -> bool {
        let o = origin.trim().to_lowercase();
        let u = username.trim();
        self.items
            .iter()
            .any(|p| p.origin.to_lowercase() == o && p.username == u && p.password == password)
    }

    pub fn add(&mut self, origin: &str, username: &str, password: &str) {
        let mut clean_origin = origin.trim().to_string();
        let clean_user = username.trim();
        let clean_pass = password;

        if clean_origin.is_empty() || clean_pass.is_empty() {
            return;
        }
        if !clean_origin.starts_with("http://") && !clean_origin.starts_with("https://") {
            clean_origin = format!("https://{}", clean_origin);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Some(existing) = self.items.iter_mut().find(|p| {
            p.origin.eq_ignore_ascii_case(&clean_origin) && p.username.eq_ignore_ascii_case(clean_user)
        }) {
            existing.username = clean_user.to_string();
            existing.password = clean_pass.to_string();
            existing.created_at = now;
        } else {
            let seq = NEXT_PWD_SEQ.fetch_add(1, Ordering::Relaxed);
            let id = (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64)
                .wrapping_add(seq);

            self.items.insert(
                0,
                SavedPassword {
                    id,
                    origin: clean_origin,
                    username: clean_user.to_string(),
                    password: clean_pass.to_string(),
                    created_at: now,
                },
            );
        }

        self.save();
    }

    pub fn update_by_id(&mut self, id: u64, origin: &str, username: &str, password: &str) {
        if let Some(item) = self.items.iter_mut().find(|p| p.id == id) {
            if !origin.trim().is_empty() {
                item.origin = origin.trim().to_string();
            }
            item.username = username.trim().to_string();
            if !password.is_empty() {
                item.password = password.to_string();
            }
            self.save();
        }
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|p| p.id != id);
        self.save();
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.save();
    }

    pub fn import_chrome_csv(&mut self, csv_text: &str) -> usize {
        let mut count = 0;
        let mut lines = csv_text.lines().filter(|l| !l.trim().is_empty());
        let header_line = match lines.next() {
            Some(h) => h,
            None => return 0,
        };

        let headers: Vec<String> = parse_csv_row(header_line)
            .into_iter()
            .map(|h| h.to_lowercase())
            .collect();

        // Detect column indices dynamically (Chrome, Firefox, Bitwarden, Edge)
        let url_idx = headers
            .iter()
            .position(|h| h == "url" || h == "login_uri" || h == "origin" || h == "website")
            .unwrap_or(if headers.len() >= 4 { 1 } else { 0 });
        let user_idx = headers
            .iter()
            .position(|h| h == "username" || h == "login_username" || h == "user" || h == "login")
            .unwrap_or(if headers.len() >= 4 { 2 } else { 1 });
        let pass_idx = headers
            .iter()
            .position(|h| h == "password" || h == "login_password" || h == "pass")
            .unwrap_or(if headers.len() >= 4 { 3 } else { 2 });

        for line in lines {
            let cols = parse_csv_row(line);
            if let (Some(url), Some(pass)) = (cols.get(url_idx), cols.get(pass_idx)) {
                let user = cols.get(user_idx).map(|s| s.as_str()).unwrap_or("");
                if !url.is_empty() && !pass.is_empty() {
                    self.add(url, user, pass);
                    count += 1;
                }
            }
        }
        count
    }

    pub fn export_csv(&self) -> String {
        let mut out = String::from("name,url,username,password\n");
        for item in &self.items {
            let esc = |s: &str| -> String {
                if s.contains(',') || s.contains('"') || s.contains('\n') {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s.to_string()
                }
            };
            out.push_str(&format!(
                "{},{},{},{}\n",
                esc(&item.origin),
                esc(&item.origin),
                esc(&item.username),
                esc(&item.password)
            ));
        }
        out
    }

    /// Generates JS to inject a persistent Shadow-DOM Save/Update Password prompt on the active page
    pub fn build_doorhanger_js(origin: &str, username: &str, password: &str, is_update: bool) -> String {
        let origin_json = serde_json::to_string(origin).unwrap_or_else(|_| "\"\"".to_string());
        let user_json = serde_json::to_string(username).unwrap_or_else(|_| "\"\"".to_string());
        let pass_json = serde_json::to_string(password).unwrap_or_else(|_| "\"\"".to_string());
        let title_label = if is_update {
            "Обновить сохранённый пароль?"
        } else {
            "Сохранить пароль для сайта?"
        };

        format!(
            r#"
            (function() {{
                const cfg = window._ferro_settings || {{}};
                if (cfg.offer_save_passwords === false) return;
                const HOST_ID = 'ferro-pwd-doorhanger-host';
                if (document.getElementById(HOST_ID)) document.getElementById(HOST_ID).remove();

                const originVal = {origin_json};
                const userVal = {user_json};
                const passVal = {pass_json};
                let hostLabel = originVal;
                try {{ hostLabel = new URL(originVal).hostname || originVal; }} catch(e) {{}}

                const isLight = cfg.theme_mode === 'light';
                const rad = cfg.corner_radius !== undefined ? Math.max(4, Math.min(14, cfg.corner_radius)) : 6;
                const bg = isLight ? '#ffffff' : '#19191c';
                const fg = isLight ? '#18181b' : '#f2f2f5';
                const muted = isLight ? '#71717a' : '#9ca3af';
                const inpBg = isLight ? '#f4f4f6' : '#111113';
                const border = isLight ? '#d4d4d8' : '#34343e';
                const effAccent = isLight ? '#18181b' : '#ececec';
                const accFg = isLight ? '#ffffff' : '#111113';

                const host = document.createElement('div');
                host.id = HOST_ID;
                host.style.cssText = 'position:fixed;top:14px;right:16px;z-index:2147483647;';
                const shadow = host.attachShadow({{ mode: 'open' }});

                shadow.innerHTML = `
                    <style>
                        * {{ box-sizing: border-box; margin: 0; padding: 0; font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif; }}
                        @keyframes slideIn {{ from {{ opacity: 0; transform: translateY(-8px); }} to {{ opacity: 1; transform: translateY(0); }} }}
                        .card {{
                            width: 332px;
                            background: ${{bg}};
                            color: ${{fg}};
                            border: 1px solid ${{border}};
                            border-radius: ${{rad}}px;
                            padding: 15px 16px;
                            box-shadow: 0 12px 32px rgba(0, 0, 0, ${{isLight ? '0.18' : '0.65'}});
                            animation: slideIn 0.16s cubic-bezier(0.2, 0, 0, 1);
                        }}
                        .head {{ display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; gap: 8px; }}
                        .head-title {{ display: flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 600; }}
                        .sub {{ font-size: 11.5px; color: ${{muted}}; margin-bottom: 12px; }}
                        .field {{ display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }}
                        .lbl {{ width: 56px; font-size: 11.5px; color: ${{muted}}; flex-shrink: 0; }}
                        .inp-wrap {{ flex: 1; display: flex; align-items: center; background: ${{inpBg}}; border: 1px solid ${{border}}; border-radius: ${{Math.max(2, rad - 2)}}px; padding: 0 6px; }}
                        input {{ flex: 1; min-width: 0; height: 28px; border: none; background: transparent; color: ${{fg}}; font-size: 12px; font-family: ui-monospace, monospace; outline: none; }}
                        .eye {{ border: none; background: transparent; color: ${{muted}}; cursor: pointer; font-size: 11px; padding: 2px 4px; }}
                        .actions {{ display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }}
                        .btn {{ height: 30px; padding: 0 13px; border-radius: ${{Math.max(2, rad - 2)}}px; font-size: 12px; font-weight: 500; cursor: pointer; border: 1px solid ${{border}}; background: transparent; color: ${{fg}}; }}
                        .btn-save {{ background: ${{effAccent}}; color: ${{accFg}}; border-color: ${{effAccent}}; font-weight: 600; }}
                    </style>
                    <div class="card">
                        <div class="head">
                            <div class="head-title">
                                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
                                <span>{title_label}</span>
                            </div>
                        </div>
                        <div class="sub">Сайт: <b>${{hostLabel}}</b></div>
                        <div class="field">
                            <span class="lbl">Логин</span>
                            <div class="inp-wrap"><input id="uInp" type="text" /></div>
                        </div>
                        <div class="field">
                            <span class="lbl">Пароль</span>
                            <div class="inp-wrap">
                                <input id="pInp" type="password" />
                                <button type="button" class="eye" id="eyeBtn" title="Показать пароль">Показать</button>
                            </div>
                        </div>
                        <div class="actions">
                            <button type="button" class="btn" id="noBtn">Не сейчас</button>
                            <button type="button" class="btn btn-save" id="yesBtn">Сохранить</button>
                        </div>
                    </div>
                `;

                const uInp = shadow.getElementById('uInp');
                const pInp = shadow.getElementById('pInp');
                uInp.value = userVal;
                pInp.value = passVal;

                shadow.getElementById('eyeBtn').addEventListener('click', (ev) => {{
                    const hidden = pInp.type === 'password';
                    pInp.type = hidden ? 'text' : 'password';
                    ev.target.textContent = hidden ? 'Скрыть' : 'Показать';
                }});

                shadow.getElementById('noBtn').addEventListener('click', () => {{
                    if (window.ipc) {{
                        window.ipc.postMessage(JSON.stringify({{ action: 'dismiss_pending_password' }}));
                    }}
                    host.remove();
                }});

                shadow.getElementById('yesBtn').addEventListener('click', () => {{
                    const finalUser = uInp.value.trim();
                    const finalPass = pInp.value;
                    if (window.ipc && finalPass) {{
                        window.ipc.postMessage(JSON.stringify({{
                            action: 'save_password',
                            url: originVal,
                            title: finalUser,
                            pass: finalPass
                        }}));
                    }}
                    host.remove();
                }});

                (document.body || document.documentElement).appendChild(host);
            }})();
            "#
        )
    }

    pub fn generate_autofill_script(&self) -> String {
        let json = serde_json::to_string(&self.items).unwrap_or_else(|_| "[]".to_string());
        format!(
            r#"
            (function() {{
                if (window._ferro_pwd_installed) return;
                window._ferro_pwd_installed = true;
                try {{
                    if (window.location.protocol === 'ferro:') return;
                    const allPasswords = {json};
                    const curOrigin = (window.location.origin || '').toLowerCase();
                    const curHost = (window.location.hostname || '').toLowerCase().replace(/^www\./, '');

                    function setInputNativeValue(el, val) {{
                        if (!el) return;
                        const proto = Object.getPrototypeOf(el);
                        const desc = Object.getOwnPropertyDescriptor(proto, 'value') ||
                                     Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
                        if (desc && desc.set) {{
                            desc.set.call(el, val);
                        }} else {{
                            el.value = val;
                        }}
                        el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                        el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    }}

                    function generateStrongPassword(len = 18) {{
                        const chars = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%&*-_';
                        const arr = new Uint32Array(len);
                        crypto.getRandomValues(arr);
                        let out = '';
                        for (let i = 0; i < len; i++) out += chars[arr[i] % chars.length];
                        return out;
                    }}

                    const siteMatches = (allPasswords || []).filter(p => {{
                        if (!curHost || !p.origin) return false;
                        const pOrig = p.origin.toLowerCase();
                        try {{
                            const pHost = new URL(pOrig).hostname.replace(/^www\./, '');
                            return pHost === curHost || curHost.endsWith('.' + pHost) || pHost.endsWith('.' + curHost);
                        }} catch (e) {{
                            return pOrig.includes(curHost);
                        }}
                    }});

                    function findFormUserInput(passInput) {{
                        const scope = passInput.closest('form') || passInput.parentElement?.parentElement?.parentElement || document;
                        const candidates = Array.from(scope.querySelectorAll(
                            'input[type="email"], input[type="text"], input[type="tel"], input:not([type])'
                        )).filter(inp => inp.offsetParent !== null && !inp.readOnly && !inp.disabled);
                        return candidates[candidates.length - 1] || candidates[0] || null;
                    }}

                    // Auto-fill primary account on initial load
                    if (siteMatches.length > 0) {{
                        const primary = siteMatches[0];
                        let autoFilledOnce = false;
                        function runInitialAutofill() {{
                            if (autoFilledOnce) return;
                            const passInputs = Array.from(document.querySelectorAll('input[type="password"]'))
                                .filter(el => el.offsetParent !== null);
                            if (passInputs.length === 0) return;
                            passInputs.forEach(pEl => {{
                                if (!pEl.value) {{
                                    setInputNativeValue(pEl, primary.password);
                                    const uEl = findFormUserInput(pEl);
                                    if (uEl && !uEl.value && primary.username) {{
                                        setInputNativeValue(uEl, primary.username);
                                    }}
                                    autoFilledOnce = true;
                                }}
                            }});
                        }}
                        if (document.readyState === 'loading') {{
                            document.addEventListener('DOMContentLoaded', runInitialAutofill, {{ once: true }});
                        }} else {{
                            runInitialAutofill();
                        }}
                        setTimeout(runInitialAutofill, 650);
                    }}

                    // Inline Shadow-DOM Autofill & Password Generator Popup on Input Focus
                    const POPUP_ID = 'ferro-autofill-popup-host';
                    function closePopup() {{
                        const old = document.getElementById(POPUP_ID);
                        if (old) old.remove();
                    }}

                    document.addEventListener('focusin', function(e) {{
                        const inp = e.target;
                        if (!inp || inp.tagName !== 'INPUT') return;
                        const t = (inp.type || 'text').toLowerCase();
                        const ac = (inp.autocomplete || '').toLowerCase();
                        const name = ((inp.name || '') + ' ' + (inp.id || '')).toLowerCase();

                        const isPass = t === 'password';
                        const isUser = !isPass && (t === 'email' || ac.includes('username') || /user|login|email|mail/.test(name)) &&
                                       document.querySelector('input[type="password"]');

                        if (!isPass && !isUser) return;
                        if (!isPass && siteMatches.length === 0) return;

                        closePopup();
                        const rect = inp.getBoundingClientRect();
                        if (rect.width < 40 || rect.height < 10) return;

                        const host = document.createElement('div');
                        host.id = POPUP_ID;
                        host.style.cssText = `position:fixed;left:${{Math.max(6, rect.left)}}px;top:${{rect.bottom + 4}}px;z-index:2147483646;`;
                        const shadow = host.attachShadow({{ mode: 'open' }});

                        const cfg = window._ferro_settings || {{}};
                        const isLight = cfg.theme_mode === 'light';
                        const bg = isLight ? '#ffffff' : '#1b1b1f';
                        const fg = isLight ? '#18181b' : '#f2f2f5';
                        const muted = isLight ? '#71717a' : '#9ca3af';
                        const hover = isLight ? '#ededf2' : '#2a2a32';
                        const border = isLight ? '#d4d4d8' : '#353540';

                        let rowsHtml = '';
                        siteMatches.forEach((m, idx) => {{
                            const uDisp = (m.username || '(без логина)').replace(/</g, '&lt;');
                            rowsHtml += `<button type="button" class="row" data-idx="${{idx}}">
                                <span class="u">${{uDisp}}</span>
                                <span class="dots">••••••••</span>
                            </button>`;
                        }});

                        if (isPass) {{
                            rowsHtml += `<button type="button" class="row gen" id="genBtn">
                                <span class="u">Сгенерировать надёжный пароль</span>
                                <span class="dots">18 симв.</span>
                            </button>`;
                        }}

                        if (!rowsHtml) return;

                        shadow.innerHTML = `
                            <style>
                                * {{ box-sizing: border-box; margin: 0; padding: 0; font-family: 'Inter', -apple-system, sans-serif; }}
                                .box {{
                                    min-width: ${{Math.max(230, Math.min(320, rect.width))}}px;
                                    background: ${{bg}};
                                    border: 1px solid ${{border}};
                                    border-radius: 6px;
                                    padding: 4px;
                                    box-shadow: 0 8px 24px rgba(0,0,0,0.45);
                                }}
                                .row {{
                                    width: 100%;
                                    display: flex;
                                    align-items: center;
                                    justify-content: space-between;
                                    gap: 10px;
                                    padding: 7px 10px;
                                    border: none;
                                    border-radius: 4px;
                                    background: transparent;
                                    color: ${{fg}};
                                    font-size: 12px;
                                    cursor: pointer;
                                    text-align: left;
                                }}
                                .row:hover {{ background: ${{hover}}; }}
                                .u {{ font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
                                .dots {{ color: ${{muted}}; font-family: ui-monospace, monospace; font-size: 11px; flex-shrink: 0; }}
                                .gen .u {{ color: #3b82f6; }}
                            </style>
                            <div class="box">${{rowsHtml}}</div>
                        `;

                        shadow.querySelectorAll('[data-idx]').forEach(btn => {{
                            btn.addEventListener('mousedown', (ev) => {{
                                ev.preventDefault();
                                const m = siteMatches[parseInt(btn.dataset.idx)];
                                if (!m) return;
                                if (isPass) {{
                                    setInputNativeValue(inp, m.password);
                                    const uEl = findFormUserInput(inp);
                                    if (uEl && m.username) setInputNativeValue(uEl, m.username);
                                }} else {{
                                    if (m.username) setInputNativeValue(inp, m.username);
                                    const scope = inp.closest('form') || document;
                                    const pEl = scope.querySelector('input[type="password"]');
                                    if (pEl) setInputNativeValue(pEl, m.password);
                                }}
                                closePopup();
                            }});
                        }});

                        const genBtn = shadow.getElementById('genBtn');
                        if (genBtn) {{
                            genBtn.addEventListener('mousedown', (ev) => {{
                                ev.preventDefault();
                                const pwd = generateStrongPassword(18);
                                setInputNativeValue(inp, pwd);
                                try {{ navigator.clipboard.writeText(pwd); }} catch(e) {{}}
                                const uEl = findFormUserInput(inp);
                                const uVal = uEl ? uEl.value.trim() : '';
                                if (window.ipc) {{
                                    window.ipc.postMessage(JSON.stringify({{
                                        action: 'stage_password',
                                        url: window.location.origin,
                                        title: uVal,
                                        pass: pwd
                                    }}));
                                }}
                                closePopup();
                            }});
                        }}

                        (document.body || document.documentElement).appendChild(host);
                    }}, true);

                    document.addEventListener('mousedown', (e) => {{
                        const host = document.getElementById(POPUP_ID);
                        if (host && e.target !== host && !host.contains(e.target)) closePopup();
                    }}, true);
                    window.addEventListener('scroll', closePopup, true);

                    // Stage credentials immediately BEFORE navigation unloads the page!
                    function extractAndStageCredentials(triggerEl) {{
                        const cfg = window._ferro_settings || {{}};
                        if (cfg.offer_save_passwords === false) return;
                        const scope = (triggerEl && triggerEl.closest ? triggerEl.closest('form') : null) || document;
                        const passInputs = Array.from(scope.querySelectorAll('input[type="password"]')).filter(p => p.value);
                        if (passInputs.length === 0) return;
                        const passEl = passInputs[passInputs.length - 1];
                        const password = passEl.value;
                        if (!password) return;

                        const uEl = findFormUserInput(passEl);
                        const username = uEl ? uEl.value.trim() : '';

                        if (window.ipc) {{
                            window.ipc.postMessage(JSON.stringify({{
                                action: 'stage_password',
                                url: window.location.origin,
                                title: username,
                                pass: password
                            }}));
                        }}
                    }}

                    document.addEventListener('submit', (e) => extractAndStageCredentials(e.target), true);
                    document.addEventListener('keydown', (e) => {{
                        if (e.key === 'Enter' && e.target && e.target.tagName === 'INPUT') {{
                            extractAndStageCredentials(e.target);
                        }}
                    }}, true);
                    document.addEventListener('click', (e) => {{
                        const btn = e.target && e.target.closest ? e.target.closest('button, input[type="submit"], [role="button"]') : null;
                        if (btn) extractAndStageCredentials(btn);
                    }}, true);
                }} catch(e) {{}}
            }})();
            "#
        )
    }

    pub fn render_html(&self) -> String {
        let items_json = serde_json::to_string(&self.items).unwrap_or_else(|_| "[]".to_string());

        format!(
            r#"<!DOCTYPE html>
<html lang="ru" data-theme="dark">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Пароли — Ferro</title>
  <style>
    :root {{
      --bg-base: #141414;
      --bg-panel: #191919;
      --bg-elevated: #202020;
      --bg-hover: #252525;
      --bg-input: #111111;
      --border-hairline: #262626;
      --border-strong: #383838;
      --text-primary: #ececec;
      --text-secondary: #9a9a9a;
      --text-muted: #666666;
      --accent: #ececec;
      --accent-fg: #111111;
      --danger: #dc4c4c;
      --radius-ctrl: 4px;
      --font-ui: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      --font-mono: 'JetBrains Mono', ui-monospace, monospace;
    }}

    [data-theme="light"], :root[data-theme="light"] {{
      --bg-base: #f5f5f7;
      --bg-panel: #ffffff;
      --bg-elevated: #f0f0f3;
      --bg-hover: #e4e4e9;
      --bg-input: #ffffff;
      --border-hairline: #dcdce0;
      --border-strong: #b8b8c0;
      --text-primary: #18181b;
      --text-secondary: #52525b;
      --text-muted: #71717a;
      --accent: #18181b;
      --accent-fg: #ffffff;
    }}

    * {{ box-sizing: border-box; margin: 0; padding: 0; }}

    body {{
      min-height: 100vh;
      background: var(--bg-base);
      color: var(--text-primary);
      font-family: var(--font-ui);
      font-size: 13px;
      display: flex;
      justify-content: center;
      padding: 32px 20px 64px;
      -webkit-font-smoothing: antialiased;
    }}

    .container {{
      width: 100%;
      max-width: 880px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }}

    .header-bar {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding-bottom: 16px;
      border-bottom: 1px solid var(--border-hairline);
      flex-wrap: wrap;
    }}

    .title-area {{
      display: flex;
      align-items: center;
      gap: 10px;
    }}

    h1 {{ font-size: 18px; font-weight: 600; }}

    .toolbar-actions {{
      display: flex;
      align-items: center;
      gap: 8px;
      flex-wrap: wrap;
    }}

    .btn {{
      height: 32px;
      padding: 0 12px;
      background: var(--bg-elevated);
      border: 1px solid var(--border-strong);
      border-radius: var(--radius-ctrl);
      color: var(--text-primary);
      font-size: 12px;
      font-weight: 500;
      font-family: inherit;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 6px;
      white-space: nowrap;
      text-decoration: none;
      transition: background 0.1s ease, border-color 0.1s ease;
    }}

    .btn:hover {{ background: var(--bg-hover); }}

    .btn-primary {{
      background: var(--accent);
      color: var(--accent-fg);
      border-color: var(--accent);
      font-weight: 600;
    }}

    .btn-danger {{
      color: var(--danger);
      border-color: rgba(220, 76, 76, 0.4);
      background: transparent;
    }}

    .btn-danger:hover {{
      background: rgba(220, 76, 76, 0.12);
      border-color: var(--danger);
    }}

    .panel {{
      background: var(--bg-panel);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      padding: 16px;
    }}

    .add-panel {{
      display: none;
      flex-direction: column;
      gap: 12px;
    }}

    .add-panel.open {{
      display: flex;
    }}

    .form-grid {{
      display: grid;
      grid-template-columns: 1.2fr 1fr 1.2fr auto;
      gap: 8px;
      align-items: center;
    }}

    .inp {{
      height: 32px;
      padding: 0 10px;
      background: var(--bg-input);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      color: var(--text-primary);
      font-size: 12.5px;
      font-family: inherit;
      outline: none;
      min-width: 0;
    }}

    .inp:focus {{ border-color: var(--accent); }}
    .mono {{ font-family: var(--font-mono); font-size: 12px; }}

    .top-controls {{
      display: flex;
      gap: 10px;
      align-items: center;
    }}

    .search-input {{
      flex: 1;
      height: 34px;
      padding: 0 12px;
      background: var(--bg-input);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      color: var(--text-primary);
      font-size: 13px;
      outline: none;
    }}

    .search-input:focus {{ border-color: var(--accent); }}

    .list-panel {{
      background: var(--bg-panel);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      overflow: hidden;
    }}

    .pwd-row {{
      display: grid;
      grid-template-columns: 28px 1.3fr 1fr 1.1fr auto;
      align-items: center;
      gap: 12px;
      padding: 11px 16px;
      border-bottom: 1px solid var(--border-hairline);
    }}

    .pwd-row:last-child {{ border-bottom: none; }}

    .site-fav {{
      width: 24px;
      height: 24px;
      border-radius: 4px;
      background: var(--bg-elevated);
      display: flex;
      align-items: center;
      justify-content: center;
      font-weight: 600;
      font-size: 11px;
      overflow: hidden;
    }}

    .site-fav img {{ width: 16px; height: 16px; object-fit: contain; }}

    .site-link {{
      color: var(--text-primary);
      font-weight: 500;
      text-decoration: none;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }}

    .site-link:hover {{ text-decoration: underline; }}

    .cell-user {{
      font-family: var(--font-mono);
      font-size: 12px;
      color: var(--text-secondary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      cursor: pointer;
    }}

    .cell-user:hover {{ color: var(--text-primary); }}

    .cell-pass {{
      display: flex;
      align-items: center;
      gap: 6px;
      min-width: 0;
    }}

    .pass-field {{
      width: 100%;
      height: 28px;
      padding: 0 8px;
      background: var(--bg-input);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      color: var(--text-primary);
      font-family: var(--font-mono);
      font-size: 12px;
      outline: none;
    }}

    .row-btns {{
      display: flex;
      align-items: center;
      gap: 6px;
    }}

    .icon-btn {{
      height: 28px;
      padding: 0 8px;
      background: transparent;
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      color: var(--text-secondary);
      font-size: 11.5px;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      justify-content: center;
    }}

    .icon-btn:hover {{
      background: var(--bg-hover);
      color: var(--text-primary);
      border-color: var(--border-strong);
    }}

    .empty-state {{
      padding: 48px 20px;
      text-align: center;
      color: var(--text-muted);
      font-size: 13px;
    }}

    .toast {{
      position: fixed;
      bottom: 20px;
      right: 24px;
      padding: 9px 15px;
      background: var(--bg-panel);
      border: 1px solid var(--border-strong);
      border-radius: var(--radius-ctrl);
      color: var(--text-primary);
      font-size: 12px;
      box-shadow: 0 8px 24px rgba(0,0,0,0.45);
      opacity: 0;
      transform: translateY(6px);
      transition: opacity 0.15s ease, transform 0.15s ease;
      pointer-events: none;
      z-index: 99;
    }}

    .toast.show {{ opacity: 1; transform: translateY(0); }}
  </style>
</head>
<body>
  <div class="container">
    <div class="header-bar">
      <div class="title-area">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="11" width="18" height="11" rx="2"></rect>
          <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
        </svg>
        <h1>Менеджер паролей (<span id="countLabel">0</span>)</h1>
      </div>

      <div class="toolbar-actions">
        <button class="btn btn-primary" id="btnToggleAdd">+ Добавить пароль</button>
        <button class="btn" id="btnGenQuick">Генератор пароля</button>
        <button class="btn" id="btnImportCsv">Импорт CSV</button>
        <button class="btn" id="btnExportCsv">Экспорт CSV</button>
        <a class="btn" href="https://accounts.google.com">Аккаунт Google</a>
        <input type="file" id="importFileInput" accept=".csv,text/csv" style="display:none;" />
      </div>
    </div>

    <!-- Add / Edit Password Panel -->
    <form class="panel add-panel" id="addForm">
      <div style="font-weight:600; font-size:13px;" id="formHeading">Новая запись</div>
      <div class="form-grid">
        <input type="text" class="inp" id="inpOrigin" placeholder="Сайт (например github.com)" required />
        <input type="text" class="inp mono" id="inpUser" placeholder="Логин или e-mail" />
        <div style="display:flex; gap:6px;">
          <input type="text" class="inp mono" id="inpPass" style="flex:1;" placeholder="Пароль" required />
          <button type="button" class="btn" id="btnGenInForm" title="Сгенерировать надёжный пароль">Сгенерировать</button>
        </div>
        <div style="display:flex; gap:6px;">
          <button type="submit" class="btn btn-primary">Сохранить</button>
          <button type="button" class="btn" id="btnCancelAdd">Отмена</button>
        </div>
      </div>
    </form>

    <div class="top-controls">
      <input type="text" class="search-input" id="searchInput" placeholder="Поиск по сайту или логину..." autocomplete="off" />
    </div>

    <div class="list-panel" id="passwordsList"></div>
  </div>

  <div class="toast" id="toast">Скопировано</div>

  <script>
    let rawItems = {items_json};
    let editingId = null;

    const listEl = document.getElementById('passwordsList');
    const searchInput = document.getElementById('searchInput');
    const fileInput = document.getElementById('importFileInput');
    const addForm = document.getElementById('addForm');
    const toastEl = document.getElementById('toast');
    let toastTimer = null;

    function showToast(msg) {{
      toastEl.textContent = msg;
      toastEl.classList.add('show');
      if (toastTimer) clearTimeout(toastTimer);
      toastTimer = setTimeout(() => toastEl.classList.remove('show'), 1800);
    }}

    function generatePassword(len = 18) {{
      const chars = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%&*-_';
      const arr = new Uint32Array(len);
      crypto.getRandomValues(arr);
      let s = '';
      for (let i = 0; i < len; i++) s += chars[arr[i] % chars.length];
      return s;
    }}

    function esc(s) {{
      return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    }}

    function render(query = '') {{
      document.getElementById('countLabel').textContent = String(rawItems.length);
      const q = query.toLowerCase().trim();
      const filtered = rawItems.filter(p => !q || (p.origin || '').toLowerCase().includes(q) || (p.username || '').toLowerCase().includes(q));

      if (filtered.length === 0) {{
        listEl.innerHTML = `<div class="empty-state">${{q ? 'Ничего не найдено по запросу' : 'Сохранённых паролей пока нет. Войдите на любой сайт, добавьте пароль вручную или импортируйте CSV.'}}</div>`;
        return;
      }}

      listEl.innerHTML = filtered.map(p => {{
        let host = p.origin || '';
        try {{ host = new URL(p.origin).hostname || p.origin; }} catch(e) {{}}
        const ch = (host.replace(/^www\./, '')[0] || 'S').toUpperCase();
        const fav = host.includes('.') ? `<img src="https://www.google.com/s2/favicons?domain=${{encodeURIComponent(host)}}&sz=32" alt="" onerror="this.outerHTML='${{ch}}'" />` : ch;

        return `
          <div class="pwd-row">
            <div class="site-fav">${{fav}}</div>
            <a class="site-link" href="${{esc(p.origin)}}" title="${{esc(p.origin)}}">${{esc(host)}}</a>
            <div class="cell-user" title="Нажмите, чтобы скопировать логин" onclick="copyText('${{encodeURIComponent(p.username || '')}}', 'Логин скопирован')">${{esc(p.username || '—')}}</div>
            <div class="cell-pass">
              <input class="pass-field" type="password" id="pf_${{p.id}}" value="${{esc(p.password)}}" readonly />
            </div>
            <div class="row-btns">
              <button class="icon-btn" onclick="toggleReveal(${{p.id}}, this)" title="Показать/скрыть">Показать</button>
              <button class="icon-btn" onclick="copyText('${{encodeURIComponent(p.password)}}', 'Пароль скопирован')" title="Копировать пароль">Копия</button>
              <button class="icon-btn" onclick="startEdit(${{p.id}})" title="Изменить запись">Изменить</button>
              <button class="icon-btn" style="color:var(--danger);" onclick="deletePass(${{p.id}})" title="Удалить">✕</button>
            </div>
          </div>
        `;
      }}).join('');
    }}

    window.toggleReveal = function(id, btn) {{
      const el = document.getElementById('pf_' + id);
      if (!el) return;
      const show = el.type === 'password';
      el.type = show ? 'text' : 'password';
      btn.textContent = show ? 'Скрыть' : 'Показать';
    }};

    window.copyText = function(enc, msg) {{
      const txt = decodeURIComponent(enc);
      if (!txt) return;
      navigator.clipboard.writeText(txt).then(() => showToast(msg)).catch(() => showToast(msg));
    }};

    window.startEdit = function(id) {{
      const item = rawItems.find(i => i.id === id);
      if (!item) return;
      editingId = id;
      document.getElementById('formHeading').textContent = 'Редактирование пароля';
      document.getElementById('inpOrigin').value = item.origin;
      document.getElementById('inpUser').value = item.username;
      document.getElementById('inpPass').value = item.password;
      addForm.classList.add('open');
      document.getElementById('inpPass').focus();
    }};

    window.deletePass = function(id) {{
      rawItems = rawItems.filter(i => i.id !== id);
      render(searchInput.value);
      fetch('ferro://api/passwords/delete?id=' + id).catch(() => {{}});
      showToast('Пароль удалён');
    }};

    document.getElementById('btnToggleAdd').addEventListener('click', () => {{
      editingId = null;
      document.getElementById('formHeading').textContent = 'Новая запись';
      document.getElementById('inpOrigin').value = '';
      document.getElementById('inpUser').value = '';
      document.getElementById('inpPass').value = '';
      addForm.classList.toggle('open');
      if (addForm.classList.contains('open')) document.getElementById('inpOrigin').focus();
    }});

    document.getElementById('btnCancelAdd').addEventListener('click', () => {{
      editingId = null;
      addForm.classList.remove('open');
    }});

    document.getElementById('btnGenInForm').addEventListener('click', () => {{
      document.getElementById('inpPass').value = generatePassword(18);
    }});

    document.getElementById('btnGenQuick').addEventListener('click', () => {{
      const pwd = generatePassword(18);
      navigator.clipboard.writeText(pwd).catch(() => {{}});
      showToast('Сгенерирован и скопирован: ' + pwd);
    }});

    addForm.addEventListener('submit', (e) => {{
      e.preventDefault();
      let origin = document.getElementById('inpOrigin').value.trim();
      const username = document.getElementById('inpUser').value.trim();
      const password = document.getElementById('inpPass').value;
      if (!origin || !password) return;
      if (!origin.startsWith('http://') && !origin.startsWith('https://')) origin = 'https://' + origin;

      if (editingId !== null) {{
        const targetId = editingId;
        const item = rawItems.find(i => i.id === targetId);
        if (item) {{
          item.origin = origin;
          item.username = username;
          item.password = password;
        }}
        fetch(`ferro://api/passwords/update?id=${{targetId}}&origin=${{encodeURIComponent(origin)}}&user=${{encodeURIComponent(username)}}&pass=${{encodeURIComponent(password)}}`).catch(() => {{}});
        showToast('Запись обновлена');
      }} else {{
        if (window.ipc) {{
          window.ipc.postMessage(JSON.stringify({{ action: 'save_password', url: origin, title: username, pass: password }}));
        }} else {{
          fetch(`ferro://api/passwords/save?origin=${{encodeURIComponent(origin)}}&user=${{encodeURIComponent(username)}}&pass=${{encodeURIComponent(password)}}`).catch(() => {{}});
        }}
        const existing = rawItems.find(i => i.origin.toLowerCase() === origin.toLowerCase() && i.username.toLowerCase() === username.toLowerCase());
        if (existing) {{
          existing.password = password;
        }} else {{
          rawItems.unshift({{ id: Date.now(), origin, username, password, created_at: Math.floor(Date.now() / 1000) }});
        }}
        showToast('Пароль сохранён');
      }}

      editingId = null;
      addForm.classList.remove('open');
      render(searchInput.value);
    }});

    document.getElementById('btnImportCsv').addEventListener('click', () => fileInput.click());

    fileInput.addEventListener('change', (e) => {{
      const file = e.target.files[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = (ev) => {{
        fetch('ferro://api/passwords/import', {{ method: 'POST', body: ev.target.result }})
          .then(r => r.json())
          .then(res => {{
            showToast(`Импортировано паролей: ${{res.imported || 'готово'}}`);
            setTimeout(() => window.location.reload(), 450);
          }})
          .catch(() => window.location.reload());
      }};
      reader.readAsText(file);
    }});

    document.getElementById('btnExportCsv').addEventListener('click', () => {{
      if (rawItems.length === 0) {{
        showToast('Список паролей пуст');
        return;
      }}
      const escCsv = (s) => /[",\n]/.test(s) ? `"${{s.replace(/"/g, '""')}}"` : s;
      const csv = 'name,url,username,password\n' + rawItems.map(i => [i.origin, i.origin, i.username, i.password].map(v => escCsv(v || '')).join(',')).join('\n');
      const blob = new Blob([csv], {{ type: 'text/csv;charset=utf-8' }});
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = 'ferro-passwords.csv';
      a.click();
      showToast('Экспортировано в ferro-passwords.csv');
    }});

    searchInput.addEventListener('input', (e) => render(e.target.value));

    window.applySettings = function(s) {{
      if (!s) return;
      const isLight = s.theme_mode === 'light';
      document.documentElement.setAttribute('data-theme', isLight ? 'light' : 'dark');
      const eff = isLight ? '#18181b' : '#ececec';
      document.documentElement.style.setProperty('--accent', eff);
      document.documentElement.style.setProperty('--accent-fg', isLight ? '#ffffff' : '#111113');
      if (s.corner_radius !== undefined) {{
        document.documentElement.style.setProperty('--radius-ctrl', s.corner_radius + 'px');
      }}
    }};

    fetch('ferro://api/settings/get')
      .then(r => r.json())
      .then(s => window.applySettings(s))
      .catch(() => {{}});

    render();
  </script>
</body>
</html>
"#
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_parser_and_import() {
        let row = parse_csv_row(r#""GitHub, Inc.",https://github.com,user@test.com,"p@ss,word""123""#);
        assert_eq!(row.len(), 4);
        assert_eq!(row[1], "https://github.com");
        assert_eq!(row[2], "user@test.com");
        assert_eq!(row[3], "p@ss,word\"123");
    }
}
