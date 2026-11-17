use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub timestamp: i64,
    pub visit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryData {
    pub items: Vec<HistoryEntry>,
}

impl HistoryData {
    pub fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("ferro");
        let _ = fs::create_dir_all(&path);
        path.push("history.json");
        path
    }

    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(data) = serde_json::from_str::<HistoryData>(&content) {
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

    pub fn add(&mut self, url: &str, title: &str) {
        let clean_url = url.trim();
        if clean_url.is_empty()
            || clean_url == "about:blank"
            || clean_url.starts_with("ferro://")
            || clean_url.starts_with("data:")
            || clean_url.starts_with("javascript:")
        {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let clean_title = if title.trim().is_empty() || title == "Новая вкладка" {
            clean_url.to_string()
        } else {
            title.trim().to_string()
        };

        if let Some(existing) = self.items.iter_mut().find(|item| item.url == clean_url) {
            existing.visit_count += 1;
            existing.timestamp = now;
            if clean_title != clean_url {
                existing.title = clean_title;
            }
            // Move to front
            let entry = existing.clone();
            self.items.retain(|item| item.url != clean_url);
            self.items.insert(0, entry);
        } else {
            let id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            self.items.insert(
                0,
                HistoryEntry {
                    id,
                    url: clean_url.to_string(),
                    title: clean_title,
                    timestamp: now,
                    visit_count: 1,
                },
            );
        }

        // Keep at most 3000 items
        if self.items.len() > 3000 {
            self.items.truncate(3000);
        }

        self.save();
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|item| item.id != id);
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
<html lang="ru">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>История — Ferro</title>
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

    .count-badge {{
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--text-muted);
      background: var(--bg-card);
      border: 1px solid var(--border-hairline);
      padding: 2px 6px;
      border-radius: var(--radius-ctrl);
    }}

    .controls {{
      display: flex;
      align-items: center;
      gap: 10px;
      flex: 1;
      max-width: 440px;
    }}

    .search-box {{
      position: relative;
      flex: 1;
    }}

    .search-box input {{
      width: 100%;
      height: 32px;
      background: var(--bg-input);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      padding: 0 10px 0 32px;
      color: var(--text-primary);
      font-size: 13px;
      font-family: inherit;
      outline: none;
      transition: border-color 0.12s ease;
    }}

    .search-box input:focus {{
      border-color: var(--border-strong);
    }}

    .search-icon {{
      position: absolute;
      left: 10px;
      top: 50%;
      transform: translateY(-50%);
      color: var(--text-muted);
      pointer-events: none;
      display: flex;
    }}

    .btn {{
      height: 32px;
      padding: 0 12px;
      background: var(--bg-card);
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
    }}

    .btn:hover {{
      background: var(--bg-hover);
      color: var(--text-primary);
      border-color: var(--border-strong);
    }}

    .btn-danger:hover {{
      background: rgba(207, 77, 77, 0.15);
      color: var(--danger);
      border-color: var(--danger);
    }}

    .group-section {{
      display: flex;
      flex-direction: column;
      gap: 8px;
    }}

    .group-title {{
      font-size: 12px;
      font-weight: 600;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.5px;
      padding: 4px 0;
    }}

    .items-list {{
      display: flex;
      flex-direction: column;
      background: var(--bg-card);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
      overflow: hidden;
    }}

    .history-item {{
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 10px 14px;
      border-bottom: 1px solid var(--border-hairline);
      transition: background 0.1s ease;
      text-decoration: none;
      color: inherit;
    }}

    .history-item:last-child {{
      border-bottom: none;
    }}

    .history-item:hover {{
      background: var(--bg-hover);
    }}

    .item-time {{
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--text-muted);
      min-width: 44px;
      flex-shrink: 0;
    }}

    .item-content {{
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 2px;
      overflow: hidden;
      cursor: pointer;
    }}

    .item-title {{
      font-size: 13px;
      font-weight: 500;
      color: var(--text-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }}

    .item-url {{
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--text-muted);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }}

    .item-actions {{
      display: flex;
      align-items: center;
      gap: 6px;
      opacity: 0;
      transition: opacity 0.12s ease;
    }}

    .history-item:hover .item-actions {{
      opacity: 1;
    }}

    .del-btn {{
      width: 24px;
      height: 24px;
      background: transparent;
      border: 1px solid transparent;
      border-radius: var(--radius-ctrl);
      color: var(--text-muted);
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.12s ease;
    }}

    .del-btn:hover {{
      background: rgba(207, 77, 77, 0.15);
      color: var(--danger);
      border-color: var(--danger);
    }}

    .empty-state {{
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 64px 20px;
      gap: 12px;
      color: var(--text-muted);
      font-size: 13px;
      background: var(--bg-card);
      border: 1px solid var(--border-hairline);
      border-radius: var(--radius-ctrl);
    }}
  </style>
