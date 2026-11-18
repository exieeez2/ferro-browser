package com.ferro.browser

import android.annotation.SuppressLint
import android.app.Activity
import android.app.AlertDialog
import android.app.Dialog
import android.app.DownloadManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.util.Base64
import android.util.TypedValue
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.Window
import android.view.WindowInsetsController
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.webkit.CookieManager
import android.webkit.JavascriptInterface
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebStorage
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.GridLayout
import android.widget.HorizontalScrollView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import kotlin.math.min

class MainActivity : Activity() {

    data class MobileTab(
        val id: Int,
        var url: String,
        var title: String,
        var isIncognito: Boolean = false,
        var isPinned: Boolean = false,
        var isDesktopMode: Boolean = false,
        val webView: WebView
    )

    private lateinit var storage: BrowserStorage
    private lateinit var rootStack: FrameLayout
    private lateinit var mainColumn: LinearLayout
    private lateinit var webContainer: FrameLayout
    private lateinit var toolbarWrap: LinearLayout
    private lateinit var findBarWrap: LinearLayout
    private lateinit var findInput: EditText
    private lateinit var progressBar: ProgressBar

    // Address bar widgets
    private lateinit var engineBadgeView: TextView
    private lateinit var omniboxDisplay: TextView
    private lateinit var tabCountBadge: TextView
    private lateinit var backNavBtn: TextView

    private val tabs = mutableListOf<MobileTab>()
    private val closedUrls = ArrayDeque<String>()
    private var activeTabId: Int = -1
    private var nextTabId: Int = 1

    // Pending password staged before form redirect
    @Volatile
    private var pendingCredential: Triple<String, String, String>? = null

    // Fullscreen HTML5 video state
    private var fullscreenCustomView: View? = null
    private var fullscreenCallback: WebChromeClient.CustomViewCallback? = null

    // File upload / Lens gallery picker callbacks
    private var fileUploadCallback: ValueCallback<Array<Uri>>? = null
    private var activeLensOverlay: LensOverlayView? = null
    private var activeLensDialog: Dialog? = null

    private val dp: Float by lazy { resources.displayMetrics.density }
    private fun Int.dp(): Int = (this * dp).toInt()

    companion object {
        private const val REQ_FILE_CHOOSER = 1001
        private const val REQ_LENS_GALLERY = 1002

        private const val MOBILE_UA =
            "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36"
        private const val DESKTOP_UA =
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"

        private val TRACKING_PARAMS = setOf(
            "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content", "utm_id",
            "fbclid", "gclid", "yclid", "dclid", "msclkid", "igshid", "mc_cid", "si", "spm"
        )
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        storage = BrowserStorage(filesDir)

        CookieManager.getInstance().setAcceptCookie(true)

        buildRootLayout()
        applyThemeToChrome()

        // Restore session or open intent URL / newtab
        val incomingUrl = extractIntentUrl(intent)
        val savedSession = if (storage.settings.restoreSession) storage.loadSessionUrls() else emptyList()

        if (incomingUrl != null) {
            createTab(incomingUrl, switchImmediately = true)
        } else if (savedSession.isNotEmpty()) {
            savedSession.take(12).forEachIndexed { idx, u ->
                createTab(u, switchImmediately = (idx == savedSession.lastIndex))
            }
        } else {
            createTab("https://ferro.local/newtab", switchImmediately = true)
        }
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        val url = extractIntentUrl(intent)
        if (!url.isNullOrBlank()) {
            createTab(url, switchImmediately = true)
        }
    }

    private fun extractIntentUrl(intent: Intent?): String? {
        if (intent == null) return null
        if (intent.action == Intent.ACTION_VIEW) {
            return intent.dataString
        }
        if (intent.action == Intent.ACTION_WEB_SEARCH) {
            val q = intent.getStringExtra("query") ?: return null
            return resolveSearchOrUrl(q)
        }
        return null
    }

    override fun onPause() {
        super.onPause()
        val normalUrls = tabs.filter { !it.isIncognito }.map { it.url }
        storage.saveSessionUrls(normalUrls)
        CookieManager.getInstance().flush()
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        if (fullscreenCustomView != null) {
            exitVideoFullscreen()
            return
        }
        activeLensOverlay?.let {
            rootStack.removeView(it)
            activeLensOverlay = null
            return
        }
        if (findBarWrap.visibility == View.VISIBLE) {
            closeFindBar()
            return
        }
        val active = getActiveTab()
        if (active != null && active.webView.canGoBack()) {
            active.webView.goBack()
            return
        }
        super.onBackPressed()
    }

    private fun buildRootLayout() {
        rootStack = FrameLayout(this)
        mainColumn = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
        }

        // Find-in-page bar (hidden by default)
        findBarWrap = buildFindBar()

