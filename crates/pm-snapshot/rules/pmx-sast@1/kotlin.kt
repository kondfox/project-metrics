import android.content.Context
import android.webkit.WebView

class Demo(private val db: SQLiteDatabase, private val ctx: Context) {
    fun sql(id: String) {
        // ruleid: pmx-kotlin-sql-injection
        db.rawQuery("SELECT * FROM users WHERE id = '$id'", null)
        // ruleid: pmx-kotlin-sql-injection
        db.execSQL("DELETE FROM users WHERE id = " + id)
        // ruleid: pmx-kotlin-sql-injection
        db.execSQL("DELETE FROM users WHERE name = '" + id + "'")
        // ok: pmx-kotlin-sql-injection
        db.rawQuery("SELECT * FROM users WHERE id = ?", arrayOf(id))
    }

    fun run(name: String) {
        // ruleid: pmx-kotlin-command-injection
        Runtime.getRuntime().exec("git log $name")
        // ok: pmx-kotlin-command-injection
        Runtime.getRuntime().exec("git status")
    }

    fun web(view: WebView) {
        // ruleid: pmx-android-webview-js-bridge
        view.addJavascriptInterface(Bridge(), "bridge")
        // ok: pmx-android-webview-js-bridge
        view.loadUrl("https://example.com")
    }

    fun prefs() {
        // ruleid: pmx-android-world-readable
        ctx.getSharedPreferences("p", Context.MODE_WORLD_READABLE)
        // ok: pmx-android-world-readable
        ctx.getSharedPreferences("p", Context.MODE_PRIVATE)
    }
}