</head>
<body>
  <div class="container">
    <div class="header-bar">
      <div class="title-area">
        <div class="title-icon">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10"></circle>
            <polyline points="12 6 12 12 16 14"></polyline>
          </svg>
        </div>
        <h1>История</h1>
        <span class="count-badge" id="countBadge">0</span>
      </div>

      <div class="controls">
        <div class="search-box">
          <div class="search-icon">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="11" cy="11" r="8"></circle>
              <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
            </svg>
          </div>
          <input type="text" id="searchInput" placeholder="Поиск в истории..." autocomplete="off" />
        </div>
        <button class="btn btn-danger" id="clearBtn">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="3 6 5 6 21 6"></polyline>
            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
          </svg>
          <span>Очистить</span>
        </button>
      </div>
    </div>

    <div id="historyGroups"></div>
  </div>

  <script>
    let rawItems = {};
    const groupsContainer = document.getElementById('historyGroups');
    const searchInput = document.getElementById('searchInput');
    const countBadge = document.getElementById('countBadge');
    const clearBtn = document.getElementById('clearBtn');

    function formatTime(timestampSec) {{
      const d = new Date(timestampSec * 1000);
      const h = String(d.getHours()).padStart(2, '0');
      const m = String(d.getMinutes()).padStart(2, '0');
      return `${{h}}:${{m}}`;
    }}

    function formatDateGroup(timestampSec) {{
      const date = new Date(timestampSec * 1000);
      const now = new Date();
      
      const isToday = date.toDateString() === now.toDateString();
      if (isToday) return 'Сегодня';

      const yesterday = new Date();
      yesterday.setDate(now.getDate() - 1);
      if (date.toDateString() === yesterday.toDateString()) return 'Вчера';

      return date.toLocaleDateString('ru-RU', {{ day: 'numeric', month: 'long', year: 'numeric' }});
    }}

    function render(filterQuery = '') {{
      const query = filterQuery.toLowerCase().trim();
      const filtered = rawItems.filter(item => {{
        if (!query) return true;
        return (item.title && item.title.toLowerCase().includes(query)) ||
               (item.url && item.url.toLowerCase().includes(query));
      }});

      countBadge.textContent = filtered.length;

      if (filtered.length === 0) {{
        groupsContainer.innerHTML = `
          <div class="empty-state">
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <circle cx="12" cy="12" r="10"></circle>
              <polyline points="12 6 12 12 16 14"></polyline>
            </svg>
            <span>${{query ? 'Ничего не найдено' : 'История просмотров пуста'}}</span>
          </div>
        `;
        return;
      }}

      // Group by date
      const groups = new Map();
      filtered.forEach(item => {{
        const groupKey = formatDateGroup(item.timestamp);
        if (!groups.has(groupKey)) {{
          groups.set(groupKey, []);
        }}
        groups.get(groupKey).push(item);
      }});

      let html = '';
      groups.forEach((items, groupTitle) => {{
        html += `
          <div class="group-section">
            <div class="group-title">${{groupTitle}}</div>
            <div class="items-list">
        `;
        items.forEach(item => {{
          const time = formatTime(item.timestamp);
          const safeTitle = item.title ? item.title.replace(/</g, '&lt;').replace(/>/g, '&gt;') : item.url;
          const safeUrl = item.url.replace(/</g, '&lt;').replace(/>/g, '&gt;');
          html += `
            <div class="history-item">
              <div class="item-time">${{time}}</div>
              <div class="item-content" onclick="openUrl('${{encodeURI(item.url)}}')">
                <div class="item-title">${{safeTitle}}</div>
                <div class="item-url">${{safeUrl}}</div>
              </div>
              <div class="item-actions">
                <button class="del-btn" title="Удалить из истории" onclick="deleteItem(${{item.id}})">
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                    <line x1="18" y1="6" x2="6" y2="18"></line>
                    <line x1="6" y1="6" x2="18" y2="18"></line>
                  </svg>
                </button>
              </div>
            </div>
          `;
        }});
        html += `
            </div>
          </div>
        `;
      }});

      groupsContainer.innerHTML = html;
    }}

    function openUrl(url) {{
      window.location.href = url;
    }}

    function deleteItem(id) {{
      rawItems = rawItems.filter(i => i.id !== id);
      render(searchInput.value);
      window.location.href = 'ferro://api/history/delete?id=' + id;
    }}

    clearBtn.addEventListener('click', () => {{
      if (confirm('Вы уверены, что хотите очистить всю историю?')) {{
        rawItems = [];
        render();
        window.location.href = 'ferro://api/history/clear';
      }}
    }});

    searchInput.addEventListener('input', (e) => {{
      render(e.target.value);
    }});

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
