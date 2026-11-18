/// Google Lens (native curl multipart uploader with valid Origin headers — zero 403!)
/// & Chrome-style Side Panel helpers for Ferro Browser.

use crate::bookmarks::BookmarksData;
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static LENS_SEQ: AtomicU64 = AtomicU64::new(1);

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(input: &[u8]) -> String {
    if input.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(((input.len() + 2) / 3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let clean = input
        .split(',')
        .last()
        .unwrap_or(input)
        .trim();
    if clean.is_empty() {
        return None;
    }

    let mut lut = [255u8; 256];
    for (idx, &ch) in B64_CHARS.iter().enumerate() {
        lut[ch as usize] = idx as u8;
    }

    let mut out = Vec::with_capacity((clean.len() * 3) / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;

    for b in clean.bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let val = lut[b as usize];
        if val == 255 {
            return None;
        }
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// Uploads raw PNG bytes directly to `https://lens.google.com/v3/upload?hl=ru` with proper
/// `Origin: https://lens.google.com` & `Referer: https://lens.google.com/` headers so Google
/// responds with `303 See Other` -> `https://www.google.com/search?vsrid=...&udm=26...&hl=ru`
/// instead of `403 Forbidden`.
#[allow(dead_code)]
pub fn upload_png_to_google_lens(png_bytes: &[u8]) -> Option<String> {
    if png_bytes.len() < 24 {
        return None;
    }
    let seq = LENS_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = std::env::temp_dir().join(format!("ferro_lens_{}_{}.png", std::process::id(), seq));
    fs::write(&tmp_path, png_bytes).ok()?;

    let form_arg = format!(
        "encoded_image=@{};type=image/png;filename=image.png",
        tmp_path.to_string_lossy()
    );

    let output = Command::new("curl")
        .args([
            "-s",
            "-i",
            "--max-time",
            "12",
            "-X",
            "POST",
            "https://lens.google.com/v3/upload?hl=ru",
            "-H",
            "User-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
            "-H",
            "Origin: https://lens.google.com",
            "-H",
            "Referer: https://lens.google.com/",
            "-F",
            &form_arg,
        ])
        .output();

    let _ = fs::remove_file(&tmp_path);

    let out = output.ok()?;
    let raw_headers = String::from_utf8_lossy(&out.stdout);
    for line in raw_headers.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("location:") {
            let loc = line["location:".len()..].trim();
            if loc.starts_with("https://") || loc.starts_with("http://") {
                return Some(loc.to_string());
            }
        }
    }
    None
}

/// Downloads an external image URL's raw bytes (works even on hotlink-protected images).
pub fn download_image_bytes(img_url: &str) -> Option<Vec<u8>> {
    if let Some(rest) = img_url.strip_prefix("data:") {
        return base64_decode(rest);
    }

    let seq = LENS_SEQ.fetch_add(1, Ordering::Relaxed);
    let dl_path = std::env::temp_dir().join(format!("ferro_lens_dl_{}_{}.img", std::process::id(), seq));

    let dl_ok = Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "8",
            "-H",
            "User-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
            "-o",
            dl_path.to_string_lossy().as_ref(),
            img_url,
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if dl_ok {
        if let Ok(bytes) = fs::read(&dl_path) {
            let _ = fs::remove_file(&dl_path);
            if bytes.len() >= 24 {
                return Some(bytes);
            }
        }
    }
    let _ = fs::remove_file(&dl_path);
    None
}

/// Renders an in-session Google Lens uploader document meant to be loaded via
/// `webview.load_html(&html, Some("https://lens.google.com/"))` so that:
/// 1) `window.origin` is `https://lens.google.com` (zero 403 error!)
/// 2) WebKit sends the Side Panel's exact Google Account cookies (`SID`/`SAPISID`) so
///    the resulting `vsrid` image token belongs to the logged-in user and never shows as broken!
pub fn render_lens_in_session_upload_html(b64_img: &str, fallback_url: &str) -> String {
    let clean_b64 = b64_img
        .split(',')
        .last()
        .unwrap_or(b64_img)
        .trim();
    let b64_json = serde_json::to_string(clean_b64).unwrap_or_else(|_| "\"\"".to_string());
    let fb_json = serde_json::to_string(fallback_url).unwrap_or_else(|_| "\"\"".to_string());

    let preview_tag = if clean_b64.len() > 24 {
        format!(
            r#"<div class="preview-box"><img src="data:image/png;base64,{}" alt="" /><div class="scan-line"></div></div>"#,
            clean_b64
        )
    } else {
        String::new()
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="ru">
<head>
  <meta charset="UTF-8" />
  <title>Google Объектив</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      background: #141417;
      color: #f2f2f5;
      font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
      height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 24px;
      gap: 18px;
      overflow: hidden;
    }}
    .preview-box {{
      position: relative;
      max-width: 280px;
      max-height: 240px;
      border-radius: 12px;
      overflow: hidden;
      border: 1px solid rgba(255,255,255,0.14);
      box-shadow: 0 12px 36px rgba(0,0,0,0.55);
    }}
    .preview-box img {{
      display: block;
      max-width: 280px;
      max-height: 240px;
      object-fit: contain;
      background: #0d0d10;
    }}
    @keyframes scanMove {{
      0% {{ top: 0%; opacity: 0.9; }}
      50% {{ top: 96%; opacity: 1; }}
      100% {{ top: 0%; opacity: 0.9; }}
    }}
    .scan-line {{
      position: absolute;
      left: 0;
      right: 0;
      height: 2px;
      background: linear-gradient(90deg, #4285f4, #a142f4, #34a853, #fbbc05);
      box-shadow: 0 0 12px rgba(66, 133, 244, 0.9);
      animation: scanMove 1.6s ease-in-out infinite;
    }}
    .status {{
      display: flex;
      align-items: center;
      gap: 10px;
      font-size: 13px;
      color: #d4d4dc;
      font-weight: 500;
    }}
    @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
    .spinner {{
      width: 16px;
      height: 16px;
      border: 2px solid rgba(255,255,255,0.18);
      border-top-color: #4285f4;
      border-radius: 50%;
      animation: spin 0.75s linear infinite;
    }}
    form {{ display: none; }}
  </style>
</head>
<body>
  {preview_tag}
  <div class="status">
    <div class="spinner"></div>
    <span>Поиск в Google Объективе...</span>
  </div>
  <form id="lensForm" method="POST" enctype="multipart/form-data" action="https://lens.google.com/v3/upload?hl=ru&re=df&ep=gsbubb">
    <input type="file" name="encoded_image" id="lensFile" />
  </form>
  <script>
    (function() {{
      const b64 = {b64_json};
      const fb = {fb_json};
      if (b64 && b64.length > 24) {{
        try {{
          const bin = atob(b64);
          const bytes = new Uint8Array(bin.length);
          for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
          let mime = 'image/png';
          let ext = 'png';
          if (bytes[0] === 0xFF && bytes[1] === 0xD8) {{ mime = 'image/jpeg'; ext = 'jpg'; }}
          else if (bytes[0] === 0x52 && bytes[1] === 0x49 && bytes[2] === 0x46 && bytes[3] === 0x46) {{ mime = 'image/webp'; ext = 'webp'; }}
          const file = new File([bytes], 'lens.' + ext, {{ type: mime }});
          const dt = new DataTransfer();
          dt.items.add(file);
          document.getElementById('lensFile').files = dt.files;
          setTimeout(() => document.getElementById('lensForm').submit(), 25);
          return;
        }} catch (e) {{}}
      }}
      if (fb && (fb.startsWith('http://') || fb.startsWith('https://'))) {{
        window.location.replace('https://lens.google.com/uploadbyurl?hl=ru&url=' + encodeURIComponent(fb));
      }}
    }})();
  </script>
</body>
</html>"##
    )
}

/// Sleek dark loading state displayed in the Side Panel for ~300ms while `curl` uploads the crop.
pub fn render_lens_loading_html(preview_b64: &str) -> String {
    let preview_tag = if !preview_b64.is_empty() {
        let src = if preview_b64.starts_with("data:") {
            preview_b64.to_string()
        } else {
            format!("data:image/png;base64,{}", preview_b64)
        };
        format!(
            r#"<div class="preview-box"><img src="{}" alt="" /><div class="scan-line"></div></div>"#,
            src
        )
    } else {
        String::new()
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="ru">
<head>
  <meta charset="UTF-8" />
  <title>Google Объектив</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      background: #141417;
      color: #f2f2f5;
      font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
      height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 24px;
      gap: 18px;
      overflow: hidden;
    }}
    .preview-box {{
      position: relative;
      max-width: 280px;
      max-height: 240px;
      border-radius: 12px;
      overflow: hidden;
      border: 1px solid rgba(255,255,255,0.14);
      box-shadow: 0 12px 36px rgba(0,0,0,0.55);
    }}
    .preview-box img {{
      display: block;
      max-width: 280px;
      max-height: 240px;
      object-fit: contain;
      background: #0d0d10;
    }}
    @keyframes scanMove {{
      0% {{ top: 0%; opacity: 0.9; }}
      50% {{ top: 96%; opacity: 1; }}
      100% {{ top: 0%; opacity: 0.9; }}
    }}
    .scan-line {{
      position: absolute;
      left: 0;
      right: 0;
      height: 2px;
      background: linear-gradient(90deg, #4285f4, #a142f4, #34a853, #fbbc05);
      box-shadow: 0 0 12px rgba(66, 133, 244, 0.9);
      animation: scanMove 1.6s ease-in-out infinite;
    }}
    .status {{
      display: flex;
      align-items: center;
      gap: 10px;
      font-size: 13px;
      color: #d4d4dc;
      font-weight: 500;
    }}
    @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
    .spinner {{
      width: 16px;
      height: 16px;
      border: 2px solid rgba(255,255,255,0.18);
      border-top-color: #4285f4;
      border-radius: 50%;
      animation: spin 0.75s linear infinite;
    }}
  </style>
</head>
<body>
  {preview_tag}
  <div class="status">
    <div class="spinner"></div>
    <span>Поиск в Google Объективе...</span>
  </div>
</body>
</html>"##
    )
}

/// Renders the 36px Chrome-style Side Panel header bar (Lens, Bookmarks, History — no Notes).
pub fn render_side_header_html() -> &'static str {
    r##"<!DOCTYPE html>
<html lang="ru" data-theme="dark">
<head>
  <meta charset="UTF-8" />
  <style>
    :root {
      --bg: #161618;
      --bg-hover: #252529;
      --border: #2a2a30;
      --fg: #ececef;
      --muted: #90909a;
      --accent: #3b82f6;
      --radius: 4px;
    }
    html[data-theme="light"] {
      --bg: #f1f3f4;
      --bg-hover: #e2e5e9;
      --border: #d0d4dc;
      --fg: #18181b;
      --muted: #5f6368;
      --accent: #2563eb;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; user-select: none; }
    body {
      height: 36px;
      background: var(--bg);
      color: var(--fg);
      border-bottom: 1px solid var(--border);
      border-left: 1px solid var(--border);
      font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 6px;
      gap: 4px;
      overflow: hidden;
    }
    .tabs {
      display: flex;
      align-items: center;
      gap: 3px;
      min-width: 0;
    }
    .tab-btn {
      height: 26px;
      padding: 0 9px;
      border: 1px solid transparent;
      border-radius: var(--radius);
      background: transparent;
      color: var(--muted);
      font-size: 11.5px;
      font-weight: 500;
      display: inline-flex;
      align-items: center;
      gap: 5px;
      cursor: pointer;
      white-space: nowrap;
      transition: background 0.12s, color 0.12s;
    }
    .tab-btn:hover {
      background: var(--bg-hover);
      color: var(--fg);
    }
    .tab-btn.active {
      background: var(--bg-hover);
      color: var(--fg);
      border-color: var(--accent);
      font-weight: 600;
    }
    .actions {
      display: flex;
      align-items: center;
      gap: 2px;
      flex-shrink: 0;
    }
    .icon-btn {
      width: 26px;
      height: 26px;
      border: none;
      border-radius: var(--radius);
      background: transparent;
      color: var(--muted);
      display: inline-flex;
      align-items: center;
      justify-content: center;
      cursor: pointer;
      transition: background 0.12s, color 0.12s;
    }
    .icon-btn:hover {
      background: var(--bg-hover);
      color: var(--fg);
    }
  </style>
</head>
<body>
  <div class="tabs">
    <button class="tab-btn active" data-mode="lens" title="Google Объектив">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none">
        <path d="M9 3H6a3 3 0 0 0-3 3v3" stroke="#4285F4" stroke-width="2.3" stroke-linecap="round"/>
        <path d="M15 3h3a3 3 0 0 1 3 3v3" stroke="#EA4335" stroke-width="2.3" stroke-linecap="round"/>
        <path d="M9 21H6a3 3 0 0 1-3-3v-3" stroke="#FBBC05" stroke-width="2.3" stroke-linecap="round"/>
        <circle cx="12" cy="12" r="3.4" fill="#4285F4"/>
        <circle cx="17.5" cy="17.5" r="2.2" fill="#34A853"/>
      </svg>
      <span>Объектив</span>
    </button>
    <button class="tab-btn" data-mode="bookmarks" title="Закладки">
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z"/></svg>
      <span>Закладки</span>
    </button>
    <button class="tab-btn" data-mode="history" title="История">
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
      <span>История</span>
    </button>
  </div>
  <div class="actions">
    <button class="icon-btn" id="btnReselectLens" title="Выделить новую область на странице">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/><rect x="8" y="8" width="8" height="8" rx="1"/></svg>
    </button>
    <button class="icon-btn" id="btnPopout" title="Открыть в новой вкладке">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>
    </button>
    <button class="icon-btn" id="btnCloseSide" title="Закрыть боковую панель">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
    </button>
  </div>

  <script>
    function sendIpc(p) {
      if (window.ipc) window.ipc.postMessage(JSON.stringify(p));
    }

    window.setSideMode = function(mode) {
      document.querySelectorAll('.tab-btn').forEach(b => {
        b.classList.toggle('active', b.dataset.mode === mode);
      });
      document.getElementById('btnReselectLens').style.display = mode === 'lens' ? 'inline-flex' : 'none';
    };

    document.querySelectorAll('.tab-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const mode = btn.dataset.mode;
        window.setSideMode(mode);
        sendIpc({ action: 'open_side_panel', url: mode });
      });
    });

    document.getElementById('btnReselectLens').addEventListener('click', () => {
      sendIpc({ action: 'open_lens_overlay' });
    });

    document.getElementById('btnPopout').addEventListener('click', () => {
      sendIpc({ action: 'side_popout' });
    });

    document.getElementById('btnCloseSide').addEventListener('click', () => {
      sendIpc({ action: 'close_side_panel' });
    });

    window.applySettings = function(s) {
      if (!s) return;
      document.documentElement.setAttribute('data-theme', s.theme_mode === 'light' ? 'light' : 'dark');
      if (s.accent_color && s.accent_color.toLowerCase() !== '#ececec') {
        document.documentElement.style.setProperty('--accent', s.accent_color);
      } else {
        document.documentElement.style.setProperty('--accent', '#3b82f6');
      }
      if (s.corner_radius !== undefined) document.documentElement.style.setProperty('--radius', Math.min(10, s.corner_radius) + 'px');
    };
  </script>
