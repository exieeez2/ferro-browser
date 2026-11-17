use crate::bookmarks::BookmarksData;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteItem {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotesData {
    pub items: Vec<NoteItem>,
}

impl NotesData {
    pub fn file_path() -> PathBuf {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ferro");
        let _ = fs::create_dir_all(&dir);
        dir.join("notes.json")
    }

    pub fn load_or_default() -> Self {
        let path = Self::file_path();
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<NotesData>(&raw) {
                    return parsed;
                }
            }
        }
        Self {
            items: vec![NoteItem {
                id: 1,
                title: "Быстрые заметки".to_string(),
                content: String::new(),
                updated_at: 0,
            }],
        }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(Self::file_path(), json);
        }
    }

    pub fn render_html(&self) -> String {
        let items_json = serde_json::to_string(&self.items).unwrap_or_else(|_| "[]".to_string());
        format!(
            r#"<!DOCTYPE html>
<html lang="ru" data-theme="dark">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Заметки — Ferro</title>
  <style>
    :root {{
      --bg: #111111;
      --bg-surface: #18181b;
      --bg-hover: #222227;
      --border: #2a2a30;
      --text: #ececef;
      --muted: #8e8e98;
      --accent: #ececec;
      --accent-fg: #111111;
      --radius: 6px;
    }}
    html[data-theme="light"] {{
      --bg: #f6f7f9;
      --bg-surface: #ffffff;
      --bg-hover: #eceef2;
      --border: #d5d8e0;
      --text: #18181b;
      --muted: #686b75;
      --accent: #18181b;
      --accent-fg: #ffffff;
    }}
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      background: var(--bg);
      color: var(--text);
      font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
      height: 100vh;
      display: flex;
      flex-direction: column;
      padding: 12px;
      gap: 10px;
    }}
    .top-row {{
      display: flex;
      align-items: center;
      gap: 6px;
    }}
    select {{
      flex: 1;
      height: 32px;
      background: var(--bg-surface);
      color: var(--text);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 0 8px;
      font-size: 12.5px;
      outline: none;
    }}
    .btn {{
      height: 32px;
      padding: 0 10px;
      border-radius: var(--radius);
      border: 1px solid var(--border);
      background: var(--bg-surface);
      color: var(--text);
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
    }}
    .btn:hover {{ background: var(--bg-hover); }}
    .btn-primary {{
      background: var(--accent);
      color: var(--accent-fg);
      border-color: var(--accent);
    }}
    input.title-inp {{
      height: 34px;
      width: 100%;
      background: var(--bg-surface);
      color: var(--text);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 0 10px;
      font-size: 13px;
      font-weight: 600;
      outline: none;
    }}
    textarea {{
      flex: 1;
      width: 100%;
      resize: none;
      background: var(--bg-surface);
      color: var(--text);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 12px;
      font-family: 'Inter', -apple-system, sans-serif;
      font-size: 13px;
      line-height: 1.55;
      outline: none;
    }}
    .foot {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      font-size: 11px;
      color: var(--muted);
    }}
  </style>
