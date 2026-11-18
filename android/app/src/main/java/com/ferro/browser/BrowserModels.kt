package com.ferro.browser

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

data class MobileSettings(
    var themeMode: String = "dark",
    var accentColor: String = "#ececec",
    var cornerRadius: Int = 10,
    var barPosition: String = "bottom", // "bottom" | "top"
    var searchEngine: String = "google",
    var adblockEnabled: Boolean = true,
    var offerSavePasswords: Boolean = true,
    var wallpaperPreset: String = "none",
    var wallpaperUrl: String = "",
    var wallpaperBlur: Int = 0,
    var wallpaperBrightness: Int = 80,
    var restoreSession: Boolean = true
) {
    fun toJson(): JSONObject = JSONObject().apply {
        put("theme_mode", themeMode)
        put("accent_color", accentColor)
        put("corner_radius", cornerRadius)
        put("bar_position", barPosition)
        put("search_engine", searchEngine)
        put("adblock_enabled", adblockEnabled)
        put("offer_save_passwords", offerSavePasswords)
        put("wallpaper_preset", wallpaperPreset)
        put("wallpaper_url", wallpaperUrl)
        put("wallpaper_blur", wallpaperBlur)
        put("wallpaper_brightness", wallpaperBrightness)
        put("restore_session", restoreSession)
    }

    companion object {
        fun fromJson(o: JSONObject): MobileSettings = MobileSettings(
            themeMode = o.optString("theme_mode", "dark"),
            accentColor = o.optString("accent_color", "#ececec"),
            cornerRadius = o.optInt("corner_radius", 10).coerceIn(0, 20),
            barPosition = o.optString("bar_position", "bottom"),
            searchEngine = o.optString("search_engine", "google"),
            adblockEnabled = o.optBoolean("adblock_enabled", true),
            offerSavePasswords = o.optBoolean("offer_save_passwords", true),
            wallpaperPreset = o.optString("wallpaper_preset", "none"),
            wallpaperUrl = o.optString("wallpaper_url", ""),
            wallpaperBlur = o.optInt("wallpaper_blur", 0).coerceIn(0, 24),
            wallpaperBrightness = o.optInt("wallpaper_brightness", 80).coerceIn(20, 100),
            restoreSession = o.optBoolean("restore_session", true)
        )
    }
}

data class BookmarkItem(val title: String, val url: String)
data class HistoryItem(val id: Long, val title: String, val url: String, val timestamp: Long)
data class PasswordItem(val id: Long, var origin: String, var username: String, var password: String, var createdAt: Long)
data class DownloadRecord(val id: Long, val filename: String, val url: String, val timestamp: Long)

class BrowserStorage(private val rootDir: File) {
    val settingsFile = File(rootDir, "settings.json")
    private val bookmarksFile = File(rootDir, "bookmarks.json")
    private val historyFile = File(rootDir, "history.json")
    private val passwordsFile = File(rootDir, "passwords.json")
    private val downloadsFile = File(rootDir, "downloads.json")
    private val sessionFile = File(rootDir, "session.json")

    var settings: MobileSettings = loadSettings()
    val bookmarks: MutableList<BookmarkItem> = loadBookmarks()
    val history: MutableList<HistoryItem> = loadHistory()
    val passwords: MutableList<PasswordItem> = loadPasswords()
    val downloads: MutableList<DownloadRecord> = loadDownloads()

    @Synchronized
    private fun loadSettings(): MobileSettings {
        return try {
            if (settingsFile.exists()) MobileSettings.fromJson(JSONObject(settingsFile.readText()))
            else MobileSettings()
        } catch (_: Exception) {
            MobileSettings()
        }
    }

    @Synchronized
    fun saveSettings(newSettings: MobileSettings) {
        settings = newSettings
        runCatching { settingsFile.writeText(settings.toJson().toString(2)) }
    }

    @Synchronized
    private fun loadBookmarks(): MutableList<BookmarkItem> {
        val list = mutableListOf<BookmarkItem>()
        if (bookmarksFile.exists()) {
            runCatching {
                val arr = JSONArray(bookmarksFile.readText())
                for (i in 0 until arr.length()) {
                    val o = arr.getJSONObject(i)
                    list.add(BookmarkItem(o.optString("title"), o.optString("url")))
                }
                return list
            }
        }
        list.addAll(
            listOf(
                BookmarkItem("Google", "https://www.google.com"),
                BookmarkItem("YouTube", "https://m.youtube.com"),
                BookmarkItem("GitHub", "https://github.com"),
                BookmarkItem("Reddit", "https://www.reddit.com"),
                BookmarkItem("Пароли", "https://ferro.local/passwords"),
                BookmarkItem("История", "https://ferro.local/history"),
                BookmarkItem("ArchWiki", "https://wiki.archlinux.org")
            )
        )
        saveBookmarksList(list)
        return list
    }

