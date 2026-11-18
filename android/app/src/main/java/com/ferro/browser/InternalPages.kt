package com.ferro.browser

import android.content.Context
import android.net.Uri
import android.webkit.WebResourceResponse
import org.json.JSONObject
import java.io.ByteArrayInputStream

object InternalPages {

    @Volatile
    var pendingLensB64: String = ""

    @Volatile
    var pendingLensUrl: String = ""

    fun serve(context: Context, uri: Uri, storage: BrowserStorage): WebResourceResponse {
        val path = uri.path?.trimStart('/') ?: "newtab"

        if (path == "logo.png") {
            return try {
                val stream = context.assets.open("logo.png")
                WebResourceResponse("image/png", null, stream)
            } catch (_: Exception) {
                htmlResponse("")
            }
        }

        val html = when {
            path.startsWith("settings") -> renderSettings(storage)
            path.startsWith("passwords") -> renderPasswords(storage)
            path.startsWith("history") -> renderHistory(storage)
            path.startsWith("downloads") -> renderDownloads(storage)
            path.startsWith("lens-upload") -> renderLensUpload(pendingLensB64)
            else -> renderNewTab(storage)
        }
        return htmlResponse(html)
    }

    private fun htmlResponse(html: String): WebResourceResponse = WebResourceResponse(
        "text/html",
        "UTF-8",
        200,
        "OK",
        mapOf("Cache-Control" to "no-store"),
        ByteArrayInputStream(html.toByteArray(Charsets.UTF_8))
    )

    private fun baseCss(s: MobileSettings): String {
        val isLight = s.themeMode == "light"
        val bg = if (isLight) "#f5f6f8" else "#111111"
        val surface = if (isLight) "#ffffff" else "#18181b"
        val border = if (isLight) "#d6d9e0" else "#2a2a30"
        val text = if (isLight) "#18181b" else "#ececef"
        val muted = if (isLight) "#686c78" else "#8f8f99"
        val rawAcc = s.accentColor.ifBlank { "#ececec" }
        val effAcc = if (isLight && rawAcc.equals("#ececec", true)) "#18181b" else rawAcc
        val accFg = if (effAcc.equals("#ececec", true)) "#111111" else "#ffffff"
        val rad = s.cornerRadius.coerceIn(0, 20)

        return """
            :root {
                --bg: $bg;
                --surface: $surface;
                --border: $border;
                --text: $text;
                --muted: $muted;
                --accent: $effAcc;
                --accent-fg: $accFg;
                --radius: ${rad}px;
            }
            * {
                box-sizing: border-box;
                margin: 0;
                padding: 0;
                -webkit-tap-highlight-color: transparent;
            }
            html, body {
                min-height: 100dvh;
                background: var(--bg);
                color: var(--text);
                font-family: -apple-system, BlinkMacSystemFont, 'Roboto', sans-serif;
                -webkit-text-size-adjust: 100%;
                overscroll-behavior-y: contain;
            }
            input, textarea, select {
                font-size: 16px;
                color: var(--text);
                background: var(--surface);
                border: 1px solid var(--border);
                border-radius: var(--radius);
                outline: none;
            }
            button, a, [role="button"] {
                touch-action: manipulation;
                user-select: none;
                -webkit-user-select: none;
                transition: transform 110ms ease-out, opacity 110ms ease-out;
            }
            button:active, .tile:active, .card-btn:active {
                transform: scale(0.96);
            }
        """.trimIndent()
    }