</body>
</html>"##
}

pub fn render_bookmarks_panel_html(bookmarks: &BookmarksData) -> String {
    let items_json = serde_json::to_string(&bookmarks.items).unwrap_or_else(|_| "[]".to_string());
    format!(
        r##"<!DOCTYPE html>
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
      listEl.innerHTML = flt.map(b => {{
        let host = '';
        try {{ host = new URL(b.url).hostname; }} catch(e) {{}}
        const ico = host ? `https://www.google.com/s2/favicons?domain=${{encodeURIComponent(host)}}&sz=32` : '';
        return `<div class="row" data-url="${{encodeURIComponent(b.url)}}">
          ${{ico ? `<img src="${{ico}}" loading="lazy" decoding="async" alt="" onerror="this.style.display='none'"/>` : ''}}
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
    if (window._ferro_settings && window._ferro_settings.theme_mode) {{
      document.documentElement.setAttribute('data-theme', window._ferro_settings.theme_mode === 'light' ? 'light' : 'dark');
    }}
    render();
  </script>
</body>
</html>"##
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
      background: #3b82f6;
      color: #ffffff;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_roundtrip() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"FerroLens"), "RmVycm9MZW5z");
        let raw = b"\x89PNG\r\n\x1a\n\x00\x01\x02\xffFerro";
        let enc = base64_encode(raw);
        let dec = base64_decode(&format!("data:image/png;base64,{}", enc)).unwrap();
        assert_eq!(dec, raw);
    }
}