    @Synchronized
    private fun saveBookmarksList(list: List<BookmarkItem>) {
        val arr = JSONArray()
        list.forEach {
            arr.put(JSONObject().put("title", it.title).put("url", it.url))
        }
        runCatching { bookmarksFile.writeText(arr.toString(2)) }
    }

    @Synchronized
    fun isBookmarked(url: String): Boolean = bookmarks.any { it.url == url }

    @Synchronized
    fun toggleBookmark(url: String, title: String): Boolean {
        val idx = bookmarks.indexOfFirst { it.url == url }
        val added = if (idx >= 0) {
            bookmarks.removeAt(idx)
            false
        } else {
            bookmarks.add(0, BookmarkItem(title.ifBlank { url }, url))
            true
        }
        saveBookmarksList(bookmarks)
        return added
    }

    @Synchronized
    fun bookmarksJson(): JSONArray {
        val arr = JSONArray()
        bookmarks.forEach { arr.put(JSONObject().put("title", it.title).put("url", it.url)) }
        return arr
    }

    @Synchronized
    private fun loadHistory(): MutableList<HistoryItem> {
        val list = mutableListOf<HistoryItem>()
        if (historyFile.exists()) {
            runCatching {
                val arr = JSONArray(historyFile.readText())
                for (i in 0 until arr.length()) {
                    val o = arr.getJSONObject(i)
                    list.add(
                        HistoryItem(
                            o.optLong("id"),
                            o.optString("title"),
                            o.optString("url"),
                            o.optLong("timestamp")
                        )
                    )
                }
            }
        }
        return list
    }

    @Synchronized
    fun addHistory(url: String, title: String) {
        if (url.isBlank() || url.startsWith("https://ferro.local") || url.startsWith("ferro://") || url == "about:blank") return
        if (history.firstOrNull()?.url == url) return
        val now = System.currentTimeMillis() / 1000
        history.add(0, HistoryItem(System.nanoTime(), title.ifBlank { url }, url, now))
        if (history.size > 500) history.subList(500, history.size).clear()
        persistHistory()
    }

    @Synchronized
    fun deleteHistory(id: Long) {
        history.removeAll { it.id == id }
        persistHistory()
    }

    @Synchronized
    fun clearHistory() {
        history.clear()
        persistHistory()
    }

    private fun persistHistory() {
        val arr = JSONArray()
        history.forEach {
            arr.put(JSONObject().put("id", it.id).put("title", it.title).put("url", it.url).put("timestamp", it.timestamp))
        }
        runCatching { historyFile.writeText(arr.toString()) }
    }

    @Synchronized
    fun historyJson(): JSONArray {
        val arr = JSONArray()
        history.forEach {
            arr.put(JSONObject().put("id", it.id).put("title", it.title).put("url", it.url).put("timestamp", it.timestamp))
        }
        return arr
    }

    @Synchronized
    private fun loadPasswords(): MutableList<PasswordItem> {
        val list = mutableListOf<PasswordItem>()
        if (passwordsFile.exists()) {
            runCatching {
                val arr = JSONArray(passwordsFile.readText())
                for (i in 0 until arr.length()) {
                    val o = arr.getJSONObject(i)
                    list.add(
                        PasswordItem(
                            o.optLong("id"),
                            o.optString("origin"),
                            o.optString("username"),
                            o.optString("password"),
                            o.optLong("created_at")
                        )
                    )
                }
            }
        }
        return list
    }

    @Synchronized
    fun isPasswordAlreadySaved(origin: String, username: String, password: String): Boolean {
        val norm = normalizeOrigin(origin)
        return passwords.any {
            it.origin.equals(norm, ignoreCase = true) &&
                it.username.equals(username.trim(), ignoreCase = true) &&
                it.password == password
        }
    }

    @Synchronized
    fun savePassword(origin: String, username: String, password: String) {
        val cleanOrigin = normalizeOrigin(origin)
        if (cleanOrigin.isBlank() || password.isEmpty()) return
        val cleanUser = username.trim()
        val now = System.currentTimeMillis() / 1000
        val existing = passwords.firstOrNull {
            it.origin.equals(cleanOrigin, ignoreCase = true) && it.username.equals(cleanUser, ignoreCase = true)
        }
        if (existing != null) {
            existing.username = cleanUser
            existing.password = password
            existing.createdAt = now
        } else {
            passwords.add(0, PasswordItem(System.currentTimeMillis() + passwords.size, cleanOrigin, cleanUser, password, now))
        }
        persistPasswords()
    }

    @Synchronized
    fun updatePassword(id: Long, origin: String, username: String, password: String) {
        passwords.firstOrNull { it.id == id }?.let {
            if (origin.isNotBlank()) it.origin = normalizeOrigin(origin)
            it.username = username.trim()
            if (password.isNotEmpty()) it.password = password
            persistPasswords()
        }
    }

    @Synchronized
    fun deletePassword(id: Long) {
        passwords.removeAll { it.id == id }
        persistPasswords()
    }

    @Synchronized
    fun clearPasswords() {
        passwords.clear()
        persistPasswords()
    }

