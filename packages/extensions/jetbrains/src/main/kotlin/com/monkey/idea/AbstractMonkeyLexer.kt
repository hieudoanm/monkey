package com.monkey.idea

import com.intellij.lexer.LexerBase
import com.intellij.psi.TokenType
import com.intellij.psi.tree.IElementType

/**
 * Base for the hand-written Monkey C/Jungle lexers. Subclasses implement
 * [findToken], which consumes one token starting at the current offset and
 * returns its type (`null` at end of input).
 */
abstract class AbstractMonkeyLexer(initialBuffer: CharSequence) : LexerBase() {
    private var myBuffer: CharSequence = initialBuffer
    private var myOffset = 0
    private var myEndOffset = 0
    private var myTokenStart = 0
    private var myTokenEnd = 0
    private var myState = 0
    private var myTokenType: IElementType? = null

    final override fun start(
        buffer: CharSequence,
        startOffset: Int,
        endOffset: Int,
        initialState: Int,
    ) {
        myBuffer = buffer
        myOffset = startOffset
        myEndOffset = endOffset
        myTokenStart = startOffset
        myTokenEnd = startOffset
        myState = initialState
        myTokenType = null
        advance()
    }

    override fun getState(): Int = myState

    override fun getTokenType(): IElementType? = myTokenType

    override fun getTokenStart(): Int = myTokenStart

    override fun getTokenEnd(): Int = myTokenEnd

    override fun getBufferSequence(): CharSequence = myBuffer

    override fun getBufferEnd(): Int = myEndOffset

    override fun advance() {
        myTokenStart = myOffset
        myTokenType = findToken()
        myTokenEnd = myOffset
    }

    // ----- cursor helpers for subclasses -----

    protected val offset: Int get() = myOffset
    protected val endOffset: Int get() = myEndOffset

    protected fun eof(): Boolean = myOffset >= myEndOffset

    protected fun peekAt(i: Int): Char? =
        if (myOffset + i < myEndOffset) myBuffer[myOffset + i] else null

    protected fun startsWith(s: String): Boolean {
        if (myOffset + s.length > myEndOffset) return false
        for (i in s.indices) {
            if (myBuffer[myOffset + i] != s[i]) return false
        }
        return true
    }

    protected fun bump() {
        if (myOffset < myEndOffset) myOffset++
    }

    protected fun takeWhile(pred: (Char) -> Boolean) {
        while (!eof() && pred(myBuffer[myOffset])) myOffset++
    }

    protected abstract fun findToken(): IElementType?

    /** Advance past a run of whitespace, including newlines. */
    protected fun scanWhitespace(): IElementType {
        takeWhile { it == ' ' || it == '\t' || it == '\r' || it == '\n' }
        return TokenType.WHITE_SPACE
    }
}
