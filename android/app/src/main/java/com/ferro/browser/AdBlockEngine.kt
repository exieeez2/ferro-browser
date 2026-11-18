package com.ferro.browser

import android.net.Uri
import android.webkit.WebResourceResponse
import java.io.ByteArrayInputStream
import java.util.concurrent.atomic.AtomicInteger

object AdBlockEngine {
    val blockedCount = AtomicInteger(0)

    private val BLOCKED_ROOT_DOMAINS = hashSetOf(
        "doubleclick.net",
        "googlesyndication.com",
        "googleadservices.com",
        "google-analytics.com",
        "googletagmanager.com",
        "googletagservices.com",
        "adnxs.com",
        "adsrvr.org",
        "criteo.com",
        "criteo.net",
        "taboola.com",
        "outbrain.com",
        "mgid.com",
        "adfox.ru",
        "an.yandex.ru",
        "mc.yandex.ru",
        "bs.yandex.ru",
        "yandexadexchange.net",
        "top-fwz1.mail.ru",
        "ad.mail.ru",
        "r.mail.ru",
        "popads.net",
        "popcash.net",
        "propellerads.com",
        "exoclick.com",
        "juicyads.com",
        "trafficjunky.net",
        "adsterra.com",
        "hilltopads.net",
        "clickadu.com",
        "bidvertiser.com",
        "revcontent.com",
        "teads.tv",
        "pubmatic.com",
        "rubiconproject.com",
        "openx.net",
        "casalemedia.com",
        "indexexchange.com",
        "smartadserver.com",
        "moatads.com",
        "adsafeprotected.com",
        "doubleverify.com",
        "scorecardresearch.com",
        "quantserve.com",
        "hotjar.com",
        "mouseflow.com",
        "luckyorange.com",
        "clarity.ms",
        "adcolony.com",
        "applovin.com",
        "unityads.unity3d.com",
        "vungle.com",
        "chartboost.com",
        "inmobi.com",
        "smaato.net",
        "amazon-adsystem.com",
        "media.net",
        "sharethrough.com",
        "3lift.com",
        "zemanta.com"
    )

    private val BLOCKED_PATH_TOKENS = arrayOf(
        "/pagead/",
        "/ads/ga-audiences",
        "/adsbygoogle.js",
        "/gpt.js",
        "/prebid",
        "/adframe",
        "/adbanner",
        "/popunder"
    )

    fun shouldBlock(uri: Uri?, pageHost: String?, enabled: Boolean): Boolean {
        if (!enabled || uri == null) return false
        val scheme = uri.scheme?.lowercase() ?: return false
        if (scheme != "http" && scheme != "https") return false
        val host = uri.host?.lowercase() ?: return false
        if (host == "ferro.local" || host.endsWith("lens.google.com")) return false

        // O(1) domain and parent-domain suffix walk
        var cursor = host
        while (true) {
            if (BLOCKED_ROOT_DOMAINS.contains(cursor)) {
                // Allow first-party navigation if user explicitly visited that domain
                if (pageHost != null && pageHost.equals(host, ignoreCase = true)) return false
                blockedCount.incrementAndGet()
                return true
            }
            val dot = cursor.indexOf('.')
            if (dot < 0) break
            cursor = cursor.substring(dot + 1)
        }

        val full = uri.toString().lowercase()
        for (tok in BLOCKED_PATH_TOKENS) {
            if (full.contains(tok)) {
                blockedCount.incrementAndGet()
                return true
            }
        }
        return false
    }

    fun emptyResponse(): WebResourceResponse = WebResourceResponse(
        "text/plain",
        "UTF-8",
        204,
        "No Content",
        mapOf("Access-Control-Allow-Origin" to "*"),
        ByteArrayInputStream(ByteArray(0))
    )

    const val COSMETIC_CSS = """
        ins.adsbygoogle, [id^="div-gpt-ad"], [id^="google_ads_iframe"],
        [class*="ad-banner"], [class*="adsbox"], [id^="yandex_rtb_"],
        .ya-partner, [data-ad-client], [data-ad-slot], iframe[src*="doubleclick"],
        iframe[src*="googlesyndication"], .outbrain-tm, .taboola-widget {
            display: none !important;
            height: 0 !important;
            visibility: hidden !important;
            pointer-events: none !important;
        }
    """
}