    private fun normalizeOrigin(raw: String): String {
        val t = raw.trim()
        if (t.isEmpty()) return ""
        return if (t.startsWith("http://") || t.startsWith("https://")) t else "https://$t"
    }

    private fun persistPasswords() {
        val arr = JSONArray()
        passwords.forEach {
            arr.put(
                JSONObject()
                    .put("id", it.id)
                    .put("origin", it.origin)
                    .put("username", it.username)
                    .put("password", it.password)
                    .put("created_at", it.createdAt)
            )
        }
        runCatching { passwordsFile.writeText(arr.toString(2)) }
    }

    @Synchronized
    fun passwordsJson(): JSONArray {
        val arr = JSONArray()
        passwords.forEach {
            arr.put(
                JSONObject()
                    .put("id", it.id)
                    .put("origin", it.origin)
                    .put("username", it.username)
                    .put("password", it.password)
                    .put("created_at", it.createdAt)
            )
        }
        return arr
    }

    @Synchronized
    fun importPasswordsCsv(csvText: String): Int {
        val lines = csvText.lines().filter { it.isNotBlank() }
        if (lines.isEmpty()) return 0
        val headers = parseCsvRow(lines.first()).map { it.lowercase() }
        val urlIdx = headers.indexOfFirst { it in listOf("url", "login_uri", "origin", "website") }
            .takeIf { it >= 0 } ?: if (headers.size >= 4) 1 else 0
        val userIdx = headers.indexOfFirst { it in listOf("username", "login_username", "user", "login") }
            .takeIf { it >= 0 } ?: if (headers.size >= 4) 2 else 1
        val passIdx = headers.indexOfFirst { it in listOf("password", "login_password", "pass") }
            .takeIf { it >= 0 } ?: if (headers.size >= 4) 3 else 2

        var imported = 0
        for (line in lines.drop(1)) {
            val cols = parseCsvRow(line)
            val u = cols.getOrNull(urlIdx).orEmpty()
            val p = cols.getOrNull(passIdx).orEmpty()
            val usr = cols.getOrNull(userIdx).orEmpty()
            if (u.isNotBlank() && p.isNotEmpty()) {
                savePassword(u, usr, p)
                imported++
            }
        }
        return imported
    }

    private fun parseCsvRow(line: String): List<String> {
        val out = mutableListOf<String>()
        val cur = StringBuilder()
        var inQuotes = false
        var i = 0
        while (i < line.length) {
            val ch = line[i]
            when {
                inQuotes && ch == '"' && i + 1 < line.length && line[i + 1] == '"' -> {
                    cur.append('"')
                    i++
                }
                inQuotes && ch == '"' -> inQuotes = false
                !inQuotes && ch == '"' -> inQuotes = true
                !inQuotes && ch == ',' -> {
                    out.add(cur.toString().trim())
                    cur.setLength(0)
                }
                else -> cur.append(ch)
            }
            i++
        }
        out.add(cur.toString().trim())
        return out
    }

    @Synchronized
    private fun loadDownloads(): MutableList<DownloadRecord> {
        val list = mutableListOf<DownloadRecord>()
        if (downloadsFile.exists()) {
            runCatching {
                val arr = JSONArray(downloadsFile.readText())
                for (i in 0 until arr.length()) {
                    val o = arr.getJSONObject(i)
                    list.add(DownloadRecord(o.optLong("id"), o.optString("filename"), o.optString("url"), o.optLong("timestamp")))
                }
            }
        }
        return list
    }

    @Synchronized
    fun addDownload(filename: String, url: String) {
        downloads.add(0, DownloadRecord(System.currentTimeMillis(), filename, url, System.currentTimeMillis() / 1000))
        val arr = JSONArray()
        downloads.forEach {
            arr.put(JSONObject().put("id", it.id).put("filename", it.filename).put("url", it.url).put("timestamp", it.timestamp))
        }
        runCatching { downloadsFile.writeText(arr.toString()) }
    }

    @Synchronized
    fun clearDownloads() {
        downloads.clear()
        runCatching { downloadsFile.writeText("[]") }
    }

    @Synchronized
    fun downloadsJson(): JSONArray {
        val arr = JSONArray()
        downloads.forEach {
            arr.put(JSONObject().put("id", it.id).put("filename", it.filename).put("url", it.url).put("timestamp", it.timestamp))
        }
        return arr
    }

    @Synchronized
    fun saveSessionUrls(urls: List<String>) {
        val arr = JSONArray()
        urls.forEach { arr.put(it) }
        runCatching { sessionFile.writeText(arr.toString()) }
    }

    @Synchronized
    fun loadSessionUrls(): List<String> {
        if (!sessionFile.exists()) return emptyList()
        return runCatching {
            val arr = JSONArray(sessionFile.readText())
            (0 until arr.length()).mapNotNull { arr.optString(it).takeIf { s -> s.isNotBlank() } }
        }.getOrDefault(emptyList())
    }
}
