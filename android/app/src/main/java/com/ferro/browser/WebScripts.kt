package com.ferro.browser

import org.json.JSONArray

object WebScripts {
    fun buildInitScript(savedPasswords: JSONArray, adblockEnabled: Boolean, offerSavePasswords: Boolean): String {
        val cosmeticCss = if (adblockEnabled) AdBlockEngine.COSMETIC_CSS.trimIndent().replace("\n", " ") else ""
        val pwdsJson = savedPasswords.toString()

        return """
        (function() {
            if (window._ferroMobileInstalled) return;
            window._ferroMobileInstalled = true;

            window.ipc = {
                postMessage: function(msg) {
                    if (window.FerroBridge && window.FerroBridge.postMessage) {
                        window.FerroBridge.postMessage(typeof msg === 'string' ? msg : JSON.stringify(msg));
                    }
                }
            };

            // Cosmetic AdBlock stylesheet
            if (${adblockEnabled}) {
                const st = document.createElement('style');
                st.id = 'ferro-mobile-adblock';
                st.textContent = `$cosmeticCss`;
                (document.head || document.documentElement).appendChild(st);
            }

            // Password Autofill & Credential Capture
            const savedList = $pwdsJson;
            const curHost = (window.location.hostname || '').toLowerCase().replace(/^www\./, '');
            const matches = savedList.filter(item => {
                try {
                    const h = new URL(item.origin).hostname.toLowerCase().replace(/^www\./, '');
                    return h && (curHost === h || curHost.endsWith('.' + h) || h.endsWith('.' + curHost));
                } catch(e) { return false; }
            });

            function setNativeVal(el, val) {
                if (!el) return;
                const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')?.set;
                if (setter) setter.call(el, val);
                else el.value = val;
                el.dispatchEvent(new Event('input', { bubbles: true }));
                el.dispatchEvent(new Event('change', { bubbles: true }));
            }

            function findUserInp(passEl) {
                const scope = (passEl && passEl.closest('form')) || document;
                const all = Array.from(scope.querySelectorAll('input:not([type="hidden"]):not([type="submit"]):not([type="checkbox"])'));
                const idx = all.indexOf(passEl);
                for (let i = (idx > 0 ? idx - 1 : all.length - 1); i >= 0; i--) {
                    const c = all[i];
                    const t = (c.type || 'text').toLowerCase();
                    if (t === 'text' || t === 'email' || t === 'tel') return c;
                }
                return null;
            }

            if (matches.length > 0 && !window.location.href.startsWith('https://ferro.local')) {
                const pInp = document.querySelector('input[type="password"]');
                if (pInp && !pInp.value) {
                    const uInp = findUserInp(pInp);
                    if (uInp && !uInp.value && matches[0].username) setNativeVal(uInp, matches[0].username);
                    setNativeVal(pInp, matches[0].password);
                }
            }

            if (${offerSavePasswords} && !window.location.href.startsWith('https://ferro.local')) {
                function stageCreds(el) {
                    const scope = (el && el.closest ? el.closest('form') : null) || document;
                    const pList = Array.from(scope.querySelectorAll('input[type="password"]')).filter(i => i.value);
                    if (!pList.length) return;
                    const pEl = pList[pList.length - 1];
                    const uEl = findUserInp(pEl);
                    window.ipc.postMessage({
                        action: 'stage_password',
                        origin: window.location.origin,
                        username: uEl ? uEl.value.trim() : '',
                        password: pEl.value
                    });
                }
                document.addEventListener('submit', e => stageCreds(e.target), true);
                document.addEventListener('click', e => {
                    const b = e.target && e.target.closest ? e.target.closest('button, input[type="submit"], [role="button"]') : null;
                    if (b) stageCreds(b);
                }, true);
            }

            // Mobile Distraction-Free Reader Mode
            window._ferroToggleReaderMode = function() {
                const ID = 'ferro-mobile-reader';
                const ex = document.getElementById(ID);
                if (ex) { ex.remove(); document.documentElement.style.overflow = ''; return; }
                const root = document.querySelector('article, main, [role="main"]') || document.body;
                if (!root) return;
                const blocks = [];
                root.querySelectorAll('h1, h2, h3, p, blockquote, pre').forEach(n => {
                    if (n.closest('nav, header, footer, aside')) return;
                    const t = (n.innerText || '').trim();
                    if (t.length < 20 && !n.tagName.startsWith('H')) return;
                    const safe = t.replace(/&/g, '&amp;').replace(/</g, '&lt;');
                    const tag = n.tagName.toLowerCase();
                    blocks.push('<' + tag + '>' + safe + '</' + tag + '>');
                });
                if (!blocks.length) return;

                const host = document.createElement('div');
                host.id = ID;
                host.style.cssText = 'position:fixed;inset:0;z-index:2147483647;';
                const sh = host.attachShadow({ mode: 'open' });
                sh.innerHTML = `
                    <style>
                        * { box-sizing: border-box; margin: 0; padding: 0; -webkit-tap-highlight-color: transparent; }
                        .wrap { position: fixed; inset: 0; overflow-y: auto; background: var(--bg, #121215); color: var(--fg, #ececf1); font-family: -apple-system, Roboto, sans-serif; }
                        .wrap[data-p="sepia"] { --bg: #f4ecd8; --fg: #3b3121; --bar: #e8dec5; }
                        .wrap[data-p="light"] { --bg: #ffffff; --fg: #18181b; --bar: #f2f2f5; }
                        .wrap[data-p="dark"]  { --bg: #121215; --fg: #ececf1; --bar: #1b1b20; }
                        .bar { position: sticky; top: 0; background: var(--bar, #1b1b20); padding: 10px 14px; display: flex; align-items: center; justify-content: space-between; gap: 6px; border-bottom: 1px solid rgba(127,127,127,0.2); }
                        button { height: 34px; padding: 0 11px; border-radius: 8px; border: 1px solid rgba(127,127,127,0.3); background: transparent; color: inherit; font-size: 13px; touch-action: manipulation; }
                        button:active { transform: scale(0.96); }
                        article { padding: 24px 18px 80px; font-size: var(--sz, 18px); line-height: 1.68; max-width: 680px; margin: 0 auto; }
                        article p, article blockquote, article pre { margin-bottom: 1.15em; }
                        article h1, article h2, article h3 { margin: 1.2em 0 0.5em; line-height: 1.3; }
                    </style>
                    <div class="wrap" id="rw" data-p="dark">
                        <div class="bar">
                            <div style="display:flex;gap:6px;">
                                <button id="fm">A−</button>
                                <button id="fp">A+</button>
                                <button data-p="dark">Тёмн</button>
                                <button data-p="sepia">Сепия</button>
                                <button data-p="light">Светл</button>
                            </div>
                            <button id="rc">✕ Закрыть</button>
                        </div>
                        <article>
                            <h1>${'$'}{(document.title || '').replace(/</g, '&lt;')}</h1>
                            ${'$'}{blocks.join('')}
                        </article>
                    </div>
                `;
                let sz = 18;
                const rw = sh.getElementById('rw');
                sh.getElementById('fm').onclick = () => { sz = Math.max(14, sz - 2); rw.style.setProperty('--sz', sz + 'px'); };
                sh.getElementById('fp').onclick = () => { sz = Math.min(28, sz + 2); rw.style.setProperty('--sz', sz + 'px'); };
                sh.querySelectorAll('[data-p]').forEach(b => b.onclick = () => rw.setAttribute('data-p', b.dataset.p));
                sh.getElementById('rc').onclick = () => { host.remove(); document.documentElement.style.overflow = ''; };
                document.documentElement.style.overflow = 'hidden';
                document.documentElement.appendChild(host);
            };
        })();
        """.trimIndent()
    }
}