</head>
<body>
  <div class="top-row">
    <select id="noteSelect"></select>
    <button class="btn btn-primary" id="btnNew">+ Заметка</button>
    <button class="btn" id="btnCopy" title="Скопировать текст">Копировать</button>
    <button class="btn" id="btnDel" title="Удалить заметку">✕</button>
  </div>
  <input type="text" class="title-inp" id="titleInp" placeholder="Заголовок заметки..." />
  <textarea id="bodyInp" placeholder="Пишите идеи, ссылки, фрагменты кода — всё сохраняется автоматически..."></textarea>
  <div class="foot">
    <span id="charCount">0 символов</span>
    <span id="saveState">Сохранено</span>
  </div>

  <script>
    let notes = {items_json};
    if (!Array.isArray(notes) || notes.length === 0) {{
      notes = [{{ id: Date.now(), title: 'Быстрые заметки', content: '', updated_at: 0 }}];
    }}
    let activeId = notes[0].id;
    let saveTimer = null;

    const selEl = document.getElementById('noteSelect');
    const titleEl = document.getElementById('titleInp');
    const bodyEl = document.getElementById('bodyInp');
    const countEl = document.getElementById('charCount');
    const stateEl = document.getElementById('saveState');

    function getActive() {{
      return notes.find(n => n.id === activeId) || notes[0];
    }}

    function renderSelect() {{
      selEl.innerHTML = notes.map(n => `<option value="${{n.id}}" ${{n.id === activeId ? 'selected' : ''}}>${{(n.title || 'Без названия').replace(/</g, '&lt;')}}</option>`).join('');
      const cur = getActive();
      titleEl.value = cur.title || '';
      bodyEl.value = cur.content || '';
      countEl.textContent = `${{(cur.content || '').length}} симв.`;
    }}

    function persist() {{
      stateEl.textContent = 'Сохранение...';
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => {{
        fetch('ferro://api/notes/save', {{
          method: 'POST',
          body: JSON.stringify({{ items: notes }})
        }}).then(() => {{ stateEl.textContent = 'Сохранено'; }})
          .catch(() => {{ stateEl.textContent = 'Сохранено локально'; }});
      }}, 250);
    }}

    selEl.addEventListener('change', () => {{
      activeId = Number(selEl.value);
      renderSelect();
    }});

    titleEl.addEventListener('input', () => {{
      const cur = getActive();
      cur.title = titleEl.value;
      cur.updated_at = Math.floor(Date.now() / 1000);
      const opt = selEl.querySelector(`option[value="${{cur.id}}"]`);
      if (opt) opt.textContent = cur.title || 'Без названия';
      persist();
    }});

    bodyEl.addEventListener('input', () => {{
      const cur = getActive();
      cur.content = bodyEl.value;
      cur.updated_at = Math.floor(Date.now() / 1000);
      countEl.textContent = `${{cur.content.length}} симв.`;
      persist();
    }});

    document.getElementById('btnNew').addEventListener('click', () => {{
      const n = {{ id: Date.now(), title: 'Новая заметка', content: '', updated_at: Math.floor(Date.now() / 1000) }};
      notes.unshift(n);
      activeId = n.id;
      renderSelect();
      bodyEl.focus();
      persist();
    }});

    document.getElementById('btnCopy').addEventListener('click', () => {{
      navigator.clipboard.writeText(bodyEl.value || '').catch(() => {{}});
      stateEl.textContent = 'Скопировано в буфер';
    }});

    document.getElementById('btnDel').addEventListener('click', () => {{
      if (notes.length <= 1) {{
        notes[0].content = '';
        notes[0].title = 'Быстрые заметки';
      }} else {{
        notes = notes.filter(n => n.id !== activeId);
        activeId = notes[0].id;
      }}
      renderSelect();
      persist();
    }});

    window.applySettings = function(s) {{
      if (!s) return;
      document.documentElement.setAttribute('data-theme', s.theme_mode === 'light' ? 'light' : 'dark');
      if (s.accent_color) {{
        const isLight = s.theme_mode === 'light';
        const eff = (isLight && s.accent_color.toLowerCase() === '#ececec') ? '#18181b' : s.accent_color;
        document.documentElement.style.setProperty('--accent', eff);
        document.documentElement.style.setProperty('--accent-fg', eff.toLowerCase() === '#ececec' ? '#111111' : '#ffffff');
      }}
      if (s.corner_radius !== undefined) document.documentElement.style.setProperty('--radius', Math.min(12, s.corner_radius) + 'px');
    }};

    fetch('ferro://api/settings/get').then(r => r.json()).then(window.applySettings).catch(() => {{}});
    renderSelect();
  </script>
</body>
</html>"#
        )
    }
}

