use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: u64,
    pub filename: String,
    pub url: String,
    pub path: String,
    pub state: String, // "downloading" | "completed" | "failed"
    pub progress: u32, // 0..=100
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadsData {
    pub items: Vec<DownloadItem>,
}

impl DownloadsData {
    pub fn file_path() -> PathBuf {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ferro");
        let _ = fs::create_dir_all(&dir);
        dir.join("downloads.json")
    }

    pub fn load_or_default() -> Self {
        let path = Self::file_path();
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<DownloadsData>(&raw) {
                    return parsed;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(Self::file_path(), json);
        }
    }

    pub fn add_started(&mut self, filename: &str, url: &str, path: &str) -> u64 {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.items.insert(
            0,
            DownloadItem {
                id,
                filename: filename.to_string(),
                url: url.to_string(),
                path: path.to_string(),
                state: "downloading".to_string(),
                progress: 0,
                timestamp: ts,
            },
        );
        if self.items.len() > 300 {
            self.items.truncate(300);
        }
        self.save();
        id
    }

    pub fn update_progress(&mut self, id: u64, pct: u32) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.progress = pct.min(100);
        }
    }

    pub fn mark_finished(&mut self, id: u64) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            if item.state == "downloading" {
                item.state = "completed".to_string();
                item.progress = 100;
            }
        }
        self.save();
    }

    pub fn mark_failed(&mut self, id: u64) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.state = "failed".to_string();
        }
        self.save();
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|i| i.id != id);
        self.save();
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.save();
    }

    pub fn render_html(&self) -> String {
        let items_json = serde_json::to_string(&self.items).unwrap_or_else(|_| "[]".to_string());
        format!(
            r#"<!DOCTYPE html>
<html lang="ru" data-theme="dark">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Загрузки — Ferro</title>
  <style>
    :root {{
      --bg-base: #141414;
      --bg-panel: #191919;
      --bg-elevated: #202020;
      --bg-hover: #242424;
      --bg-input: #111111;
      --border-hairline: #262626;
      --border-strong: #383838;
      --text-primary: #ececec;
      --text-secondary: #9a9a9a;
      --text-muted: #666666;
      --accent: #ececec;
      --danger: #dc4c4c;
      --success: #10b981;
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
    }}
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      min-height: 100vh;
      background: var(--bg-base);
      color: var(--text-primary);
      font-family: var(--font-ui);
      padding: 32px 20px 64px;
      display: flex;
      justify-content: center;
      -webkit-font-smoothing: antialiased;
    }}
    .container {{
      width: 100%;
      max-width: 860px;
      display: flex;
      flex-direction: column;
      gap: 18px;
    }}
    .header {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding-bottom: 16px;
      border-bottom: 1px solid var(--border-hairline);
      gap: 12px;
    }}
    .header-left {{
      display: flex;
      align-items: center;
      gap: 10px;
    }}
    h1 {{ font-size: 18px; font-weight: 600; }}
    .actions {{ display: flex; gap: 8px; }}
    .btn {{
      height: 32px;
      padding: 0 12px;
      background: var(--bg-elevated);
      color: var(--text-primary);
      border: 1px solid var(--border-strong);
      border-radius: var(--radius-ctrl);
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 6px;
    }}
    .btn:hover {{ background: var(--bg-hover); }}
    .btn-danger {{ color: var(--danger); border-color: rgba(220,76,76,0.4); background: transparent; }}
    .btn-danger:hover {{ background: rgba(220,76,76,0.12); border-color: var(--danger); }}
    .search-input {{
      width: 100%;
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
    .panel {{
      background: var(--bg-panel);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      overflow: hidden;
    }}
    .dl-item {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 14px 16px;
      border-bottom: 1px solid var(--border-hairline);
    }}
    .dl-item:last-child {{ border-bottom: none; }}
    .dl-icon {{
      width: 34px;
      height: 34px;
      border-radius: var(--radius-ctrl);
      background: var(--bg-elevated);
      border: 1px solid var(--border-hairline);
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
      color: var(--text-secondary);
    }}
    .dl-info {{ flex: 1; min-width: 0; }}
    .dl-name {{ font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
    .dl-meta {{ font-size: 11.5px; color: var(--text-secondary); font-family: var(--font-mono); margin-top: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
    .prog-bar {{
      height: 4px;
      background: var(--bg-elevated);
      border-radius: 2px;
      margin-top: 6px;
      overflow: hidden;
    }}
    .prog-fill {{ height: 100%; background: var(--accent); transition: width 0.2s ease; }}
    .empty {{
      padding: 48px 20px;
      text-align: center;
      color: var(--text-muted);
      font-size: 13px;
    }}
  </style>
</head>
<body>
  <div class="container">
    <div class="header">
      <div class="header-left">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
        <h1>Загрузки</h1>
      </div>
      <div class="actions">
        <button class="btn" id="btnOpenFolder">Открыть папку загрузок</button>
        <button class="btn btn-danger" id="btnClearAll">Очистить список</button>
      </div>
    </div>

    <input type="text" class="search-input" id="searchInput" placeholder="Поиск загруженных файлов..." />

    <div class="panel" id="dlList"></div>
  </div>

  <script>
    let items = {items_json};
    const listEl = document.getElementById('dlList');
    const searchEl = document.getElementById('searchInput');

    function sendIpc(payload) {{
      if (window.ipc) window.ipc.postMessage(JSON.stringify(payload));
    }}

    function esc(s) {{
      return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    }}

    function render() {{
      const q = searchEl.value.trim().toLowerCase();
      const filtered = items.filter(i => !q || (i.filename || '').toLowerCase().includes(q) || (i.path || '').toLowerCase().includes(q));
      if (filtered.length === 0) {{
        listEl.innerHTML = `<div class="empty">${{q ? 'Файлы не найдены' : 'Список загрузок пуст'}}</div>`;
        return;
      }}
      listEl.innerHTML = filtered.map(it => {{
        const statusLabel = it.state === 'downloading'
          ? `Загрузка: ${{it.progress || 0}}%`
          : (it.state === 'failed' ? 'Ошибка загрузки' : 'Завершено');
        return `
          <div class="dl-item">
            <div class="dl-icon">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
            </div>
            <div class="dl-info">
              <div class="dl-name">${{esc(it.filename)}}</div>
              <div class="dl-meta">${{statusLabel}} • ${{esc(it.path)}}</div>
              ${{it.state === 'downloading' ? `<div class="prog-bar"><div class="prog-fill" style="width:${{it.progress || 0}}%"></div></div>` : ''}}
            </div>
            <div class="actions">
              <button class="btn" onclick="openFile('${{encodeURIComponent(it.path)}}')">Открыть</button>
              <button class="btn" onclick="openDir('${{encodeURIComponent(it.path)}}')">В папке</button>
              <button class="btn btn-danger" onclick="removeItem(${{it.id}})">✕</button>
            </div>
          </div>
        `;
      }}).join('');
    }}

    window.openFile = function(encPath) {{
      sendIpc({{ action: 'open_path', url: decodeURIComponent(encPath) }});
    }};

    window.openDir = function(encPath) {{
      const p = decodeURIComponent(encPath);
      const dir = p.substring(0, p.lastIndexOf('/')) || p;
      sendIpc({{ action: 'open_path', url: dir }});
    }};

    window.removeItem = function(id) {{
      items = items.filter(i => i.id !== id);
      render();
      fetch('ferro://api/downloads/delete?id=' + id).catch(() => {{}});
    }};

    document.getElementById('btnOpenFolder').addEventListener('click', () => {{
      sendIpc({{ action: 'open_downloads_folder' }});
    }});

    document.getElementById('btnClearAll').addEventListener('click', () => {{
      items = [];
      render();
      fetch('ferro://api/downloads/clear').catch(() => {{}});
      sendIpc({{ action: 'clear_downloads' }});
    }});

    searchEl.addEventListener('input', render);

    // Poll while any download is active
    setInterval(() => {{
      if (items.some(i => i.state === 'downloading')) {{
        fetch('ferro://api/downloads/get')
          .then(r => r.json())
          .then(list => {{ if (Array.isArray(list)) {{ items = list; render(); }} }})
          .catch(() => {{}});
      }}
    }}, 900);

    window.applySettings = function(s) {{
      if (!s) return;
      if (s.theme_mode) document.documentElement.setAttribute('data-theme', s.theme_mode);
      if (s.accent_color) document.documentElement.style.setProperty('--accent', s.accent_color);
      if (s.corner_radius !== undefined) document.documentElement.style.setProperty('--radius-ctrl', s.corner_radius + 'px');
    }};

    fetch('ferro://api/settings/get')
      .then(r => r.json())
      .then(s => window.applySettings(s))
      .catch(() => {{}});

    render();
  </script>
</body>
</html>"#
        )
    }
}