    private fun renderNewTab(storage: BrowserStorage): String {
        val s = storage.settings
        val bmJson = storage.bookmarksJson().toString()
        val settingsJson = s.toJson().toString()

        return """
        <!DOCTYPE html>
        <html lang="ru">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content" />
            <title>Новая вкладка</title>
            <style>
                ${baseCss(s)}
                body {
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                    padding: 12vh 20px 40px;
                    position: relative;
                    overflow-x: hidden;
                }
                .wp-layer {
                    position: fixed;
                    inset: 0;
                    z-index: 0;
                    background-size: cover;
                    background-position: center;
                    pointer-events: none;
                }
                .main {
                    position: relative;
                    z-index: 1;
                    width: 100%;
                    max-width: 480px;
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                }
                .brand {
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                    gap: 10px;
                    margin-bottom: 28px;
                }
                .brand-box {
                    width: 56px;
                    height: 56px;
                    border-radius: calc(var(--radius) + 2px);
                    overflow: hidden;
                    border: 1px solid var(--border);
                    background: #111111;
                    box-shadow: 0 8px 24px rgba(0,0,0,0.35);
                }
                .brand-box img { width: 100%; height: 100%; object-fit: cover; }
                .brand-name {
                    font-size: 13px;
                    font-weight: 700;
                    letter-spacing: 0.34em;
                    padding-left: 0.34em;
                    opacity: 0.9;
                }
                .search-bar {
                    width: 100%;
                    height: 50px;
                    background: var(--surface);
                    border: 1px solid var(--border);
                    border-radius: calc(var(--radius) + 4px);
                    display: flex;
                    align-items: center;
                    padding: 0 8px;
                    gap: 8px;
                    box-shadow: 0 6px 20px rgba(0,0,0,0.18);
                    position: relative;
                }
                .eng-pill {
                    height: 36px;
                    padding: 0 10px;
                    border-radius: var(--radius);
                    border: 1px solid var(--border);
                    background: rgba(127,127,127,0.08);
                    color: var(--text);
                    display: inline-flex;
                    align-items: center;
                    gap: 6px;
                    cursor: pointer;
                    flex-shrink: 0;
                }
                .search-bar input {
                    flex: 1;
                    min-width: 0;
                    border: none;
                    background: transparent;
                    height: 100%;
                    font-size: 16px;
                }
                .lens-btn {
                    width: 38px;
                    height: 38px;
                    border: none;
                    background: transparent;
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    border-radius: var(--radius);
                }
                .eng-menu {
                    width: 100%;
                    margin-top: 8px;
                    background: var(--surface);
                    border: 1px solid var(--border);
                    border-radius: var(--radius);
                    padding: 6px;
                    display: none;
                    grid-template-columns: repeat(2, 1fr);
                    gap: 6px;
                }
                .eng-menu.open { display: grid; }
                .eng-opt {
                    height: 38px;
                    padding: 0 10px;
                    border: 1px solid transparent;
                    border-radius: calc(var(--radius) - 2px);
                    background: transparent;
                    color: var(--text);
                    font-size: 13px;
                    display: flex;
                    align-items: center;
                    gap: 8px;
                }
                .eng-opt.active { border-color: var(--accent); background: rgba(127,127,127,0.12); }
                .grid {
                    width: 100%;
                    margin-top: 28px;
                    display: grid;
                    grid-template-columns: repeat(4, 1fr);
                    gap: 14px 10px;
                }
                .tile {
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                    gap: 7px;
                    text-decoration: none;
                    color: var(--text);
                }
                .tile-ico {
                    width: 54px;
                    height: 54px;
                    background: var(--surface);
                    border: 1px solid var(--border);
                    border-radius: calc(var(--radius) + 2px);
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    font-size: 18px;
                    font-weight: 600;
                    box-shadow: 0 4px 12px rgba(0,0,0,0.14);
                }
                .tile-ico img { width: 24px; height: 24px; border-radius: 4px; }
                .tile-lbl {
                    font-size: 11.5px;
                    color: var(--muted);
                    max-width: 74px;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                    text-align: center;
                }
            </style>
        </head>
        <body>
            <div class="wp-layer" id="wpLayer"></div>
            <div class="main">
                <div class="brand">
                    <div class="brand-box"><img src="/logo.png" alt="Ferro" /></div>
                    <div class="brand-name">FERRO</div>
                </div>

                <form class="search-bar" id="searchForm">
                    <button type="button" class="eng-pill" id="engPill">
                        <span id="engIcon"></span>
                        <span style="font-size:10px;opacity:0.65;">▾</span>
                    </button>
                    <input type="search" id="qInp" enterkeyhint="search" placeholder="Поиск или адрес сайта..." autocomplete="off" />
                    <button type="button" class="lens-btn" id="ntLensBtn" title="Google Объектив">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
                            <path d="M9 3H6a3 3 0 0 0-3 3v3" stroke="#4285F4" stroke-width="2.2" stroke-linecap="round"/>
                            <path d="M15 3h3a3 3 0 0 1 3 3v3" stroke="#EA4335" stroke-width="2.2" stroke-linecap="round"/>
                            <path d="M9 21H6a3 3 0 0 1-3-3v-3" stroke="#FBBC05" stroke-width="2.2" stroke-linecap="round"/>
                            <circle cx="12" cy="12" r="3.4" fill="#4285F4"/>
                            <circle cx="17.5" cy="17.5" r="2.2" fill="#34A853"/>
                        </svg>
                    </button>
                </form>

                <div class="eng-menu" id="engMenu"></div>
                <div class="grid" id="speedGrid"></div>
            </div>

            <script>
                const cfg = $settingsJson;
                const bookmarks = $bmJson;

                const ENGINES = {
                    google: { name: 'Google', url: 'https://www.google.com/search?q=', badge: '#4285F4', ch: 'G' },
                    duckduckgo: { name: 'DuckDuckGo', url: 'https://duckduckgo.com/?q=', badge: '#DE5833', ch: 'D' },
                    yandex: { name: 'Яндекс', url: 'https://ya.ru/search/?text=', badge: '#FC3F1D', ch: 'Я' },
                    bing: { name: 'Bing', url: 'https://www.bing.com/search?q=', badge: '#00809D', ch: 'B' },
                    brave: { name: 'Brave', url: 'https://search.brave.com/search?q=', badge: '#FB542B', ch: 'V' },
                    youtube: { name: 'YouTube', url: 'https://m.youtube.com/results?search_query=', badge: '#FF0000', ch: '▶' },
                    github: { name: 'GitHub', url: 'https://github.com/search?q=', badge: '#24292F', ch: 'GH' }
                };

                const WP_PRESETS = {
                    obsidian_mesh: 'radial-gradient(at 20% 20%, #27273a 0%, #111115 65%)',
                    aurora_borealis: 'linear-gradient(135deg, #06283d 0%, #1363df 48%, #112b3c 100%)',
                    crimson_dusk: 'linear-gradient(135deg, #2d0b1e 0%, #7a1c3a 52%, #141118 100%)'
                };

                const wpEl = document.getElementById('wpLayer');
                if (cfg.wallpaper_url) {
                    wpEl.style.backgroundImage = 'url(' + cfg.wallpaper_url + ')';
                    wpEl.style.filter = `blur(${'$'}{cfg.wallpaper_blur || 0}px) brightness(${'$'}{(cfg.wallpaper_brightness || 80) / 100})`;
                } else if (WP_PRESETS[cfg.wallpaper_preset]) {
                    wpEl.style.background = WP_PRESETS[cfg.wallpaper_preset];
                }

                function sendIpc(obj) {
                    if (window.FerroBridge) window.FerroBridge.postMessage(JSON.stringify(obj));
                }

                function badgeHtml(e) {
                    return `<span style="width:20px;height:20px;border-radius:50%;background:${'$'}{e.badge};color:#fff;font-size:11px;font-weight:700;display:inline-flex;align-items:center;justify-content:center;">${'$'}{e.ch}</span>`;
                }

                function renderEngines() {
                    const curKey = cfg.search_engine || 'google';
                    const cur = ENGINES[curKey] || ENGINES.google;
                    document.getElementById('engIcon').innerHTML = badgeHtml(cur);
                    document.getElementById('qInp').placeholder = 'Поиск в ' + cur.name + ' или адрес...';

                    const menu = document.getElementById('engMenu');
                    menu.innerHTML = Object.entries(ENGINES).map(([k, v]) =>
                        `<button type="button" class="eng-opt ${'$'}{k === curKey ? 'active' : ''}" data-k="${'$'}{k}">${'$'}{badgeHtml(v)}<span>${'$'}{v.name}</span></button>`
                    ).join('');

                    menu.querySelectorAll('.eng-opt').forEach(b => {
                        b.onclick = () => {
                            cfg.search_engine = b.dataset.k;
                            menu.classList.remove('open');
                            renderEngines();
                            sendIpc({ action: 'save_settings', settings: cfg });
                        };
                    });
                }

                document.getElementById('engPill').onclick = () => {
                    document.getElementById('engMenu').classList.toggle('open');
                };

                document.getElementById('ntLensBtn').onclick = () => {
                    sendIpc({ action: 'open_image_lens_picker' });
                };

                document.getElementById('searchForm').onsubmit = (e) => {
                    e.preventDefault();
                    const q = document.getElementById('qInp').value.trim();
                    if (!q) return;
                    const cur = ENGINES[cfg.search_engine] || ENGINES.google;
                    let target = q;
                    if (q.startsWith('ferro://')) {
                        target = 'https://ferro.local/' + q.slice(8);
                    } else if (!q.startsWith('http://') && !q.startsWith('https://')) {
                        const isDom = /^[a-zA-Z0-9-]+(\.[a-zA-Z0-9-]+)+(\/.*)?$/.test(q);
                        target = isDom ? ('https://' + q) : (cur.url + encodeURIComponent(q));
                    }
                    window.location.href = target;
                };

                const grid = document.getElementById('speedGrid');
                grid.innerHTML = bookmarks.slice(0, 12).map(b => {
                    let ico = `<span>${'$'}{(b.title || 'F')[0].toUpperCase()}</span>`;
                    if (!b.url.includes('ferro.local')) {
                        try {
                            const h = new URL(b.url).hostname;
                            ico = `<img src="https://www.google.com/s2/favicons?domain=${'$'}{encodeURIComponent(h)}&sz=64" alt="" onerror="this.remove()" />`;
                        } catch (_) {}
                    }
                    const safeTitle = (b.title || '').replace(/</g, '&lt;');
                    return `<a class="tile" href="${'$'}{b.url}"><div class="tile-ico">${'$'}{ico}</div><span class="tile-lbl">${'$'}{safeTitle}</span></a>`;
                }).join('');

                renderEngines();
            </script>
        </body>
        </html>
        """.trimIndent()
    }

