use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

    pub fn add(&mut self, origin: &str, username: &str, password: &str) {
        let clean_origin = origin.trim();
        let clean_user = username.trim();
        let clean_pass = password.trim();

        if clean_origin.is_empty() || clean_pass.is_empty() {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Some(existing) = self
            .items
            .iter_mut()
            .find(|p| p.origin == clean_origin && p.username == clean_user)
        {
            existing.password = clean_pass.to_string();
            existing.created_at = now;
        } else {
            let id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            self.items.push(SavedPassword {
                id,
                origin: clean_origin.to_string(),
                username: clean_user.to_string(),
                password: clean_pass.to_string(),
                created_at: now,
            });
        }

        self.save();
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|p| p.id != id);
        self.save();
    }

    pub fn import_chrome_csv(&mut self, csv_text: &str) -> usize {
        let mut count = 0;
        let mut lines = csv_text.lines();
        // Skip header if present
        let first = lines.next();
        if first.is_none() {
            return 0;
        }

        for line in lines {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim_matches('"').trim()).collect();
            // Standard Chrome CSV: name,url,username,password
            if parts.len() >= 4 {
                let url = parts[1];
                let user = parts[2];
                let pass = parts[3];
                if !url.is_empty() && !pass.is_empty() {
                    self.add(url, user, pass);
                    count += 1;
                }
            }
        }
        count
    }

    pub fn generate_autofill_script(&self) -> String {
        let json = serde_json::to_string(&self.items).unwrap_or_else(|_| "[]".to_string());
        format!(
            r#"
            (function() {{
                try {{
                    const passwords = {};
                    if (!passwords || passwords.length === 0) return;

                    const curOrigin = window.location.origin.toLowerCase();
                    const curHost = window.location.hostname.toLowerCase();

                    const match = passwords.find(p => {{
                        const pUrl = (p.origin || '').toLowerCase();
                        return pUrl.includes(curHost) || curOrigin.includes(pUrl);
                    }});

                    if (!match) return;

                    function doAutofill() {{
                        const passInputs = document.querySelectorAll('input[type="password"]');
                        if (passInputs.length === 0) return;

                        passInputs.forEach(passInput => {{
                            if (!passInput.value) {{
                                passInput.value = match.password;
                                passInput.dispatchEvent(new Event('input', {{ bubbles: true }}));
                                passInput.dispatchEvent(new Event('change', {{ bubbles: true }}));
                            }}

                            // Find preceding username/email input
                            const form = passInput.form || document;
                            const textInputs = form.querySelectorAll('input[type="text"], input[type="email"], input:not([type])');
                            textInputs.forEach(textInput => {{
                                if (!textInput.value && match.username) {{
                                    textInput.value = match.username;
                                    textInput.dispatchEvent(new Event('input', {{ bubbles: true }}));
                                    textInput.dispatchEvent(new Event('change', {{ bubbles: true }}));
                                }}
                            }});
                        }});
                    }}

                    if (document.readyState === 'loading') {{
                        document.addEventListener('DOMContentLoaded', doAutofill);
                    }} else {{
                        doAutofill();
                    }}
                    setTimeout(doAutofill, 800);
                    setTimeout(doAutofill, 2000);

                    // Firefox-style Save Password Banner
                    function showSavePasswordBanner(origin, username, password) {{
                        if (!password || password.length < 1) return;
                        if (document.getElementById('ferro-save-pwd-banner')) return;

                        const banner = document.createElement('div');
                        banner.id = 'ferro-save-pwd-banner';
                        banner.style.cssText = 'position:fixed;top:14px;right:18px;width:320px;background:#141414;border:1px solid #333333;border-radius:2px;box-shadow:0 8px 24px rgba(0,0,0,0.7);z-index:2147483647;font-family:Inter,-apple-system,sans-serif;color:#ececec;padding:14px;box-sizing:border-box;';

                        const displayHost = window.location.hostname;
                        banner.innerHTML = `
                            <style>
                                .ferro-pwd-title {{ font-size: 13px; font-weight: 600; margin-bottom: 10px; display: flex; align-items: center; gap: 6px; }}
                                .ferro-pwd-row {{ display: flex; align-items: center; margin-bottom: 8px; gap: 8px; font-size: 12px; }}
                                .ferro-pwd-label {{ color: #888888; width: 52px; flex-shrink: 0; }}
                                .ferro-pwd-val {{ flex: 1; background: #111111; border: 1px solid #2a2a2a; border-radius: 2px; color: #ececec; padding: 4px 8px; font-family: monospace; font-size: 12px; outline: none; }}
                                .ferro-pwd-actions {{ display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }}
                                .ferro-btn-save {{ background: #ececec; color: #111111; border: none; border-radius: 2px; padding: 5px 12px; font-size: 12px; font-weight: 600; cursor: pointer; }}
                                .ferro-btn-save:hover {{ background: #ffffff; }}
                                .ferro-btn-dismiss {{ background: transparent; color: #888888; border: 1px solid #2a2a2a; border-radius: 2px; padding: 5px 10px; font-size: 12px; cursor: pointer; }}
                                .ferro-btn-dismiss:hover {{ background: #222222; color: #ececec; }}
                            </style>
                            <div class="ferro-pwd-title">
                                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"></path></svg>
                                <span>Сохранить пароль для <b>${{displayHost}}</b>?</span>
                            </div>
                            <div class="ferro-pwd-row">
                                <span class="ferro-pwd-label">Логин:</span>
                                <input class="ferro-pwd-val" id="ferro-input-user" value="${{username || ''}}" />
                            </div>
                            <div class="ferro-pwd-row">
                                <span class="ferro-pwd-label">Пароль:</span>
                                <input class="ferro-pwd-val" id="ferro-input-pass" type="password" value="${{password}}" />
                            </div>
                            <div class="ferro-pwd-actions">
                                <button class="ferro-btn-dismiss" id="ferro-btn-no">Не сейчас</button>
                                <button class="ferro-btn-save" id="ferro-btn-yes">Сохранить</button>
                            </div>
                        `;

                        (document.body || document.documentElement).appendChild(banner);

                        document.getElementById('ferro-btn-no').addEventListener('click', () => {{
                            banner.remove();
                        }});

                        document.getElementById('ferro-btn-yes').addEventListener('click', () => {{
                            const finalUser = document.getElementById('ferro-input-user').value.trim();
                            const finalPass = document.getElementById('ferro-input-pass').value.trim();
                            const saveUrl = 'ferro://api/passwords/save?origin=' + encodeURIComponent(window.location.origin)
                                + '&user=' + encodeURIComponent(finalUser)
                                + '&pass=' + encodeURIComponent(finalPass);
                            
                            const img = new Image();
                            img.src = saveUrl;
                            try {{ fetch(saveUrl); }} catch(e){{}}
                            banner.remove();
                        }});
                    }}

                    function findCredentials(target) {{
                        const root = (target && target.closest ? target.closest('form') : null) || document;
                        const passInput = root.querySelector('input[type="password"]');
                        if (!passInput || !passInput.value) return null;

                        let userInput = null;
                        const textInputs = Array.from(root.querySelectorAll('input[type="text"], input[type="email"], input[name*="user"], input[name*="login"], input[name*="email"]'));
                        if (textInputs.length > 0) {{
                            userInput = textInputs[0];
                        }}

                        return {{
                            username: userInput ? userInput.value.trim() : '',
                            password: passInput.value
                        }};
                    }}

                    document.addEventListener('submit', function(e) {{
                        const creds = findCredentials(e.target);
                        if (creds && creds.password) {{
                            setTimeout(() => showSavePasswordBanner(window.location.origin, creds.username, creds.password), 200);
                        }}
                    }}, true);

                    document.addEventListener('click', function(e) {{
                        const btn = e.target.closest('button, input[type="submit"], [role="button"]');
                        if (!btn) return;
                        const text = (btn.innerText || btn.value || '').toLowerCase();
                        if (/войти|вход|login|sign in|log in|submit|продолжить|next/i.test(text)) {{
                            const creds = findCredentials(btn);
                            if (creds && creds.password) {{
                                setTimeout(() => showSavePasswordBanner(window.location.origin, creds.username, creds.password), 400);
                            }}
                        }}
                    }}, true);

                    document.addEventListener('keydown', function(e) {{
                        if (e.key === 'Enter' && e.target && e.target.type === 'password') {{
                            const creds = findCredentials(e.target);
                            if (creds && creds.password) {{
                                setTimeout(() => showSavePasswordBanner(window.location.origin, creds.username, creds.password), 400);
                            }}
                        }}
                    }}, true);
                }} catch(e) {{}}
            }})();
            "#,
            json
        )
    }

    pub fn render_html(&self) -> String {
        let items_json = serde_json::to_string(&self.items).unwrap_or_else(|_| "[]".to_string());

        format!(
            r#"<!DOCTYPE html>
<html lang="ru">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Пароли и аккаунт — Ferro</title>
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet" />
  <style>
    :root {{
      --bg-surface: #141414;
      --bg-card: #191919;
      --bg-hover: #222222;
      --bg-input: #111111;
      --border-hairline: #2a2a2a;
      --border-strong: #383838;
      --text-primary: #ececec;
      --text-secondary: #9c9c9c;
      --text-muted: #666666;
      --accent: #e0e0e0;
      --danger: #cf4d4d;
      --radius-ctrl: 2px;
      --font-sans: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
      --font-mono: 'JetBrains Mono', ui-monospace, monospace;
    }}

    [data-theme="light"], :root[data-theme="light"] {{
      --bg-surface: #f4f4f5;
      --bg-card: #ffffff;
      --bg-hover: #eaeaea;
      --bg-input: #ffffff;
      --border-hairline: #dcdcdc;
      --border-strong: #bebebe;
      --text-primary: #141414;
      --text-secondary: #555555;
      --text-muted: #888888;
      --accent: #111111;
    }}

    * {{
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }}

    body {{
      min-height: 100vh;
      background: var(--bg-surface);
      color: var(--text-primary);
      font-family: var(--font-sans);
      display: flex;
      flex-direction: column;
      align-items: center;
      padding: 32px 16px 64px;
      -webkit-font-smoothing: antialiased;
    }}

    .container {{
      width: 100%;
      max-width: 840px;
      display: flex;
      flex-direction: column;
      gap: 20px;
    }}

    .header-bar {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding-bottom: 16px;
      border-bottom: 1px solid var(--border-hairline);
    }}

    .title-area {{
      display: flex;
      align-items: center;
      gap: 10px;
    }}

    .title-icon {{
      width: 20px;
      height: 20px;
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--text-secondary);
    }}

    h1 {{
      font-size: 18px;
      font-weight: 600;
      letter-spacing: -0.3px;
    }}

    .sync-card {{
      background: var(--bg-card);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      padding: 16px;
      display: flex;
      flex-direction: column;
      gap: 12px;
    }}

    .sync-header {{
      display: flex;
      align-items: center;
      justify-content: space-between;
    }}

    .sync-title {{
      font-size: 14px;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 8px;
    }}

    .sync-desc {{
      font-size: 12px;
      color: var(--text-secondary);
      line-height: 1.5;
    }}

    .sync-actions {{
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;
    }}

    .btn {{
      height: 32px;
      padding: 0 12px;
      background: var(--bg-surface);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      color: var(--text-secondary);
      font-size: 12px;
      font-family: inherit;
      cursor: pointer;
      display: flex;
      align-items: center;
      gap: 6px;
      transition: all 0.12s ease;
      white-space: nowrap;
      text-decoration: none;
    }}

    .btn:hover {{
      background: var(--bg-hover);
      color: var(--text-primary);
      border-color: var(--border-strong);
    }}

    .btn-primary {{
      background: var(--text-primary);
      color: var(--bg-surface);
      border-color: var(--text-primary);
      font-weight: 500;
    }}

    .btn-primary:hover {{
      background: #ffffff;
      color: #000000;
    }}

    .search-box {{
      position: relative;
      width: 100%;
    }}

    .search-box input {{
      width: 100%;
      height: 34px;
      background: var(--bg-input);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      padding: 0 12px;
      color: var(--text-primary);
      font-size: 13px;
      font-family: inherit;
      outline: none;
    }}

    .passwords-list {{
      display: flex;
      flex-direction: column;
      background: var(--bg-card);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      overflow: hidden;
    }}

    .password-item {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 14px;
      border-bottom: 1px solid var(--border-hairline);
      gap: 12px;
    }}

    .password-item:last-child {{
      border-bottom: none;
    }}

    .pwd-info {{
      display: flex;
      flex-direction: column;
      gap: 3px;
      min-width: 0;
      flex: 1;
    }}

    .pwd-origin {{
      font-size: 13px;
      font-weight: 500;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }}

    .pwd-user {{
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--text-secondary);
    }}

    .pwd-creds {{
      display: flex;
      align-items: center;
      gap: 8px;
    }}

    .pwd-masked {{
      font-family: var(--font-mono);
      font-size: 12px;
      background: var(--bg-input);
      padding: 4px 8px;
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      min-width: 100px;
      text-align: center;
    }}

    .empty-state {{
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 48px 20px;
      gap: 12px;
      color: var(--text-muted);
      font-size: 13px;
      background: var(--bg-card);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
    }}

    #importFileInput {{
      display: none;
    }}
  </style>