        // Progress bar (2dp)
        progressBar = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 100
            visibility = View.GONE
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 2.dp())
        }

        // WebView stack container
        webContainer = FrameLayout(this).apply {
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f)
        }

        // Native Mobile Address / Navigation Bar
        toolbarWrap = buildMobileToolbar()

        rebuildColumnOrder()
        rootStack.addView(mainColumn)
        setContentView(rootStack)
    }

    private fun rebuildColumnOrder() {
        mainColumn.removeAllViews()
        mainColumn.addView(findBarWrap)
        if (storage.settings.barPosition == "top") {
            mainColumn.addView(toolbarWrap)
            mainColumn.addView(progressBar)
            mainColumn.addView(webContainer)
        } else {
            mainColumn.addView(progressBar)
            mainColumn.addView(webContainer)
            mainColumn.addView(toolbarWrap)
        }
    }

    private fun buildFindBar(): LinearLayout {
        val bar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            visibility = View.GONE
            setPadding(12.dp(), 6.dp(), 8.dp(), 6.dp())
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 48.dp())
        }

        findInput = EditText(this).apply {
            hint = "Найти на странице..."
            setSingleLine(true)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            background = null
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f)
            imeOptions = EditorInfo.IME_ACTION_SEARCH
            setOnEditorActionListener { _, _, _ ->
                getActiveTab()?.webView?.findNext(true)
                true
            }
            addTextChangedListener(object : android.text.TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                    val q = s?.toString().orEmpty()
                    val wv = getActiveTab()?.webView ?: return
                    if (q.isEmpty()) wv.clearMatches() else wv.findAllAsync(q)
                }
                override fun afterTextChanged(s: android.text.Editable?) {}
            })
        }
        bar.addView(findInput)

        bar.addView(makeIconButton("↑") { getActiveTab()?.webView?.findNext(false) })
        bar.addView(makeIconButton("↓") { getActiveTab()?.webView?.findNext(true) })
        bar.addView(makeIconButton("✕") { closeFindBar() })
        return bar
    }

    private fun closeFindBar() {
        findBarWrap.visibility = View.GONE
        findInput.setText("")
        getActiveTab()?.webView?.clearMatches()
        hideKeyboard(findInput)
    }

    private fun buildMobileToolbar(): LinearLayout {
        val wrap = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(8.dp(), 7.dp(), 8.dp(), 7.dp())
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 56.dp())
        }

        backNavBtn = makeIconButton("←") {
            getActiveTab()?.webView?.let { if (it.canGoBack()) it.goBack() }
        }
        wrap.addView(backNavBtn)

        // Omnibox pill
        val omniPill = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(8.dp(), 0, 6.dp(), 0)
            layoutParams = LinearLayout.LayoutParams(0, 40.dp(), 1f).apply {
                leftMargin = 4.dp()
                rightMargin = 6.dp()
            }
            tag = "omnibox_pill"
            setOnClickListener { openUrlInputSheet() }
        }

        engineBadgeView = TextView(this).apply {
            gravity = Gravity.CENTER
            setTextColor(Color.WHITE)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
            typeface = Typeface.DEFAULT_BOLD
            layoutParams = LinearLayout.LayoutParams(22.dp(), 22.dp()).apply {
                rightMargin = 8.dp()
            }
            setOnClickListener { showSearchEngineQuickDialog() }
        }
        omniPill.addView(engineBadgeView)

        omniboxDisplay = TextView(this).apply {
            text = "Поиск или адрес..."
            setSingleLine(true)
            ellipsize = android.text.TextUtils.TruncateAt.END
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
        }
        omniPill.addView(omniboxDisplay)

        // Google Lens quick button inside Omnibox (1:1 like Chrome Mobile!)
        val lensBtn = TextView(this).apply {
            text = "◉"
            setTextColor(Color.parseColor("#4285F4"))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(32.dp(), 32.dp())
            contentDescription = "Google Объектив"
            setOnClickListener { startGoogleLensOnActiveTab() }
        }
        omniPill.addView(lensBtn)

        wrap.addView(omniPill)

        // Tab switcher counter badge [1]
        tabCountBadge = TextView(this).apply {
            text = "1"
            gravity = Gravity.CENTER
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
            typeface = Typeface.DEFAULT_BOLD
            layoutParams = LinearLayout.LayoutParams(32.dp(), 32.dp()).apply {
                rightMargin = 4.dp()
            }
            setOnClickListener { showTabSwitcherSheet() }
            setOnLongClickListener {
                createTab("https://ferro.local/newtab", switchImmediately = true)
                Toast.makeText(this@MainActivity, "Новая вкладка", Toast.LENGTH_SHORT).show()
                true
            }
        }
        wrap.addView(tabCountBadge)

        // 3-dots browser menu button
        val menuBtn = makeIconButton("⋮") { showBrowserMenuSheet() }
        wrap.addView(menuBtn)

        return wrap
    }

    private fun makeIconButton(symbol: String, onClick: () -> Unit): TextView {
        return TextView(this).apply {
            text = symbol
            gravity = Gravity.CENTER
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 18f)
            layoutParams = LinearLayout.LayoutParams(36.dp(), 36.dp())
            setOnClickListener { onClick() }
        }
    }

    private fun applyThemeToChrome() {
        val s = storage.settings
        val isLight = s.themeMode == "light"
        val bgChrome = if (isLight) Color.parseColor("#F1F3F5") else Color.parseColor("#141416")
        val bgPill = if (isLight) Color.parseColor("#E3E6EC") else Color.parseColor("#202025")
        val borderCol = if (isLight) Color.parseColor("#CFD4DC") else Color.parseColor("#2F2F37")
        val fgPrimary = if (isLight) Color.parseColor("#18181B") else Color.parseColor("#ECECEF")
        val fgMuted = if (isLight) Color.parseColor("#5F6368") else Color.parseColor("#9A9AA6")
        val radPx = s.cornerRadius.coerceIn(0, 20).dp().toFloat()

        window.statusBarColor = if (isLight) Color.parseColor("#F5F6F8") else Color.parseColor("#111111")
        window.navigationBarColor = bgChrome
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val appearance = if (isLight) {
                WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            } else 0
            window.insetsController?.setSystemBarsAppearance(
                appearance,
                WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            )
        }

        toolbarWrap.setBackgroundColor(bgChrome)
        findBarWrap.setBackgroundColor(bgChrome)
        findInput.setTextColor(fgPrimary)
        findInput.setHintTextColor(fgMuted)

        for (i in 0 until toolbarWrap.childCount) {
            val ch = toolbarWrap.getChildAt(i)
            if (ch is TextView && ch !== tabCountBadge) ch.setTextColor(fgPrimary)
            if (ch.tag == "omnibox_pill") {
                ch.background = GradientDrawable().apply {
                    setColor(bgPill)
                    cornerRadius = maxOf(8.dp().toFloat(), radPx)
                    setStroke(1.dp(), borderCol)
                }
            }
        }
        for (i in 0 until findBarWrap.childCount) {
            val ch = findBarWrap.getChildAt(i)
            if (ch is TextView && ch !== findInput) ch.setTextColor(fgPrimary)
        }

        omniboxDisplay.setTextColor(fgPrimary)
        tabCountBadge.setTextColor(fgPrimary)
        tabCountBadge.background = GradientDrawable().apply {
            setColor(Color.TRANSPARENT)
            cornerRadius = 7.dp().toFloat()
            setStroke((1.6f * dp).toInt(), fgPrimary)
        }

        updateEngineBadge()
        rebuildColumnOrder()
        syncActiveToolbar()
    }

    private fun updateEngineBadge() {
        val (ch, col) = when (storage.settings.searchEngine) {
            "duckduckgo" -> "D" to "#DE5833"
            "yandex" -> "Я" to "#FC3F1D"
            "bing" -> "B" to "#00809D"
            "brave" -> "V" to "#FB542B"
            "youtube" -> "▶" to "#FF0000"
            "github" -> "G" to "#24292F"
            else -> "G" to "#4285F4"
        }
        engineBadgeView.text = ch
        engineBadgeView.background = GradientDrawable().apply {
            shape = GradientDrawable.OVAL
            setColor(Color.parseColor(col))
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun createTab(initialUrl: String, switchImmediately: Boolean = true, incognito: Boolean = false): MobileTab {
        val id = nextTabId++
        val normalizedUrl = normalizeTargetUrl(initialUrl)

        val wv = WebView(this).apply {
            layoutParams = FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
            setBackgroundColor(if (storage.settings.themeMode == "light") Color.parseColor("#F5F6F8") else Color.parseColor("#111111"))
        }

        val ws = wv.settings
        ws.javaScriptEnabled = true
        ws.domStorageEnabled = !incognito
        ws.databaseEnabled = !incognito
        ws.mediaPlaybackRequiresUserGesture = false
        ws.setSupportZoom(true)
        ws.builtInZoomControls = true
        ws.displayZoomControls = false
        ws.useWideViewPort = true
        ws.loadWithOverviewMode = true
        ws.allowFileAccess = true
        ws.mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
        ws.cacheMode = if (incognito) WebSettings.LOAD_NO_CACHE else WebSettings.LOAD_DEFAULT
        ws.userAgentString = MOBILE_UA

        CookieManager.getInstance().setAcceptThirdPartyCookies(wv, !incognito)

        wv.addJavascriptInterface(FerroJsBridge(id), "FerroBridge")

        val tab = MobileTab(
            id = id,
            url = normalizedUrl,
            title = "Новая вкладка",
            isIncognito = incognito,
            webView = wv
        )

        wv.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val u = request.url.toString()
                if (u.startsWith("ferro://")) {
                    view.loadUrl("https://ferro.local/" + u.removePrefix("ferro://"))
                    return true
                }
                if (!u.startsWith("http://") && !u.startsWith("https://")) {
                    runCatching { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(u))) }
                    return true
                }
                return false
            }

            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                val reqUri = request.url
                if (reqUri.host == "ferro.local") {
                    return InternalPages.serve(this@MainActivity, reqUri, storage)
                }
                val pageHost = runCatching { Uri.parse(tab.url).host }.getOrNull()
                if (!request.isForMainFrame && AdBlockEngine.shouldBlock(reqUri, pageHost, storage.settings.adblockEnabled)) {
                    return AdBlockEngine.emptyResponse()
                }
                return super.shouldInterceptRequest(view, request)
            }

            override fun onPageStarted(view: WebView, url: String?, favicon: android.graphics.Bitmap?) {
                if (url != null) {
                    tab.url = url
                    if (tab.id == activeTabId) syncActiveToolbar()
                }
            }

            override fun onPageFinished(view: WebView, url: String?) {
                val curUrl = url ?: return
                tab.url = curUrl
                tab.title = view.title?.takeIf { it.isNotBlank() } ?: displayUrl(curUrl)

                if (!tab.isIncognito) {
                    storage.addHistory(curUrl, tab.title)
                }

                // Inject AdBlock cosmetic CSS + Password Autofill + Reader Mode
                val initJs = WebScripts.buildInitScript(
                    storage.passwordsJson(),
                    storage.settings.adblockEnabled,
                    storage.settings.offerSavePasswords && !tab.isIncognito
                )
                view.evaluateJavascript(initJs, null)

                // Show Save Password prompt if credentials were staged before redirect!
                if (!curUrl.startsWith("https://ferro.local") && !tab.isIncognito) {
                    pendingCredential?.let { (orig, usr, pwd) ->
                        if (!storage.isPasswordAlreadySaved(orig, usr, pwd)) {
                            showSavePasswordSheet(orig, usr, pwd)
                        } else {
                            pendingCredential = null
                        }
                    }
                }

                if (tab.id == activeTabId) syncActiveToolbar()
            }
        }

        wv.webChromeClient = object : WebChromeClient() {
            override fun onProgressChanged(view: WebView?, newProgress: Int) {
                if (tab.id != activeTabId) return
                if (newProgress in 1..99) {
                    progressBar.visibility = View.VISIBLE
                    progressBar.progress = newProgress
                } else {
                    progressBar.visibility = View.GONE
                }
            }

            override fun onReceivedTitle(view: WebView?, title: String?) {
                if (!title.isNullOrBlank()) {
                    tab.title = title
                    if (tab.id == activeTabId) syncActiveToolbar()
                }
            }

            override fun onShowCustomView(view: View?, callback: CustomViewCallback?) {
                if (view == null) return
                fullscreenCustomView = view
                fullscreenCallback = callback
                mainColumn.visibility = View.GONE
                rootStack.addView(
                    view,
                    FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.MATCH_PARENT
                    )
                )
                window.addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
            }

            override fun onHideCustomView() {
                exitVideoFullscreen()
            }

            override fun onShowFileChooser(
                webView: WebView?,
                filePathCallback: ValueCallback<Array<Uri>>?,
                fileChooserParams: FileChooserParams?
            ): Boolean {
                fileUploadCallback?.onReceiveValue(null)
                fileUploadCallback = filePathCallback
                return try {
                    val intent = fileChooserParams?.createIntent() ?: Intent(Intent.ACTION_GET_CONTENT).apply {
                        addCategory(Intent.CATEGORY_OPENABLE)
                        type = "*/*"
                    }
                    startActivityForResult(intent, REQ_FILE_CHOOSER)
                    true
                } catch (_: Exception) {
                    fileUploadCallback = null
                    false
                }
            }
        }

        // Long-press custom context sheet (with Google Lens on images & Clean URL on links!)
        wv.setOnLongClickListener {
            val hit = wv.hitTestResult
            when (hit.type) {
                WebView.HitTestResult.IMAGE_TYPE,
                WebView.HitTestResult.SRC_IMAGE_ANCHOR_TYPE -> {
                    val imgUrl = hit.extra
                    if (!imgUrl.isNullOrBlank()) {
                        showImageOrLinkContextSheet(imageUrl = imgUrl, linkUrl = if (hit.type == WebView.HitTestResult.SRC_IMAGE_ANCHOR_TYPE) imgUrl else null)
                        return@setOnLongClickListener true
                    }
                }
                WebView.HitTestResult.SRC_ANCHOR_TYPE -> {
                    val linkUrl = hit.extra
                    if (!linkUrl.isNullOrBlank()) {
                        showImageOrLinkContextSheet(imageUrl = null, linkUrl = linkUrl)
                        return@setOnLongClickListener true
                    }
                }
            }
            false
        }

        // System DownloadManager integration
        wv.setDownloadListener { url, userAgent, contentDisposition, mimeType, _ ->
            startAndroidDownload(url, userAgent, contentDisposition, mimeType)
        }

        tabs.add(tab)
        webContainer.addView(wv)
        wv.loadUrl(normalizedUrl)

        if (switchImmediately) {
            switchToTab(id)
        } else {
            wv.visibility = View.GONE
            syncActiveToolbar()
        }
        return tab
    }

    private fun exitVideoFullscreen() {
        fullscreenCustomView?.let { rootStack.removeView(it) }
        fullscreenCustomView = null
        fullscreenCallback?.onCustomViewHidden()
        fullscreenCallback = null
        mainColumn.visibility = View.VISIBLE
        window.clearFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
    }

    private fun switchToTab(id: Int) {
        activeTabId = id
        tabs.forEach { t ->
            t.webView.visibility = if (t.id == id) View.VISIBLE else View.GONE
        }
        syncActiveToolbar()
    }

    private fun closeTab(id: Int) {
        val idx = tabs.indexOfFirst { it.id == id }
        if (idx < 0) return
        val removed = tabs.removeAt(idx)
        if (!removed.isIncognito && !removed.url.startsWith("https://ferro.local/newtab")) {
            closedUrls.addLast(removed.url)
        }
        webContainer.removeView(removed.webView)
        removed.webView.destroy()

        if (tabs.isEmpty()) {
            createTab("https://ferro.local/newtab", switchImmediately = true)
        } else if (activeTabId == id) {
            val nextIdx = idx.coerceAtMost(tabs.lastIndex)
            switchToTab(tabs[nextIdx].id)
        } else {
            syncActiveToolbar()
        }
    }

    private fun getActiveTab(): MobileTab? = tabs.firstOrNull { it.id == activeTabId }

    private fun normalizeTargetUrl(raw: String): String {
        val t = raw.trim()
        if (t.isEmpty() || t == "ferro://newtab") return "https://ferro.local/newtab"
        if (t.startsWith("ferro://")) return "https://ferro.local/" + t.removePrefix("ferro://")
        return resolveSearchOrUrl(t)
    }

    private fun resolveSearchOrUrl(query: String): String {
        val q = query.trim()
        if (q.startsWith("http://") || q.startsWith("https://")) return q
        if (q.startsWith("ferro://")) return "https://ferro.local/" + q.removePrefix("ferro://")
        val isDomain = Regex("^[a-zA-Z0-9-]+(\\.[a-zA-Z0-9-]+)+(:\\d+)?(/.*)?$").matches(q)
        if (isDomain) return "https://$q"

        val prefix = when (storage.settings.searchEngine) {
            "duckduckgo" -> "https://duckduckgo.com/?q="
            "yandex" -> "https://ya.ru/search/?text="
            "bing" -> "https://www.bing.com/search?q="
            "brave" -> "https://search.brave.com/search?q="
            "youtube" -> "https://m.youtube.com/results?search_query="
            "github" -> "https://github.com/search?q="
            else -> "https://www.google.com/search?q="
        }
        return prefix + Uri.encode(q)
    }

    private fun displayUrl(url: String): String {
        if (url.startsWith("https://ferro.local/newtab")) return ""
        if (url.startsWith("https://ferro.local/")) return "ferro://" + url.removePrefix("https://ferro.local/")
        return runCatching {
            val u = Uri.parse(url)
            (u.host ?: url).removePrefix("www.")
        }.getOrDefault(url)
    }

    private fun syncActiveToolbar() {
        val active = getActiveTab()
        tabCountBadge.text = tabs.size.toString()
        if (active == null) return

        val disp = displayUrl(active.url)
        val prefix = if (active.isIncognito) "Инкогнито • " else ""
        omniboxDisplay.text = if (disp.isEmpty()) "${prefix}Поиск или адрес..." else "$prefix$disp"
        backNavBtn.alpha = if (active.webView.canGoBack()) 1f else 0.38f
    }

    // =========================================================================
    // Google Lens — Interactive Region Selection + Image Search + Bottom Sheet
    // =========================================================================
    private fun startGoogleLensOnActiveTab() {
        val active = getActiveTab() ?: return
        if (active.url.startsWith("https://ferro.local/newtab")) {
            openGalleryForLens()
            return
        }
        activeLensOverlay?.let { rootStack.removeView(it) }
        val overlay = LensOverlayView(
            context = this,
            targetWebView = active.webView,
            onRegionCaptured = { b64Png ->
                rootStack.removeView(activeLensOverlay)
                activeLensOverlay = null
                openGoogleLensBottomSheet(b64Png = b64Png, imageUrl = "")
            },
            onCancel = {
                rootStack.removeView(activeLensOverlay)
                activeLensOverlay = null
            }
        )
        activeLensOverlay = overlay
        rootStack.addView(
            overlay,
            FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
        )
    }

    private fun openGalleryForLens() {
        runCatching {
            val intent = Intent(Intent.ACTION_GET_CONTENT).apply {
                type = "image/*"
                addCategory(Intent.CATEGORY_OPENABLE)
            }
            startActivityForResult(Intent.createChooser(intent, "Выбрать фото для Google Объектива"), REQ_LENS_GALLERY)
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun openGoogleLensBottomSheet(b64Png: String, imageUrl: String) {
        activeLensDialog?.dismiss()
        InternalPages.pendingLensB64 = b64Png
        InternalPages.pendingLensUrl = imageUrl

        val sheet = createBottomSheetDialog(heightRatio = 0.80f)
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#16161A"))
        }

        // Top handle + header bar
        val header = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(16.dp(), 10.dp(), 10.dp(), 10.dp())
            setBackgroundColor(Color.parseColor("#1C1C22"))
        }

        val title = TextView(this).apply {
            text = "Google Объектив"
            setTextColor(Color.WHITE)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            typeface = Typeface.DEFAULT_BOLD
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
        }
        header.addView(title)

        var lensWvRef: WebView? = null

        val popBtn = TextView(this).apply {
            text = "Во вкладке ↗"
            setTextColor(Color.parseColor("#8AB4F8"))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            setPadding(12.dp(), 6.dp(), 12.dp(), 6.dp())
            setOnClickListener {
                val u = lensWvRef?.url
                if (!u.isNullOrBlank() && !u.startsWith("https://ferro.local")) {
                    sheet.dismiss()
                    createTab(u, switchImmediately = true)
                }
            }
        }
        header.addView(popBtn)

        val clsBtn = makeIconButton("✕") { sheet.dismiss() }.apply { setTextColor(Color.WHITE) }
        header.addView(clsBtn)
        root.addView(header)

        val lensWv = WebView(this).apply {
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f)
            setBackgroundColor(Color.parseColor("#141418"))
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.userAgentString = MOBILE_UA
            webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                    if (request.url.host == "ferro.local") {
                        return InternalPages.serve(this@MainActivity, request.url, storage)
                    }
                    return super.shouldInterceptRequest(view, request)
                }
            }
        }
        lensWvRef = lensWv
        CookieManager.getInstance().setAcceptThirdPartyCookies(lensWv, true)
        root.addView(lensWv)

        lensWv.loadUrl("https://ferro.local/lens-upload")
        sheet.setContentView(root)
        sheet.setOnDismissListener { lensWv.destroy() }
        activeLensDialog = sheet
        sheet.show()
    }

    // =========================================================================
    // Bottom Sheets: URL Input, Context Menu, Tab Switcher, Browser Menu
    // =========================================================================
    private fun createBottomSheetDialog(heightRatio: Float = 0f): Dialog {
        return Dialog(this).apply {
            requestWindowFeature(Window.FEATURE_NO_TITLE)
            window?.let { w ->
                w.setBackgroundDrawable(GradientDrawable().apply {
                    val isLight = storage.settings.themeMode == "light"
                    setColor(if (isLight) Color.parseColor("#FFFFFF") else Color.parseColor("#1A1A1F"))
                    cornerRadii = floatArrayOf(18.dp().toFloat(), 18.dp().toFloat(), 18.dp().toFloat(), 18.dp().toFloat(), 0f, 0f, 0f, 0f)
                })
                w.setGravity(Gravity.BOTTOM)
                val targetH = if (heightRatio > 0f) {
                    (resources.displayMetrics.heightPixels * heightRatio).toInt()
                } else {
                    ViewGroup.LayoutParams.WRAP_CONTENT
                }
                w.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, targetH)
            }
        }
    }

    private fun openUrlInputSheet() {
        val active = getActiveTab() ?: return
        val dialog = createBottomSheetDialog()
        val isLight = storage.settings.themeMode == "light"
        val fg = if (isLight) Color.parseColor("#18181B") else Color.parseColor("#F2F2F6")
        val muted = if (isLight) Color.parseColor("#686C78") else Color.parseColor("#8F8F9C")

        val box = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(16.dp(), 16.dp(), 16.dp(), 20.dp())
        }

        val row = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val curRaw = if (active.url.startsWith("https://ferro.local/newtab")) ""
        else if (active.url.startsWith("https://ferro.local/")) "ferro://" + active.url.removePrefix("https://ferro.local/")
        else active.url

        val input = EditText(this).apply {
            setText(curRaw)
            hint = "Введите адрес или поисковый запрос..."
            setTextColor(fg)
            setHintTextColor(muted)
            setSingleLine(true)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
            imeOptions = EditorInfo.IME_ACTION_GO
            layoutParams = LinearLayout.LayoutParams(0, 46.dp(), 1f)
            selectAll()
        }
        row.addView(input)

        val goBtn = TextView(this).apply {
            text = "Перейти"
            setTextColor(if (isLight) Color.WHITE else Color.parseColor("#111111"))
            typeface = Typeface.DEFAULT_BOLD
            gravity = Gravity.CENTER
            setPadding(14.dp(), 0, 14.dp(), 0)
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, 40.dp()).apply {
                leftMargin = 8.dp()
            }
            background = GradientDrawable().apply {
                setColor(if (isLight) Color.parseColor("#18181B") else Color.parseColor("#ECECEC"))
                cornerRadius = 10.dp().toFloat()
            }
            setOnClickListener {
                val q = input.text.toString().trim()
                if (q.isNotEmpty()) {
                    dialog.dismiss()
                    active.webView.loadUrl(normalizeTargetUrl(q))
                }
            }
        }
        row.addView(goBtn)
        box.addView(row)

        // Quick action chips under input (Copy URL, Clean URL, Paste & Go)
        val chips = HorizontalScrollView(this).apply {
            isHorizontalScrollBarEnabled = false
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                topMargin = 10.dp()
            }
        }
        val chipRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }

        fun addChip(label: String, act: () -> Unit) {
            val c = TextView(this).apply {
                text = label
                setTextColor(fg)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 12.5f)
                setPadding(12.dp(), 7.dp(), 12.dp(), 7.dp())
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                    rightMargin = 8.dp()
                }
                background = GradientDrawable().apply {
                    cornerRadius = 8.dp().toFloat()
                    setStroke(1.dp(), muted)
                }
                setOnClickListener { act() }
            }
            chipRow.addView(c)
        }

        if (curRaw.isNotEmpty()) {
            addChip("Копировать URL") {
                copyText(curRaw, "Адрес скопирован")
                dialog.dismiss()
            }
            addChip("Чистая ссылка") {
                copyText(cleanTrackingUrl(curRaw), "Чистая ссылка скопирована")
                dialog.dismiss()
            }
            addChip("Поделиться") {
                dialog.dismiss()
                shareUrl(curRaw)
            }
        }
        addChip("Вставить и перейти") {
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            val pasted = cm.primaryClip?.getItemAt(0)?.coerceToText(this)?.toString()?.trim()
            if (!pasted.isNullOrEmpty()) {
                dialog.dismiss()
                active.webView.loadUrl(normalizeTargetUrl(pasted))
            }
        }
        chips.addView(chipRow)
        box.addView(chips)

        input.setOnEditorActionListener { _, _, event ->
            if (event == null || event.keyCode == KeyEvent.KEYCODE_ENTER) {
                goBtn.performClick()
                true
            } else false
        }

        dialog.setContentView(box)
        dialog.window?.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE)
        dialog.show()
        input.requestFocus()
    }

    private fun showImageOrLinkContextSheet(imageUrl: String?, linkUrl: String?) {
        val dialog = createBottomSheetDialog()
        val isLight = storage.settings.themeMode == "light"
        val fg = if (isLight) Color.parseColor("#18181B") else Color.parseColor("#F2F2F6")
        val muted = if (isLight) Color.parseColor("#686C78") else Color.parseColor("#8F8F9C")

        val list = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(12.dp(), 14.dp(), 12.dp(), 20.dp())
        }

        val hdr = TextView(this).apply {
            text = (linkUrl ?: imageUrl).orEmpty()
            setTextColor(muted)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
            maxLines = 2
            ellipsize = android.text.TextUtils.TruncateAt.END
            setPadding(12.dp(), 0, 12.dp(), 12.dp())
        }
        list.addView(hdr)

        fun item(label: String, highlightColor: Int? = null, onClick: () -> Unit) {
            val tv = TextView(this).apply {
                text = label
                setTextColor(highlightColor ?: fg)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
                setPadding(14.dp(), 13.dp(), 14.dp(), 13.dp())
                setOnClickListener {
                    dialog.dismiss()
                    onClick()
                }
            }
            list.addView(tv)
        }

        if (!linkUrl.isNullOrBlank()) {
            item("Открыть ссылку в новой вкладке") { createTab(linkUrl, switchImmediately = true) }
            item("Открыть во вкладке Инкогнито") { createTab(linkUrl, switchImmediately = true, incognito = true) }
            item("Копировать адрес ссылки") { copyText(linkUrl, "Ссылка скопирована") }
            val clean = cleanTrackingUrl(linkUrl)
            if (clean != linkUrl) {
                item("Копировать чистую ссылку (без меток)") { copyText(clean, "Чистая ссылка скопирована") }
            }
            item("Поделиться ссылкой") { shareUrl(linkUrl) }
        }

        if (!imageUrl.isNullOrBlank()) {
            item("Найти картинку через Google Объектив", Color.parseColor("#4285F4")) {
                openGoogleLensBottomSheet(b64Png = "", imageUrl = imageUrl)
            }
            item("Открыть изображение в новой вкладке") { createTab(imageUrl, switchImmediately = true) }
            item("Скачать изображение") { startAndroidDownload(imageUrl, MOBILE_UA, "", "image/*") }
            item("Копировать адрес изображения") { copyText(imageUrl, "Адрес изображения скопирован") }
        }

        dialog.setContentView(list)
        dialog.show()
    }

    private fun showTabSwitcherSheet() {
        val dialog = createBottomSheetDialog(heightRatio = 0.75f)
        val isLight = storage.settings.themeMode == "light"
        val fg = if (isLight) Color.parseColor("#18181B") else Color.parseColor("#F2F2F6")
        val cardBg = if (isLight) Color.parseColor("#F2F4F8") else Color.parseColor("#24242B")
        val border = if (isLight) Color.parseColor("#D0D5DD") else Color.parseColor("#353540")

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(14.dp(), 14.dp(), 14.dp(), 16.dp())
        }

        val topRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(4.dp(), 0, 4.dp(), 12.dp())
        }

        topRow.addView(TextView(this).apply {
            text = "Вкладки (${tabs.size})"
            setTextColor(fg)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
            typeface = Typeface.DEFAULT_BOLD
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
        })

        if (closedUrls.isNotEmpty()) {
            topRow.addView(TextView(this).apply {
                text = "Вернуть"
                setTextColor(fg)
                setPadding(10.dp(), 6.dp(), 10.dp(), 6.dp())
                setOnClickListener {
                    val last = closedUrls.removeLastOrNull()
                    if (last != null) {
                        dialog.dismiss()
                        createTab(last, switchImmediately = true)
                    }
                }
            })
        }

        topRow.addView(TextView(this).apply {
            text = "+ Инкогнито"
            setTextColor(fg)
            setPadding(10.dp(), 6.dp(), 10.dp(), 6.dp())
            setOnClickListener {
                dialog.dismiss()
                createTab("https://ferro.local/newtab", switchImmediately = true, incognito = true)
            }
        })

        topRow.addView(TextView(this).apply {
            text = "+ Новая"
            setTextColor(if (isLight) Color.WHITE else Color.parseColor("#111111"))
            typeface = Typeface.DEFAULT_BOLD
            setPadding(12.dp(), 6.dp(), 12.dp(), 6.dp())
            background = GradientDrawable().apply {
                setColor(if (isLight) Color.parseColor("#18181B") else Color.parseColor("#ECECEC"))
                cornerRadius = 8.dp().toFloat()
            }
            setOnClickListener {
                dialog.dismiss()
                createTab("https://ferro.local/newtab", switchImmediately = true)
            }
        })
        root.addView(topRow)

        val scroll = ScrollView(this).apply {
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f)
        }
        val grid = GridLayout(this).apply {
            columnCount = 2
            layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        }

        val screenW = resources.displayMetrics.widthPixels
        val cardW = (screenW - 44.dp()) / 2

        tabs.sortedByDescending { it.isPinned }.forEach { t ->
            val card = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(12.dp(), 10.dp(), 10.dp(), 12.dp())
                layoutParams = GridLayout.LayoutParams().apply {
                    width = cardW
                    height = 108.dp()
                    setMargins(4.dp(), 4.dp(), 4.dp(), 4.dp())
                }
                background = GradientDrawable().apply {
                    setColor(cardBg)
                    cornerRadius = 12.dp().toFloat()
                    setStroke(if (t.id == activeTabId) 2.dp() else 1.dp(), if (t.id == activeTabId) fg else border)
                }
                setOnClickListener {
                    dialog.dismiss()
                    switchToTab(t.id)
                }
            }

            val cardTop = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
            }

            cardTop.addView(TextView(this).apply {
                val badge = buildString {
                    if (t.isPinned) append("[Закреп] ")
                    if (t.isIncognito) append("[Инког] ")
                    append(t.title.ifBlank { "Новая вкладка" })
                }
                text = badge
                setTextColor(fg)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                typeface = Typeface.DEFAULT_BOLD
                maxLines = 2
                ellipsize = android.text.TextUtils.TruncateAt.END
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            })

            cardTop.addView(TextView(this).apply {
                text = "✕"
                setTextColor(fg)
                setPadding(8.dp(), 2.dp(), 4.dp(), 2.dp())
                setOnClickListener {
                    closeTab(t.id)
                    dialog.dismiss()
                    if (tabs.isNotEmpty()) showTabSwitcherSheet()
                }
            })
            card.addView(cardTop)

            card.addView(TextView(this).apply {
                text = displayUrl(t.url).ifEmpty { "ferro://newtab" }
                setTextColor(Color.GRAY)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 11.5f)
                setSingleLine(true)
                ellipsize = android.text.TextUtils.TruncateAt.END
                setPadding(0, 8.dp(), 0, 6.dp())
            })

            card.addView(TextView(this).apply {
                text = if (t.isPinned) "Открепить" else "Закрепить"
                setTextColor(Color.parseColor("#60A5FA"))
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 11.5f)
                setOnClickListener {
                    t.isPinned = !t.isPinned
                    dialog.dismiss()
                    showTabSwitcherSheet()
                }
            })

            grid.addView(card)
        }

        scroll.addView(grid)
        root.addView(scroll)
        dialog.setContentView(root)
        dialog.show()
    }

    private fun showBrowserMenuSheet() {
        val active = getActiveTab() ?: return
        val dialog = createBottomSheetDialog()
        val isLight = storage.settings.themeMode == "light"
        val fg = if (isLight) Color.parseColor("#18181B") else Color.parseColor("#F2F2F6")
        val divider = if (isLight) Color.parseColor("#E4E7EC") else Color.parseColor("#2B2B33")

        val scroll = ScrollView(this)
        val box = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(10.dp(), 12.dp(), 10.dp(), 20.dp())
        }

        // Top 5 quick action buttons row
        val quickRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            weightSum = 5f
            setPadding(4.dp(), 0, 4.dp(), 10.dp())
        }

        fun quickBtn(sym: String, lbl: String, act: () -> Unit) {
            val col = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
                setPadding(4.dp(), 8.dp(), 4.dp(), 8.dp())
                setOnClickListener {
                    dialog.dismiss()
                    act()
                }
            }
            col.addView(TextView(this).apply {
                text = sym
                setTextColor(fg)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 18f)
                gravity = Gravity.CENTER
            })
            col.addView(TextView(this).apply {
                text = lbl
                setTextColor(fg)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 10.5f)
                gravity = Gravity.CENTER
            })
            quickRow.addView(col)
        }

        quickBtn("→", "Вперёд") { if (active.webView.canGoForward()) active.webView.goForward() }
        quickBtn("↻", "Обновить") { active.webView.reload() }
        val starred = storage.isBookmarked(active.url)
        quickBtn(if (starred) "★" else "☆", "Закладка") {
            val added = storage.toggleBookmark(active.url, active.title)
            Toast.makeText(this, if (added) "Добавлено в закладки" else "Удалено из закладок", Toast.LENGTH_SHORT).show()
        }
        quickBtn("↗", "Поделиться") { shareUrl(active.url) }
        quickBtn("⌕", "Найти") {
            findBarWrap.visibility = View.VISIBLE
            findInput.requestFocus()
        }
        box.addView(quickRow)

        fun addSep() {
            box.addView(View(this).apply {
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 1.dp()).apply {
                    topMargin = 4.dp()
                    bottomMargin = 4.dp()
                }
                setBackgroundColor(divider)
            })
        }

        fun rowItem(title: String, color: Int = fg, onClick: () -> Unit) {
            box.addView(TextView(this).apply {
                text = title
                setTextColor(color)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
                setPadding(16.dp(), 12.dp(), 16.dp(), 12.dp())
                setOnClickListener {
                    dialog.dismiss()
                    onClick()
                }
            })
        }

        addSep()
        rowItem("Найти с помощью Google Объектива", Color.parseColor("#4285F4")) { startGoogleLensOnActiveTab() }
        rowItem("Режим чтения") {
            active.webView.evaluateJavascript("if(window._ferroToggleReaderMode) window._ferroToggleReaderMode();", null)
        }
        rowItem("Перевести страницу на русский") {
            if (!active.url.startsWith("https://ferro.local")) {
                val trUrl = "https://translate.google.com/translate?sl=auto&tl=ru&u=" + Uri.encode(active.url)
                active.webView.loadUrl(trUrl)
            }
        }
        rowItem(if (active.isDesktopMode) "Версия для ПК: Включена ✓" else "Версия для ПК") {
            active.isDesktopMode = !active.isDesktopMode
            active.webView.settings.userAgentString = if (active.isDesktopMode) DESKTOP_UA else MOBILE_UA
            active.webView.settings.useWideViewPort = true
            active.webView.settings.loadWithOverviewMode = true
            active.webView.reload()
        }
        rowItem("QR-код этой страницы") { showQrDialog(active.url) }

        addSep()
        rowItem("Новая вкладка") { createTab("https://ferro.local/newtab", switchImmediately = true) }
        rowItem("Новая вкладка Инкогнито") { createTab("https://ferro.local/newtab", switchImmediately = true, incognito = true) }
        rowItem("Пароли") { active.webView.loadUrl("https://ferro.local/passwords") }
        rowItem("История просмотров") { active.webView.loadUrl("https://ferro.local/history") }
        rowItem("Загрузки") { active.webView.loadUrl("https://ferro.local/downloads") }
        rowItem("Быстрые заметки") { active.webView.loadUrl("https://ferro.local/notes") }

        addSep()
        val abStatus = if (storage.settings.adblockEnabled) {
            "Блокировщик рекламы: Вкл (${AdBlockEngine.blockedCount.get()})"
        } else {
            "Блокировщик рекламы: Выкл"
        }
        rowItem(abStatus) {
            storage.settings.adblockEnabled = !storage.settings.adblockEnabled
            storage.saveSettings(storage.settings)
            active.webView.reload()
        }
        rowItem("Настройки браузера") { active.webView.loadUrl("https://ferro.local/settings") }

        scroll.addView(box)
        dialog.setContentView(scroll)
        dialog.show()
    }

    private fun showSearchEngineQuickDialog() {
        val names = arrayOf("Google", "DuckDuckGo", "Яндекс", "Bing", "Brave", "YouTube", "GitHub")
        val ids = arrayOf("google", "duckduckgo", "yandex", "bing", "brave", "youtube", "github")
        AlertDialog.Builder(this)
            .setTitle("Поисковая система")
            .setItems(names) { _, which ->
                storage.settings.searchEngine = ids[which]
                storage.saveSettings(storage.settings)
                updateEngineBadge()
                val active = getActiveTab()
                if (active?.url?.startsWith("https://ferro.local/newtab") == true) {
                    active.webView.reload()
                }
            }
            .show()
    }

    private fun showSavePasswordSheet(origin: String, username: String, password: String) {
        val dialog = createBottomSheetDialog()
        val isLight = storage.settings.themeMode == "light"
        val fg = if (isLight) Color.parseColor("#18181B") else Color.parseColor("#F2F2F6")

        val box = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(18.dp(), 18.dp(), 18.dp(), 22.dp())
        }

        box.addView(TextView(this).apply {
            text = "Сохранить пароль?"
            setTextColor(fg)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
            typeface = Typeface.DEFAULT_BOLD
        })
        box.addView(TextView(this).apply {
            text = origin
            setTextColor(Color.GRAY)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12.5f)
            setPadding(0, 4.dp(), 0, 12.dp())
        })

        val userInp = EditText(this).apply {
            setText(username)
            hint = "Логин"
            setTextColor(fg)
        }
        val passInp = EditText(this).apply {
            setText(password)
            hint = "Пароль"
            setTextColor(fg)
        }
        box.addView(userInp)
        box.addView(passInp)

        val btns = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END
            setPadding(0, 14.dp(), 0, 0)
        }
        btns.addView(TextView(this).apply {
            text = "Не сейчас"
            setTextColor(Color.GRAY)
            setPadding(14.dp(), 10.dp(), 14.dp(), 10.dp())
            setOnClickListener {
                pendingCredential = null
                dialog.dismiss()
            }
        })
        btns.addView(TextView(this).apply {
            text = "Сохранить"
            setTextColor(Color.parseColor("#4285F4"))
            typeface = Typeface.DEFAULT_BOLD
            setPadding(14.dp(), 10.dp(), 14.dp(), 10.dp())
            setOnClickListener {
                storage.savePassword(origin, userInp.text.toString(), passInp.text.toString())
                pendingCredential = null
                dialog.dismiss()
                Toast.makeText(this@MainActivity, "Пароль сохранён в Ferro", Toast.LENGTH_SHORT).show()
            }
        })
        box.addView(btns)
        dialog.setContentView(box)
        dialog.show()
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun showQrDialog(url: String) {
        if (url.startsWith("https://ferro.local")) {
            Toast.makeText(this, "Откройте веб-страницу для генерации QR-кода", Toast.LENGTH_SHORT).show()
            return
        }
        val dialog = createBottomSheetDialog()
        val box = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(20.dp(), 20.dp(), 20.dp(), 24.dp())
        }
        box.addView(TextView(this).apply {
            text = "QR-код страницы"
            setTextColor(Color.WHITE)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
            typeface = Typeface.DEFAULT_BOLD
            setPadding(0, 0, 0, 14.dp())
        })
        val wv = WebView(this).apply {
            layoutParams = LinearLayout.LayoutParams(220.dp(), 220.dp())
            val qr = "https://api.qrserver.com/v1/create-qr-code/?size=220x220&margin=10&data=" + Uri.encode(url)
            loadDataWithBaseURL(null, "<body style='margin:0;background:#fff;display:flex;align-items:center;justify-content:center;'><img src='$qr' width='220' height='220'/></body>", "text/html", "UTF-8", null)
        }
        box.addView(wv)
        box.addView(TextView(this).apply {
            text = "Скопировать ссылку"
            setTextColor(Color.parseColor("#8AB4F8"))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            setPadding(16.dp(), 16.dp(), 16.dp(), 4.dp())
            setOnClickListener {
                copyText(url, "Ссылка скопирована")
                dialog.dismiss()
            }
        })
        dialog.setContentView(box)
        dialog.show()
    }

    // =========================================================================
    // JS Bridge & Utilities
    // =========================================================================
    inner class FerroJsBridge(private val tabId: Int) {
        @JavascriptInterface
        fun postMessage(rawJson: String) {
            runOnUiThread {
                runCatching {
                    val msg = JSONObject(rawJson)
                    when (msg.optString("action")) {
                        "save_settings" -> {
                            msg.optJSONObject("settings")?.let { sObj ->
                                storage.saveSettings(MobileSettings.fromJson(sObj))
                                applyThemeToChrome()
                            }
                        }
                        "stage_password" -> {
                            val orig = msg.optString("origin")
                            val usr = msg.optString("username")
                            val pwd = msg.optString("password")
                            if (orig.isNotBlank() && pwd.isNotEmpty() && storage.settings.offerSavePasswords) {
                                pendingCredential = Triple(orig, usr, pwd)
                            }
                        }
                        "save_password" -> {
                            storage.savePassword(
                                msg.optString("origin"),
                                msg.optString("username"),
                                msg.optString("password")
                            )
                            Toast.makeText(this@MainActivity, "Пароль сохранён", Toast.LENGTH_SHORT).show()
                        }
                        "delete_password" -> storage.deletePassword(msg.optLong("id"))
                        "clear_history" -> storage.clearHistory()
                        "clear_downloads" -> storage.clearDownloads()
                        "save_notes" -> storage.saveNotesFromJson(msg.optString("body"))
                        "copy_text" -> copyText(msg.optString("text"), msg.optString("toast"))
                        "open_image_lens_picker" -> openGalleryForLens()
                        "clear_browsing_data" -> {
                            storage.clearHistory()
                            WebStorage.getInstance().deleteAllData()
                            CookieManager.getInstance().removeAllCookies(null)
                            getActiveTab()?.webView?.clearCache(true)
                            Toast.makeText(this@MainActivity, "Кэш, куки и история очищены", Toast.LENGTH_SHORT).show()
                        }
                    }
                }
            }
        }
    }

    private fun startAndroidDownload(url: String, userAgent: String, contentDisposition: String, mimeType: String) {
        runCatching {
            val filename = URLUtil.guessFileName(url, contentDisposition, mimeType)
            val req = DownloadManager.Request(Uri.parse(url)).apply {
                setMimeType(mimeType)
                addRequestHeader("User-Agent", userAgent)
                CookieManager.getInstance().getCookie(url)?.let { addRequestHeader("Cookie", it) }
                setTitle(filename)
                setDescription("Загрузка файла в Ferro")
                setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, filename)
            }
            val dm = getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager
            dm.enqueue(req)
            storage.addDownload(filename, url)
            Toast.makeText(this, "Загрузка начата: $filename", Toast.LENGTH_SHORT).show()
        }.onFailure {
            Toast.makeText(this, "Не удалось начать загрузку", Toast.LENGTH_SHORT).show()
        }
    }

    private fun cleanTrackingUrl(rawUrl: String): String {
        return runCatching {
            val u = Uri.parse(rawUrl)
            val b = u.buildUpon().clearQuery()
            for (name in u.queryParameterNames) {
                if (!TRACKING_PARAMS.contains(name.lowercase()) && !name.lowercase().startsWith("utm_")) {
                    u.getQueryParameters(name).forEach { v -> b.appendQueryParameter(name, v) }
                }
            }
            b.build().toString()
        }.getOrDefault(rawUrl)
    }

    private fun copyText(text: String, toastMsg: String) {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("Ferro", text))
        if (toastMsg.isNotBlank()) {
            Toast.makeText(this, toastMsg, Toast.LENGTH_SHORT).show()
        }
    }

    private fun shareUrl(url: String) {
        if (url.isBlank() || url.startsWith("https://ferro.local")) return
        val intent = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, url)
        }
        startActivity(Intent.createChooser(intent, "Поделиться ссылкой"))
    }

    private fun hideKeyboard(view: View) {
        val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imm.hideSoftInputFromWindow(view.windowToken, 0)
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQ_FILE_CHOOSER) {
            val results = if (resultCode == RESULT_OK && data?.data != null) arrayOf(data.data!!) else null
            fileUploadCallback?.onReceiveValue(results)
            fileUploadCallback = null
        } else if (requestCode == REQ_LENS_GALLERY && resultCode == RESULT_OK) {
            val uri = data?.data ?: return
            runCatching {
                contentResolver.openInputStream(uri)?.use { stream ->
                    val orig = BitmapFactory.decodeStream(stream) ?: return
                    val maxSide = 1100
                    val scaled = if (orig.width > maxSide || orig.height > maxSide) {
                        val ratio = min(maxSide.toFloat() / orig.width, maxSide.toFloat() / orig.height)
                        android.graphics.Bitmap.createScaledBitmap(
                            orig,
                            (orig.width * ratio).toInt().coerceAtLeast(1),
                            (orig.height * ratio).toInt().coerceAtLeast(1),
                            true
                        )
                    } else orig
                    val out = ByteArrayOutputStream()
                    scaled.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out)
                    val b64 = Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
                    openGoogleLensBottomSheet(b64Png = b64, imageUrl = "")
                }
            }
        }
    }
}