    private fun renderSettings(storage: BrowserStorage): String {
        val s = storage.settings
        val sJson = s.toJson().toString()
        val blocked = AdBlockEngine.blockedCount.get()

        return """
        <!DOCTYPE html>
        <html lang="ru">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
            <title>Настройки — Ferro</title>
            <style>
                ${baseCss(s)}
                body { padding: 20px 16px 64px; max-width: 600px; margin: 0 auto; }
                h1 { font-size: 20px; font-weight: 700; margin-bottom: 18px; }
                .sec-title { font-size: 12px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.06em; margin: 20px 0 8px; }
                .card { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
                .row { padding: 14px; border-bottom: 1px solid var(--border); display: flex; align-items: center; justify-content: space-between; gap: 12px; }
                .row:last-child { border-bottom: none; }
                .lbl { font-size: 14.5px; font-weight: 500; }
                .sub { font-size: 12px; color: var(--muted); margin-top: 2px; }
                .swatches { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 8px; }
                .sw { width: 32px; height: 32px; border-radius: 50%; border: 2px solid transparent; }
                .sw.active { border-color: var(--text); transform: scale(1.1); }
                select, input[type="url"] { height: 38px; padding: 0 10px; }
                .btn { height: 38px; padding: 0 14px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 13.5px; font-weight: 500; }
                .btn-danger { color: #ef4444; border-color: rgba(239,68,68,0.35); }
            </style>
        </head>
        <body>
            <h1>Настройки Ferro</h1>

            <div class="sec-title">Внешний вид и панель</div>
            <div class="card">
                <div class="row">
                    <div><div class="lbl">Тема оформления</div><div class="sub">Тёмная Obsidian или Светлая</div></div>
                    <select id="themeSel">
                        <option value="dark">Тёмная</option>
                        <option value="light">Светлая</option>
                    </select>
                </div>
                <div class="row">
                    <div><div class="lbl">Положение адресной строки</div><div class="sub">Удобное управление одной рукой снизу</div></div>
                    <select id="barPosSel">
                        <option value="bottom">Снизу экрана</option>
                        <option value="top">Сверху экрана</option>
                    </select>
                </div>
                <div class="row" style="flex-direction:column;align-items:flex-start;">
                    <div class="lbl">Акцентный цвет (Material You)</div>
                    <div class="swatches" id="swatches"></div>
                </div>
                <div class="row">
                    <div><div class="lbl">Скругление углов</div><div class="sub" id="radVal">10 px</div></div>
                    <input type="range" id="radRange" min="0" max="20" style="width:130px;" />
                </div>
                <div class="row">
                    <div><div class="lbl">Фон новой вкладки</div></div>
                    <select id="wpSel">
                        <option value="none">Чистый тон</option>
                        <option value="obsidian_mesh">Графит</option>
                        <option value="aurora_borealis">Сияние</option>
                        <option value="crimson_dusk">Закат</option>
                    </select>
                </div>
            </div>

            <div class="sec-title">Поиск, защита и пароли</div>
            <div class="card">
                <div class="row">
                    <div class="lbl">Поисковая система</div>
                    <select id="engSel">
                        <option value="google">Google</option>
                        <option value="duckduckgo">DuckDuckGo</option>
                        <option value="yandex">Яндекс</option>
                        <option value="bing">Bing</option>
                        <option value="brave">Brave</option>
                    </select>
                </div>
                <div class="row">
                    <div><div class="lbl">Блокировщик рекламы</div><div class="sub">Заблокировано запросов: $blocked</div></div>
                    <input type="checkbox" id="abChk" style="width:22px;height:22px;" />
                </div>
                <div class="row">
                    <div><div class="lbl">Предлагать сохранять пароли</div><div class="sub">Автозаполнение и сохранение входов</div></div>
                    <input type="checkbox" id="pwdChk" style="width:22px;height:22px;" />
                </div>
            </div>

            <div class="sec-title">Данные браузера</div>
            <div class="card">
                <div class="row">
                    <div class="lbl">Менеджер паролей</div>
                    <a href="/passwords"><button class="btn">Открыть</button></a>
                </div>
                <div class="row">
                    <div class="lbl">Очистить кэш, историю и куки</div>
                    <button class="btn btn-danger" id="clearAllBtn">Очистить</button>
                </div>
            </div>

            <script>
                const cfg = $sJson;
                const COLORS = ['#ececec', '#60a5fa', '#34d399', '#a78bfa', '#f472b6', '#fb923c', '#facc15'];

                function save() {
                    if (window.FerroBridge) {
                        window.FerroBridge.postMessage(JSON.stringify({ action: 'save_settings', settings: cfg }));
                    }
                    setTimeout(() => window.location.reload(), 120);
                }

                const themeSel = document.getElementById('themeSel');
                themeSel.value = cfg.theme_mode || 'dark';
                themeSel.onchange = () => { cfg.theme_mode = themeSel.value; save(); };

                const barPosSel = document.getElementById('barPosSel');
                barPosSel.value = cfg.bar_position || 'bottom';
                barPosSel.onchange = () => { cfg.bar_position = barPosSel.value; save(); };

                const wpSel = document.getElementById('wpSel');
                wpSel.value = cfg.wallpaper_preset || 'none';
                wpSel.onchange = () => { cfg.wallpaper_preset = wpSel.value; save(); };

                const engSel = document.getElementById('engSel');
                engSel.value = cfg.search_engine || 'google';
                engSel.onchange = () => { cfg.search_engine = engSel.value; save(); };

                const radRange = document.getElementById('radRange');
                radRange.value = cfg.corner_radius ?? 10;
                document.getElementById('radVal').textContent = radRange.value + ' px';
                radRange.onchange = () => { cfg.corner_radius = Number(radRange.value); save(); };

                const abChk = document.getElementById('abChk');
                abChk.checked = cfg.adblock_enabled !== false;
                abChk.onchange = () => { cfg.adblock_enabled = abChk.checked; save(); };

                const pwdChk = document.getElementById('pwdChk');
                pwdChk.checked = cfg.offer_save_passwords !== false;
                pwdChk.onchange = () => { cfg.offer_save_passwords = pwdChk.checked; save(); };

                const swWrap = document.getElementById('swatches');
                swWrap.innerHTML = COLORS.map(c => `<button class="sw ${'$'}{(cfg.accent_color || '').toLowerCase() === c ? 'active' : ''}" style="background:${'$'}{c}" data-c="${'$'}{c}"></button>`).join('');
                swWrap.querySelectorAll('.sw').forEach(b => b.onclick = () => { cfg.accent_color = b.dataset.c; save(); });

                document.getElementById('clearAllBtn').onclick = () => {
                    if (window.FerroBridge) window.FerroBridge.postMessage(JSON.stringify({ action: 'clear_browsing_data' }));
                };
            </script>
        </body>
        </html>
        """.trimIndent()
    }

