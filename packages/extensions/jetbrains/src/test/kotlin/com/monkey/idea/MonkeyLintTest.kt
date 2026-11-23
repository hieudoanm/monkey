package com.monkey.idea

import com.intellij.psi.TokenType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MonkeyLintTest {

    @Test
    fun parsesDiagnosticsJson() {
        val json = """[
            {"path":"a.mc","line":3,"column":5,"severity":"error","code":"lint001","message":"bad thing","span":{"start":10,"end":12},"notes":[],"help":"try this","suggestions":[]},
            {"path":"a.mc","line":null,"column":null,"severity":"hint","code":"fmt","message":"format drift","span":null,"notes":[],"help":null,"suggestions":[]}
        ]"""
        val diags = MonkeyLint.parse(json)
        assertEquals(2, diags.size)
        assertEquals(3, diags[0].line)
        assertEquals(5, diags[0].column)
        assertEquals("error", diags[0].severity)
        assertEquals("lint001", diags[0].code)
        assertEquals("bad thing", diags[0].message)
        assertEquals("try this", diags[0].help)
        assertEquals(null, diags[1].line)
        assertEquals("hint", diags[1].severity)
    }

    @Test
    fun parseToleratesBadJson() {
        assertTrue(MonkeyLint.parse("not json").isEmpty())
        assertTrue(MonkeyLint.parse("").isEmpty())
    }

    @Test
    fun offsetAtConvertsLineCol() {
        val text = "ab\ncd\nef"
        assertEquals(0, MonkeyLint.offsetAt(text, 1, 1))
        assertEquals(3, MonkeyLint.offsetAt(text, 2, 1))
        assertEquals(4, MonkeyLint.offsetAt(text, 2, 2))
        assertEquals(6, MonkeyLint.offsetAt(text, 3, 1))
        // Past end clamps.
        assertEquals(text.length, MonkeyLint.offsetAt(text, 9, 9))
    }

    @Test
    fun highlightMapping() {
        assertEquals(
            com.intellij.codeInspection.ProblemHighlightType.ERROR,
            MonkeyLint.highlightOf("error"),
        )
        assertEquals(
            com.intellij.codeInspection.ProblemHighlightType.WEAK_WARNING,
            MonkeyLint.highlightOf("hint"),
        )
    }
}

class MonkeyCLexerTest {

    private fun lexAll(src: String): List<Pair<String, String>> {
        val lexer = MonkeyCLexer(src)
        lexer.start(src, 0, src.length, 0)
        val out = mutableListOf<Pair<String, String>>()
        while (lexer.tokenType != null) {
            out += (lexer.tokenType.toString()) to
                src.substring(lexer.tokenStart, lexer.tokenEnd)
            lexer.advance()
        }
        return out
    }

    @Test
    fun lexesKeywordsAndIdents() {
        val toks = lexAll("function foo() { return 1; }")
        val keywords = toks.filter { it.first.contains("KEYWORD") }.map { it.second }
        assertTrue(keywords.containsAll(listOf("function", "return")))
        assertTrue(toks.any { it.first.contains("IDENT") && it.second == "foo" })
    }

    @Test
    fun lexesCommentsStringsNumbers() {
        val toks = lexAll("// hi\nvar s = \"a\\\"b\"; var n = 0x1F;")
        assertTrue(toks.any { it.first.contains("LINE_COMMENT") && it.second == "// hi" })
        assertTrue(toks.any { it.first.contains("STRING") })
        assertTrue(toks.any { it.first.contains("NUMBER") && it.second == "0x1F" })
    }

    @Test
    fun lexesOperatorsLongestMatch() {
        val toks = lexAll("a >>>= b; a == b;")
        val ops = toks.filter { it.first.contains("OPERATOR") }.map { it.second }
        assertTrue(ops.contains(">>>="))
        assertTrue(ops.contains("=="))
    }

    @Test
    fun lexesWhitespace() {
        val toks = lexAll("a b")
        assertTrue(toks.any { it.first == TokenType.WHITE_SPACE.toString() })
    }
}

class JungleLexerTest {

    private fun lexAll(src: String): List<Pair<String, String>> {
        val lexer = JungleLexer(src)
        lexer.start(src, 0, src.length, 0)
        val out = mutableListOf<Pair<String, String>>()
        while (lexer.tokenType != null) {
            out += (lexer.tokenType.toString()) to
                src.substring(lexer.tokenStart, lexer.tokenEnd)
            lexer.advance()
        }
        return out
    }

    @Test
    fun lexesKeyValueLines() {
        val src = "# comment\nproject.manifest = manifest.xml\nfoo=bar\n"
        val toks = lexAll(src)
        assertTrue(toks.any { it.first.contains("COMMENT") })
        assertTrue(toks.any { it.first.contains("KEY") && it.second == "project.manifest" })
        assertTrue(toks.any { it.first.contains("OPERATOR") && it.second == "=" })
        assertTrue(toks.any { it.first.contains("VALUE") && it.second.trim() == "manifest.xml" })
        // Second line's key is also KEY, not VALUE.
        assertTrue(toks.any { it.first.contains("KEY") && it.second == "foo" })
    }
}
