package com.monkey.idea

import com.google.gson.JsonParser
import com.intellij.codeInspection.LocalInspectionTool
import com.intellij.codeInspection.ProblemHighlightType
import com.intellij.codeInspection.ProblemsHolder
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiElementVisitor
import com.intellij.psi.PsiFile
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.util.concurrent.TimeUnit

/** One diagnostic parsed from `monkey lint --stdin` JSON output. */
data class MonkeyDiagnostic(
    val line: Int?,
    val column: Int?,
    val severity: String,
    val code: String,
    val message: String,
    val help: String?,
)

/**
 * Runs `monkey lint --stdin <name>` and parses the JSON diagnostics array.
 * Kept separate from the inspection so it is unit-testable without the IDE.
 */
object MonkeyLint {

    /** Lint [text] as if it were a file named [fileName]. Empty on failure. */
    fun check(cliPath: String, text: String, fileName: String): List<MonkeyDiagnostic> {
        return try {
            val pb = ProcessBuilder(cliPath, "lint", "--stdin", fileName)
                .redirectErrorStream(false)
            val proc = pb.start()
            OutputStreamWriter(proc.outputStream, Charsets.UTF_8).use { it.write(text) }
            val stdout = BufferedReader(InputStreamReader(proc.inputStream, Charsets.UTF_8))
                .readText()
            proc.waitFor(30, TimeUnit.SECONDS)
            parse(stdout)
        } catch (_: Exception) {
            emptyList()
        }
    }

    /** Parse the `diagnostics_json_all` array shape into [MonkeyDiagnostic]s. */
    fun parse(json: String): List<MonkeyDiagnostic> {
        val arr = try {
            JsonParser.parseString(json).asJsonArray
        } catch (_: Exception) {
            return emptyList()
        }
        return arr.mapNotNull { el ->
            val o = el.asJsonObject
            val severity = o.get("severity")?.asString ?: "warning"
            val code = o.get("code")?.asString ?: ""
            val message = o.get("message")?.asString ?: ""
            val help = o.get("help")?.takeIf { !it.isJsonNull }?.asString
            MonkeyDiagnostic(
                line = o.get("line")?.takeIf { !it.isJsonNull }?.asInt,
                column = o.get("column")?.takeIf { !it.isJsonNull }?.asInt,
                severity = severity,
                code = code,
                message = message,
                help = help,
            )
        }
    }

    fun highlightOf(severity: String): ProblemHighlightType = when (severity) {
        "error" -> ProblemHighlightType.ERROR
        "warning" -> ProblemHighlightType.WARNING
        else -> ProblemHighlightType.WEAK_WARNING
    }

    /** 1-based line/char-column → 0-based char offset in [text]. */
    fun offsetAt(text: String, line: Int, column: Int): Int {
        var l = 1
        var i = 0
        while (i < text.length && l < line) {
            if (text[i] == '\n') l++
            i++
        }
        var col = 1
        while (i < text.length && col < column && text[i] != '\n') {
            i++
            col++
        }
        return i.coerceIn(0, text.length)
    }
}

/**
 * On-the-fly inspection that surfaces `monkey lint` diagnostics for Monkey C
 * and Jungle files, including unsaved editor contents.
 */
open class MonkeyLintInspection : LocalInspectionTool() {

    override fun getShortName() = "MonkeyLint"

    override fun buildVisitor(
        holder: ProblemsHolder,
        isOnTheFly: Boolean,
    ): PsiElementVisitor = object : PsiElementVisitor() {
        override fun visitFile(file: PsiFile) {
            if (!MonkeySettings.lintEnabled) return
            val text = file.text
            val diags = MonkeyLint.check(MonkeySettings.cliPath, text, file.name)
            for (d in diags) {
                val message = buildString {
                    append("[${d.code}] ").append(d.message)
                    d.help?.let { append(" — ").append(it) }
                }
                val highlight = MonkeyLint.highlightOf(d.severity)
                val line = d.line
                val element: PsiElement? = if (line != null) {
                    file.findElementAt(
                        MonkeyLint.offsetAt(text, line, d.column ?: 1),
                    )
                } else {
                    null
                }
                if (element != null) {
                    holder.registerProblem(element, message, highlight)
                } else {
                    // No usable span — annotate at file level.
                    holder.registerProblem(file, message, highlight)
                }
            }
        }
    }
}

/** Same lint pass for Jungle files (registered under a separate short name). */
class JungleLintInspection : MonkeyLintInspection() {
    override fun getShortName() = "MonkeyLintJungle"
}