    private fun renderPasswords(storage: BrowserStorage): String {
        val s = storage.settings
        val itemsJson = storage.passwordsJson().toString()
        return """
        <!DOCTYPE html>
        <html lang="ru">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
            <title>Пароли — Ferro</title>
            <style>
                ${baseCss(s)}
                body { padding: 18px 14px 70px; max-width: 600px; margin: 0 auto; }
                .head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; gap: 8px; flex-wrap: wrap; }
                h1 { font-size: 19px; font-weight: 700; }
                .btn { height: 36px; padding: 0 12px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 13px; font-weight: 500; }
                .btn-acc { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); font-weight: 600; }
                .search { width: 100%; height: 42px; padding: 0 12px; margin-bottom: 12px; }
                .form-box { display: none; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 14px; margin-bottom: 14px; gap: 10px; flex-direction: column; }
                .form-box.open { display: flex; }
                .form-box input { height: 40px; padding: 0 10px; width: 100%; }
                .item { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 12px; margin-bottom: 10px; }
                .site { font-size: 14px; font-weight: 600; word-break: break-all; }
                .usr { font-size: 12.5px; color: var(--muted); margin: 4px 0 10px; }
                .actions { display: flex; gap: 6px; flex-wrap: wrap; }
            </style>
        </head>
        <body>
            <div class="head">
                <h1>Пароли</h1>
                <div style="display:flex;gap:6px;">
                    <button class="btn btn-acc" id="addBtn">+ Добавить</button>
                    <button class="btn" id="genBtn">Генератор</button>
                </div>
            </div>
            <input type="search" class="search" id="q" placeholder="Поиск сайта или логина..." />

            <form class="form-box" id="addForm">
                <input id="fOrig" type="text" placeholder="Сайт (например github.com)" required />
                <input id="fUser" type="text" placeholder="Логин или Email" />
                <input id="fPass" type="text" placeholder="Пароль" required />
                <div style="display:flex;gap:8px;justify-content:flex-end;">
                    <button type="button" class="btn" id="fGen">Сгенерировать</button>
                    <button type="submit" class="btn btn-acc">Сохранить</button>
                </div>
            </form>

            <div id="list"></div>

            <script>
                let items = $itemsJson;
                const listEl = document.getElementById('list');

                function send(p) { if (window.FerroBridge) window.FerroBridge.postMessage(JSON.stringify(p)); }

                function genPass() {
                    const ch = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#${'$'}%&*';
                    const arr = new Uint32Array(18);
                    crypto.getRandomValues(arr);
                    return Array.from(arr, x => ch[x % ch.length]).join('');
                }

                function render() {
                    const q = document.getElementById('q').value.trim().toLowerCase();
                    const flt = items.filter(i => !q || (i.origin || '').toLowerCase().includes(q) || (i.username || '').toLowerCase().includes(q));
                    if (!flt.length) {
                        listEl.innerHTML = '<div style="text-align:center;color:var(--muted);padding:36px 0;font-size:13.5px;">Сохранённых паролей пока нет</div>';
                        return;
                    }
                    listEl.innerHTML = flt.map(it => `
                        <div class="item">
                            <div class="site">${'$'}{(it.origin || '').replace(/</g, '&lt;')}</div>
                            <div class="usr">${'$'}{(it.username || '(без логина)').replace(/</g, '&lt;')}</div>
                            <div class="actions">
                                <button class="btn" onclick="cp('${'$'}{encodeURIComponent(it.username || '')}', 'Логин скопирован')">Логин</button>
                                <button class="btn" onclick="cp('${'$'}{encodeURIComponent(it.password || '')}', 'Пароль скопирован')">Пароль</button>
                                <button class="btn" style="color:#ef4444" onclick="del(${'$'}{it.id})">Удалить</button>
                            </div>
                        </div>
                    `).join('');
                }

                window.cp = function(enc, msg) {
                    send({ action: 'copy_text', text: decodeURIComponent(enc), toast: msg });
                };

                window.del = function(id) {
                    items = items.filter(i => i.id !== id);
                    send({ action: 'delete_password', id: id });
                    render();
                };

                document.getElementById('addBtn').onclick = () => document.getElementById('addForm').classList.toggle('open');
                document.getElementById('fGen').onclick = () => document.getElementById('fPass').value = genPass();
                document.getElementById('genBtn').onclick = () => {
                    const p = genPass();
                    send({ action: 'copy_text', text: p, toast: 'Сгенерирован и скопирован: ' + p });
                };

                document.getElementById('addForm').onsubmit = (e) => {
                    e.preventDefault();
                    const origin = document.getElementById('fOrig').value.trim();
                    const username = document.getElementById('fUser').value.trim();
                    const password = document.getElementById('fPass').value;
                    if (!origin || !password) return;
                    send({ action: 'save_password', origin, username, password });
                    setTimeout(() => window.location.reload(), 120);
                };

                document.getElementById('q').oninput = render;
                render();
            </script>
        </body>
        </html>
        """.trimIndent()
    }