pub fn render_bookmarks_panel_html(bookmarks: &BookmarksData) -> String {
    let items_json = serde_json::to_string(&bookmarks.items).unwrap_or_else(|_| "[]".to_string());
    format!(
        r#"<!DOCTYPE html>
<html lang="ru" data-theme="dark">
<head>
  <meta charset="UTF-8" />
  <title>Закладки — Ferro</title>
  <style>
    :root {{
      --bg: #111111;
      --surface: #18181b;
      --hover: #222227;
      --border: #2a2a30;
      --text: #ececef;
      --muted: #8e8e98;
      --radius: 6px;
    }}
    html[data-theme="light"] {{
      --bg: #f6f7f9;
      --surface: #ffffff;
      --hover: #eceef2;
      --border: #d5d8e0;
      --text: #18181b;
      --muted: #686b75;
    }}
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      background: var(--bg);
      color: var(--text);
      font-family: 'Inter', -apple-system, sans-serif;
      padding: 12px;
    }}
    .top {{ display: flex; gap: 6px; margin-bottom: 10px; }}
    input {{
      flex: 1;
      height: 32px;
      padding: 0 10px;
      background: var(--surface);
      color: var(--text);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      font-size: 12.5px;
      outline: none;
    }}
    .btn {{
      height: 32px;
      padding: 0 10px;
      border-radius: var(--radius);
      border: 1px solid var(--border);
      background: var(--surface);
      color: var(--text);
      font-size: 12px;
      cursor: pointer;
    }}
    .btn:hover {{ background: var(--hover); }}
    .list {{
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      overflow: hidden;
    }}
    .row {{
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 9px 11px;
      border-bottom: 1px solid var(--border);
      cursor: pointer;
    }}
    .row:last-child {{ border-bottom: none; }}
    .row:hover {{ background: var(--hover); }}
    .row img {{ width: 16px; height: 16px; border-radius: 3px; flex-shrink: 0; }}
    .meta {{ flex: 1; min-width: 0; }}
    .title {{ font-size: 12.5px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
    .url {{ font-size: 11px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
    .del {{ border: none; background: transparent; color: var(--muted); cursor: pointer; padding: 4px 6px; border-radius: 4px; }}
    .del:hover {{ color: #ef4444; background: rgba(239,68,68,0.12); }}
    .empty {{ padding: 32px 14px; text-align: center; color: var(--muted); font-size: 12.5px; }}
  </style>
</head>
<body>
  <div class="top">
    <input type="text" id="q" placeholder="Поиск закладок..." />
    <button class="btn" id="btnAddActive" title="Добавить текущую вкладку">+ Текущая</button>
  </div>
  <div class="list" id="list"></div>
  <script>
    let items = {items_json};
    const listEl = document.getElementById('list');
    const qEl = document.getElementById('q');
    function send(p) {{ if (window.ipc) window.ipc.postMessage(JSON.stringify(p)); }}
    function esc(s) {{ return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;'); }}

    function render() {{
      const q = qEl.value.trim().toLowerCase();
      const flt = items.filter(b => !q || (b.title || '').toLowerCase().includes(q) || (b.url || '').toLowerCase().includes(q));
      if (!flt.length) {{
        listEl.innerHTML = '<div class="empty">Закладки не найдены</div>';
        return;
      }}
      listEl.innerHTML = flt.map((b, i) => {{
        let host = '';
        try {{ host = new URL(b.url).hostname; }} catch(e) {{}}
        const ico = host ? `https://www.google.com/s2/favicons?domain=${{encodeURIComponent(host)}}&sz=32` : '';
        return `<div class="row" data-url="${{encodeURIComponent(b.url)}}">
          ${{ico ? `<img src="${{ico}}" alt="" onerror="this.style.display='none'"/>` : ''}}
          <div class="meta">
            <div class="title">${{esc(b.title || b.url)}}</div>
            <div class="url">${{esc(b.url)}}</div>
          </div>
          <button class="del" data-del="${{encodeURIComponent(b.url)}}" data-title="${{encodeURIComponent(b.title || '')}}" title="Удалить">✕</button>
        </div>`;
      }}).join('');

      listEl.querySelectorAll('.row').forEach(r => {{
        r.addEventListener('click', (ev) => {{
          const delBtn = ev.target.closest('.del');
          if (delBtn) {{
            ev.stopPropagation();
            const u = decodeURIComponent(delBtn.dataset.del);
            const t = decodeURIComponent(delBtn.dataset.title);
            items = items.filter(x => x.url !== u);
            send({{ action: 'toggle_bookmark', url: u, title: t }});
            render();
            return;
          }}
          send({{ action: 'navigate', url: decodeURIComponent(r.dataset.url) }});
        }});
      }});
    }}

    document.getElementById('btnAddActive').addEventListener('click', () => {{
      send({{ action: 'toggle_bookmark_active' }});
      setTimeout(() => window.location.reload(), 200);
    }});
    qEl.addEventListener('input', render);
    window.applySettings = function(s) {{
      if (s && s.theme_mode) document.documentElement.setAttribute('data-theme', s.theme_mode === 'light' ? 'light' : 'dark');
    }};
    fetch('ferro://api/settings/get').then(r => r.json()).then(window.applySettings).catch(() => {{}});
    render();
  </script>
</body>
</html>"#
    )
}

pub fn render_sleeping_tab_html(target_url: &str, title: &str) -> String {
    let url_json = serde_json::to_string(target_url).unwrap_or_else(|_| "\"\"".to_string());
    let safe_title = title.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let safe_url = target_url.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r##"<!DOCTYPE html>
<html lang="ru">
<head>
  <meta charset="UTF-8" />
  <title>{safe_title}</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      background: #121215;
      color: #ececef;
      font-family: 'Inter', -apple-system, sans-serif;
      height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 24px;
    }}
    .card {{
      max-width: 440px;
      width: 100%;
      background: #18181c;
      border: 1px solid #2b2b33;
      border-radius: 10px;
      padding: 26px;
      text-align: center;
    }}
    h2 {{ font-size: 15px; font-weight: 600; margin: 12px 0 6px; }}
    p {{ font-size: 12px; color: #90909c; word-break: break-all; margin-bottom: 18px; }}
    button {{
      height: 34px;
      padding: 0 18px;
      border-radius: 6px;
      border: none;
      background: #ececec;
      color: #111111;
      font-size: 12.5px;
      font-weight: 600;
      cursor: pointer;
    }}
  </style>
</head>
<body>
  <div class="card">
    <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="#9ca3af" stroke-width="1.8"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
    <h2>Вкладка выгружена для экономии памяти</h2>
    <p>{safe_url}</p>
    <button id="wakeBtn">Разбудить вкладку</button>
  </div>
  <script>
    const target = {url_json};
    document.getElementById('wakeBtn').addEventListener('click', () => {{
      if (target) window.location.replace(target);
    }});
  </script>
</body>
</html>"##
    )
}