</head>
<body>
  <div class="container">
    <div class="header-bar">
      <div class="title-area">
        <div class="title-icon">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect>
            <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
          </svg>
        </div>
        <h1>Пароли и аккаунт Google</h1>
      </div>
    </div>

    <!-- Google Account Card -->
    <div class="sync-card">
      <div class="sync-header">
        <div class="sync-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12.545,10.239v3.821h5.445c-0.712,2.315-2.647,3.972-5.445,3.972c-3.332,0-6.033-2.701-6.033-6.032s2.701-6.032,6.033-6.032c1.498,0,2.866,0.549,3.921,1.453l2.814-2.814C17.503,2.988,15.139,2,12.545,2C7.021,2,2.543,6.477,2.543,12s4.478,10,10.002,10c8.396,0,10.249-7.85,9.426-11.748L12.545,10.239z"/>
          </svg>
          <span>Вход в Google и синхронизация</span>
        </div>
      </div>
      <div class="sync-desc">
        В браузере настроен оригинальный Chrome User-Agent, поэтому вы можете полноценно войти в свой аккаунт Google (YouTube, Gmail, Диск). Закрытый протокол автосинхронизации Chrome Sync заблокирован Google для сторонних браузеров с 2021 года, но все сохранённые пароли из Google Chrome можно перенести в 1 клик через экспорт CSV.
      </div>
      <div class="sync-actions">
        <a class="btn btn-primary" href="https://accounts.google.com" target="_self">
          Войти в Google Аккаунт
        </a>
        <a class="btn" href="https://passwords.google.com" target="_self">
          Открыть passwords.google.com
        </a>
        <button class="btn" onclick="document.getElementById('importFileInput').click()">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
            <polyline points="17 8 12 3 7 8"></polyline>
            <line x1="12" y1="3" x2="12" y2="15"></line>
          </svg>
          Импорт паролей из Chrome (.csv)
        </button>
        <input type="file" id="importFileInput" accept=".csv" />
      </div>
    </div>

    <!-- Search & List -->
    <div class="search-box">
      <input type="text" id="searchInput" placeholder="Поиск по сохранённым паролям..." />
    </div>

    <div class="passwords-list" id="passwordsList"></div>
  </div>

  <script>
    let rawItems = {};
    const list = document.getElementById('passwordsList');
    const searchInput = document.getElementById('searchInput');
    const fileInput = document.getElementById('importFileInput');

    function render(query = '') {{
      query = query.toLowerCase().trim();
      const filtered = rawItems.filter(p => {{
        if (!query) return true;
        return (p.origin && p.origin.toLowerCase().includes(query)) ||
               (p.username && p.username.toLowerCase().includes(query));
      }});

      if (filtered.length === 0) {{
        list.innerHTML = `
          <div class="empty-state">
            <span>${{query ? 'Ничего не найдено' : 'Нет сохранённых паролей. Вы можете импортировать CSV из Chrome.'}}</span>
          </div>
        `;
        return;
      }}

      let html = '';
      filtered.forEach(p => {{
        const safeOrigin = p.origin.replace(/</g, '&lt;').replace(/>/g, '&gt;');
        const safeUser = p.username.replace(/</g, '&lt;').replace(/>/g, '&gt;');
        html += `
          <div class="password-item">
            <div class="pwd-info">
              <div class="pwd-origin">${{safeOrigin}}</div>
              <div class="pwd-user">${{safeUser}}</div>
            </div>
            <div class="pwd-creds">
              <div class="pwd-masked" id="pwd_${{p.id}}">••••••••</div>
              <button class="btn" onclick="togglePass(${{p.id}}, '${{encodeURIComponent(p.password)}}')">Показать</button>
              <button class="btn" onclick="copyPass('${{encodeURIComponent(p.password)}}')">Копировать</button>
              <button class="btn btn-danger" onclick="deletePass(${{p.id}})">✕</button>
            </div>
          </div>
        `;
      }});
      list.innerHTML = html;
    }}

    function togglePass(id, enc) {{
      const el = document.getElementById('pwd_' + id);
      if (el.textContent === '••••••••') {{
        el.textContent = decodeURIComponent(enc);
      }} else {{
        el.textContent = '••••••••';
      }}
    }}

    function copyPass(enc) {{
      navigator.clipboard.writeText(decodeURIComponent(enc));
      alert('Пароль скопирован в буфер обмена');
    }}

    function deletePass(id) {{
      if (confirm('Удалить этот пароль?')) {{
        rawItems = rawItems.filter(i => i.id !== id);
        render(searchInput.value);
        window.location.href = 'ferro://api/passwords/delete?id=' + id;
      }}
    }}

    fileInput.addEventListener('change', (e) => {{
      const file = e.target.files[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = (event) => {{
        const csv = event.target.result;
        // Post to Ferro API
        fetch('ferro://api/passwords/import', {{
          method: 'POST',
          body: csv
        }}).then(() => {{
          alert('Пароли успешно импортированы!');
          window.location.reload();
        }}).catch(() => {{
          // Fallback via location
          alert('Импорт завершен');
          window.location.reload();
        }});
      }};
      reader.readAsText(file);
    }});

    searchInput.addEventListener('input', (e) => render(e.target.value));

    window.applySettings = function(s) {{
      if (!s) return;
      if (s.theme_mode) document.documentElement.setAttribute('data-theme', s.theme_mode);
      if (s.accent_color) {{
        document.documentElement.style.setProperty('--accent', s.accent_color);
        document.documentElement.style.setProperty('--border-strong', s.accent_color);
      }}
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
"#,
            items_json
        )
    }
}