    private fun renderHistory(storage: BrowserStorage): String {
        val s = storage.settings
        val hJson = storage.historyJson().toString()
        return """
        <!DOCTYPE html>
        <html lang="ru">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
            <title>История — Ferro</title>
            <style>
                ${baseCss(s)}
                body { padding: 18px 14px 64px; max-width: 600px; margin: 0 auto; }
                .head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; }
                h1 { font-size: 19px; font-weight: 700; }
                .btn { height: 36px; padding: 0 12px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--surface); color: #ef4444; font-size: 13px; }
                .search { width: 100%; height: 42px; padding: 0 12px; margin-bottom: 12px; }
                .row { display: block; text-decoration: none; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 11px 12px; margin-bottom: 8px; }
                .t { font-size: 14px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
                .u { font-size: 12px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 2px; }
            </style>
        </head>
        <body>
            <div class="head">
                <h1>История</h1>
                <button class="btn" id="clrBtn">Очистить</button>
            </div>
            <input type="search" class="search" id="q" placeholder="Поиск по истории..." />
            <div id="list"></div>
            <script>
                let items = $hJson;
                const listEl = document.getElementById('list');
                function render() {
                    const q = document.getElementById('q').value.trim().toLowerCase();
                    const flt = items.filter(i => !q || (i.title || '').toLowerCase().includes(q) || (i.url || '').toLowerCase().includes(q));
                    listEl.innerHTML = flt.length ? flt.map(i => `<a class="row" href="${'$'}{i.url}"><div class="t">${'$'}{(i.title || i.url).replace(/</g, '&lt;')}</div><div class="u">${'$'}{(i.url || '').replace(/</g, '&lt;')}</div></a>`).join('') : '<div style="text-align:center;color:var(--muted);padding:36px 0;">История пуста</div>';
                }
                document.getElementById('clrBtn').onclick = () => {
                    items = [];
                    if (window.FerroBridge) window.FerroBridge.postMessage(JSON.stringify({ action: 'clear_history' }));
                    render();
                };
                document.getElementById('q').oninput = render;
                render();
            </script>
        </body>
        </html>
        """.trimIndent()
    }

