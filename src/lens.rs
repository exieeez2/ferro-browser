/// Google Lens & Chrome-style Side Panel support for Ferro Browser.

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

/// Renders the auto-submitting `ferro://lens-upload` page inside the Side Panel WebView.
/// Submits raw PNG bytes as `multipart/form-data` (`encoded_image`) directly to
/// `https://lens.google.com/v3/upload?hl=ru` using the user's WebKit session & cookies.
pub fn render_lens_upload_html(b64_png: &str, fallback_url: &str) -> String {
    let b64_json = serde_json::to_string(b64_png).unwrap_or_else(|_| "\"\"".to_string());
    let url_json = serde_json::to_string(fallback_url).unwrap_or_else(|_| "\"\"".to_string());

    format!(
        r#"<!DOCTYPE html>
<html lang="ru">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
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
      display: none;
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
      animation: scanMove 1.8s ease-in-out infinite;
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
  <div class="preview-box" id="previewBox">
    <img id="previewImg" alt="Lens selection" />
    <div class="scan-line"></div>
  </div>

  <div class="status">
    <div class="spinner"></div>
    <span id="statusTxt">Анализ изображения в Google Объективе...</span>
  </div>

  <form id="lensForm" method="POST" enctype="multipart/form-data" action="https://lens.google.com/v3/upload?hl=ru&re=df&ep=ccm">
    <input type="file" name="encoded_image" id="fileInput" />
  </form>

  <script>
    (function() {{
      const rawB64 = {b64_json};
      const fallbackUrl = {url_json};

      function submitBase64Png(b64Data) {{
        const clean = b64Data.includes(',') ? b64Data.split(',')[1] : b64Data;
        const previewImg = document.getElementById('previewImg');
        const previewBox = document.getElementById('previewBox');
        previewImg.src = 'data:image/png;base64,' + clean;
        previewBox.style.display = 'block';

        const bin = atob(clean);
        const bytes = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) {{
          bytes[i] = bin.charCodeAt(i);
        }}
        const file = new File([bytes], 'selection.png', {{ type: 'image/png' }});
        const dt = new DataTransfer();
        dt.items.add(file);
        const inp = document.getElementById('fileInput');
        inp.files = dt.files;
        setTimeout(() => {{
          document.getElementById('lensForm').submit();
        }}, 60);
      }}

      if (rawB64 && rawB64.length > 16) {{
        try {{
          submitBase64Png(rawB64);
          return;
        }} catch (e) {{}}
      }}

      if (fallbackUrl) {{
        if (fallbackUrl.startsWith('data:image/')) {{
          try {{
            submitBase64Png(fallbackUrl);
            return;
          }} catch (e) {{}}
        }}
        window.location.replace('https://lens.google.com/uploadbyurl?hl=ru&url=' + encodeURIComponent(fallbackUrl));
      }} else {{
        document.getElementById('statusTxt').textContent = 'Выберите область на странице для поиска';
      }}
    }})();
  </script>
</body>
</html>"#
    )
}

/// Renders the 36px Chrome-style Side Panel header bar.
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
      --accent: #ececec;
      --radius: 4px;
    }
    html[data-theme="light"] {
      --bg: #f1f3f4;
      --bg-hover: #e2e5e9;
      --border: #d0d4dc;
      --fg: #18181b;
      --muted: #5f6368;
      --accent: #18181b;
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
      gap: 2px;
      min-width: 0;
      overflow-x: auto;
      scrollbar-width: none;
    }
    .tabs::-webkit-scrollbar { display: none; }
    .tab-btn {
      height: 26px;
      padding: 0 8px;
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
      border-color: var(--border);
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
    <button class="tab-btn" data-mode="notes" title="Заметки">
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
      <span>Заметки</span>
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
      if (s.accent_color) document.documentElement.style.setProperty('--accent', s.accent_color);
      if (s.corner_radius !== undefined) document.documentElement.style.setProperty('--radius', Math.min(10, s.corner_radius) + 'px');
    };
  </script>
</body>
</html>"##
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"FerroLens"), "RmVycm9MZW5z");
    }
}
