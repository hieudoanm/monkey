package com.monkey.idea

import com.intellij.lexer.Lexer
import com.intellij.psi.TokenType
import com.intellij.psi.tree.IElementType

/**
 * Hand-written lexer for Monkey C (`.mc`, `.mb`, `.mcgen`). Mirrors the token
 * model of `crates/monkey-syntax`: identifiers/keywords, numbers, strings,
 * char literals, line/block comments, and longest-match operators.
 */
class MonkeyCLexer(buffer: CharSequence) : AbstractMonkeyLexer(buffer) {

    override fun findToken(): IElementType? {
        if (eof()) return null
        val c = peekAt(0)!!
        return when {
            c == ' ' || c == '\t' || c == '\r' || c == '\n' -> scanWhitespace()
            c == '/' && peekAt(1) == '/' -> scanLineComment()
            c == '/' && peekAt(1) == '*' -> scanBlockComment()
            c == '"' -> scanLiteral('"', MonkeyCTokens.STRING)
            c == '\'' -> scanLiteral('\'', MonkeyCTokens.CHAR)
            c.isDigit() || (c == '.' && peekAt(1)?.isDigit() == true) -> scanNumber()
            c.isLetter() || c == '_' -> scanIdent()
            else -> scanOperator()
        }
    }

    private fun scanLineComment(): IElementType {
        bump(); bump() // '//'
        while (!eof() && peekAt(0) != '\n') bump()
        return MonkeyCTokens.LINE_COMMENT
    }

    private fun scanBlockComment(): IElementType {
        bump(); bump() // '/*'
        while (!eof()) {
            if (peekAt(0) == '*' && peekAt(1) == '/') {
                bump(); bump()
                return MonkeyCTokens.BLOCK_COMMENT
            }
            bump()
        }
        return MonkeyCTokens.BLOCK_COMMENT
    }

    private fun scanLiteral(quote: Char, type: IElementType): IElementType {
        bump() // opening quote
        while (!eof()) {
            val c = peekAt(0)!!
            bump()
            if (c == '\\') {
                if (!eof()) bump() // escaped char
            } else if (c == quote) {
                break
            }
        }
        return type
    }

    private fun scanNumber(): IElementType {
        // Digits, hex letters, '.', '_', and exponent signs — permissive like
        // the Rust lexer; this only drives highlighting.
        takeWhile { it.isLetterOrDigit() || it == '.' || it == '_' }
        return MonkeyCTokens.NUMBER
    }

    private fun scanIdent(): IElementType {
        val start = offset
        takeWhile { it.isLetterOrDigit() || it == '_' }
        val text = getBufferSequence().subSequence(start, offset).toString()
        return if (text in KEYWORDS) MonkeyCTokens.KEYWORD else MonkeyCTokens.IDENT
    }

    private fun scanOperator(): IElementType {
        for (op in MULTI_PUNCT) {
            if (startsWith(op)) {
                repeat(op.length) { bump() }
                return MonkeyCTokens.OPERATOR
            }
        }
        if (peekAt(0) in SINGLE_PUNCT) {
            bump()
            return MonkeyCTokens.OPERATOR
        }
        bump()
        return MonkeyCTokens.BAD_CHARACTER
    }

    companion object {
        /** Mirrors `KEYWORDS` in `crates/monkey-syntax/src/token.rs`. */
        private val KEYWORDS = setOf(
            "and", "as", "break", "case", "catch", "class", "const", "continue",
            "default", "do", "else", "enum", "extends", "false", "final",
            "finally", "for", "function", "has", "hidden", "if", "import", "in",
            "instanceof", "module", "new", "not", "null", "or", "private",
            "protected", "public", "return", "super", "switch", "this", "typedef",
            "throw", "true", "try", "var", "void", "while",
        )

        /** Longest first — mirrors `MULTI_PUNCT` in the Rust lexer. */
        private val MULTI_PUNCT = listOf(
            ">>>=", ">>>", "<<=", ">>=", "==", "!=", "<=", ">=", "&&", "||",
            "++", "--", "+=", "-=", "*=", "/=", "%=", "<<", ">>", "&=", "|=",
            "^=", "=>",
        )

        private val SINGLE_PUNCT = setOf(
            '(', ')', '{', '}', '[', ']', '<', '>', ',', ';', '.', ':', '+',
            '-', '*', '/', '%', '=', '!', '&', '|', '^', '~', '?', '@', '$',
        )
    }
}

/** Lexer for Jungle (`*.jungle`) `key = value` project files. */
class JungleLexer(buffer: CharSequence) : AbstractMonkeyLexer(buffer) {
    private var expectKey = true

    override fun findToken(): IElementType? {
        if (eof()) return null
        val c = peekAt(0)!!
        return when {
            c == ' ' || c == '\t' || c == '\r' || c == '\n' -> {
                val wsStart = offset
                val ws = scanWhitespace()
                // Only a newline ends the logical line; horizontal whitespace
                // keeps the current key/value position.
                val consumed = getBufferSequence().subSequence(wsStart, offset)
                if (consumed.contains('\n')) expectKey = true
                ws
            }
            c == '#' -> scanComment()
            expectKey && isKeyChar(c) -> scanKey()
            c == '=' -> {
                bump()
                expectKey = false
                JungleTokens.OPERATOR
            }
            else -> scanValue()
        }
    }

    private fun scanComment(): IElementType {
        while (!eof() && peekAt(0) != '\n') bump()
        return JungleTokens.COMMENT
    }

    private fun scanKey(): IElementType {
        takeWhile { isKeyChar(it) }
        expectKey = false
        return JungleTokens.KEY
    }

    private fun scanValue(): IElementType {
        // Consume to end of line or comment; trailing whitespace stays inside
        // the token, which is harmless for highlighting.
        while (!eof() && peekAt(0) != '\n' && peekAt(0) != '#') bump()
        expectKey = false
        return JungleTokens.VALUE
    }

    private fun isKeyChar(c: Char) =
        c.isLetterOrDigit() || c == '_' || c == '.' || c == '-' || c == '/'

    companion object {
        fun create(buffer: CharSequence): Lexer = JungleLexer(buffer)
    }
}