    private fun renderDownloads(storage: BrowserStorage): String {
        val s = storage.settings
        val dJson = storage.downloadsJson().toString()
        return """
        <!DOCTYPE html>
        <html lang="ru">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
            <title>Загрузки — Ferro</title>
            <style>
                ${baseCss(s)}
                body { padding: 18px 14px 64px; max-width: 600px; margin: 0 auto; }
                .head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; }
                h1 { font-size: 19px; font-weight: 700; }
                .btn { height: 36px; padding: 0 12px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 13px; }
                .row { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 12px; margin-bottom: 8px; }
                .t { font-size: 14px; font-weight: 600; word-break: break-all; }
                .u { font-size: 12px; color: var(--muted); word-break: break-all; margin-top: 3px; }
            </style>
        </head>
        <body>
            <div class="head">
                <h1>Загрузки</h1>
                <button class="btn" id="clrBtn">Очистить список</button>
            </div>
            <div id="list"></div>
            <script>
                let items = $dJson;
                const listEl = document.getElementById('list');
                function render() {
                    listEl.innerHTML = items.length ? items.map(i => `<div class="row"><div class="t">${'$'}{(i.filename || '').replace(/</g, '&lt;')}</div><div class="u">Сохранено в Downloads • ${'$'}{(i.url || '').replace(/</g, '&lt;')}</div></div>`).join('') : '<div style="text-align:center;color:var(--muted);padding:36px 0;">Список загрузок пуст</div>';
                }
                document.getElementById('clrBtn').onclick = () => {
                    items = [];
                    if (window.FerroBridge) window.FerroBridge.postMessage(JSON.stringify({ action: 'clear_downloads' }));
                    render();
                };
                render();
            </script>
        </body>
        </html>
        """.trimIndent()
    }

