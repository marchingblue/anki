package com.klepci.app

import java.io.File

/**
 * Owns the Rust core. The UniFFI-generated `MobileCore` class lives in
 * `app/src/main/java/uniffi/klep_mobile/` (generated, see README).
 * `libklep_mobile.so` is built from `klep-mobile/` with the NDK.
 */
object Klep {
    @Volatile private var core: uniffi.klep_mobile.MobileCore? = null

    fun open(filesDir: File): uniffi.klep_mobile.MobileCore {
        core?.let { return it }
        return synchronized(this) {
            core?.let { return it }
            System.loadLibrary("klep_mobile")
            val fresh = uniffi.klep_mobile.MobileCore.open(
                File(filesDir, "collection.anki2").absolutePath
            )
            core = fresh
            fresh
        }
    }
}

/** Card HTML to readable text (same rules as the desktop client). */
fun htmlToText(html: String): String {
    var out = html
        .replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n")
        .replace("</p>", "\n").replace("</div>", "\n").replace("</li>", "\n")
        .replace("<li>", "\n• ")
    while (true) {
        val s = out.indexOf("[sound:")
        if (s < 0) break
        val e = out.indexOf(']', s)
        if (e < 0) break
        out = out.substring(0, s) + "🔊" + out.substring(e + 1)
    }
    while (true) {
        val s = out.indexOf("<img")
        if (s < 0) break
        val e = out.indexOf('>', s)
        if (e < 0) break
        out = out.substring(0, s) + "[image]" + out.substring(e + 1)
    }
    val text = StringBuilder()
    var inside = false
    for (ch in out) {
        when {
            ch == '<' -> inside = true
            ch == '>' -> inside = false
            !inside -> text.append(ch)
        }
    }
    return text.toString()
        .replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&amp;", "&").replace("&quot;", "\"")
        .lines().joinToString("\n") { it.trimEnd() }.trim()
}

fun formatSecs(s: Double): String =
    if (s < 60) "${"%.1f".format(s)}s" else "${"%.1f".format(s / 60)}m"