    private fun renderLensUpload(b64Png: String): String {
        val clean = if (b64Png.contains(",")) b64Png.substringAfter(",") else b64Png.trim()
        val imgHtml = if (clean.length > 20) {
            """<img src="data:image/png;base64,$clean" alt="" />"""
        } else {
            ""
        }
        return """
        <!DOCTYPE html>
        <html lang="ru">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1" />
            <title>Google Объектив</title>
            <style>
                * { box-sizing: border-box; margin: 0; padding: 0; }
                body { background: #141418; color: #f2f2f6; font-family: -apple-system, Roboto, sans-serif; height: 100vh; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 16px; padding: 20px; }
                img { max-width: 220px; max-height: 190px; border-radius: 12px; border: 1px solid rgba(255,255,255,0.16); }
                @keyframes spin { to { transform: rotate(360deg); } }
                .sp { width: 20px; height: 20px; border: 2.5px solid rgba(255,255,255,0.2); border-top-color: #4285F4; border-radius: 50%; animation: spin 0.75s linear infinite; }
            </style>
        </head>
        <body>
            $imgHtml
            <div style="display:flex;align-items:center;gap:10px;font-size:14px;">
                <div class="sp"></div>
                <span>Поиск в Google Объективе...</span>
            </div>
        </body>
        </html>
        """.trimIndent()
    }
}
