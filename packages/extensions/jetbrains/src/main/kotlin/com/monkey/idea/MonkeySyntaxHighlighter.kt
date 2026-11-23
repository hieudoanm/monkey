package com.monkey.idea

import com.intellij.openapi.editor.colors.TextAttributesKey
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterBase
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.lexer.Lexer
import com.intellij.openapi.editor.DefaultLanguageHighlighterColors
import com.intellij.psi.tree.IElementType

private val MONKEY_KEYWORD = TextAttributesKey.createTextAttributesKey(
    "MONKEYC_KEYWORD", DefaultLanguageHighlighterColors.KEYWORD)
private val MONKEY_NUMBER = TextAttributesKey.createTextAttributesKey(
    "MONKEYC_NUMBER", DefaultLanguageHighlighterColors.NUMBER)
private val MONKEY_STRING = TextAttributesKey.createTextAttributesKey(
    "MONKEYC_STRING", DefaultLanguageHighlighterColors.STRING)
private val MONKEY_LINE_COMMENT = TextAttributesKey.createTextAttributesKey(
    "MONKEYC_LINE_COMMENT", DefaultLanguageHighlighterColors.LINE_COMMENT)
private val MONKEY_BLOCK_COMMENT = TextAttributesKey.createTextAttributesKey(
    "MONKEYC_BLOCK_COMMENT", DefaultLanguageHighlighterColors.BLOCK_COMMENT)
private val MONKEY_OPERATOR = TextAttributesKey.createTextAttributesKey(
    "MONKEYC_OPERATOR", DefaultLanguageHighlighterColors.OPERATION_SIGN)
private val MONKEY_BAD = TextAttributesKey.createTextAttributesKey(
    "MONKEYC_BAD_CHARACTER", DefaultLanguageHighlighterColors.INVALID_STRING_ESCAPE)

private val JUNGLE_KEY = TextAttributesKey.createTextAttributesKey(
    "JUNGLE_KEY", DefaultLanguageHighlighterColors.IDENTIFIER)
private val JUNGLE_VALUE = TextAttributesKey.createTextAttributesKey(
    "JUNGLE_VALUE", DefaultLanguageHighlighterColors.STRING)
private val JUNGLE_OPERATOR = TextAttributesKey.createTextAttributesKey(
    "JUNGLE_OPERATOR", DefaultLanguageHighlighterColors.OPERATION_SIGN)
private val JUNGLE_COMMENT = TextAttributesKey.createTextAttributesKey(
    "JUNGLE_COMMENT", DefaultLanguageHighlighterColors.LINE_COMMENT)

private val MONKEYC_HIGHLIGHTS: Map<IElementType, Array<TextAttributesKey>> = mapOf(
    MonkeyCTokens.KEYWORD to arrayOf(MONKEY_KEYWORD),
    MonkeyCTokens.NUMBER to arrayOf(MONKEY_NUMBER),
    MonkeyCTokens.STRING to arrayOf(MONKEY_STRING),
    MonkeyCTokens.CHAR to arrayOf(MONKEY_STRING),
    MonkeyCTokens.LINE_COMMENT to arrayOf(MONKEY_LINE_COMMENT),
    MonkeyCTokens.BLOCK_COMMENT to arrayOf(MONKEY_BLOCK_COMMENT),
    MonkeyCTokens.OPERATOR to arrayOf(MONKEY_OPERATOR),
    MonkeyCTokens.BAD_CHARACTER to arrayOf(MONKEY_BAD),
)

private val JUNGLE_HIGHLIGHTS: Map<IElementType, Array<TextAttributesKey>> = mapOf(
    JungleTokens.KEY to arrayOf(JUNGLE_KEY),
    JungleTokens.VALUE to arrayOf(JUNGLE_VALUE),
    JungleTokens.OPERATOR to arrayOf(JUNGLE_OPERATOR),
    JungleTokens.COMMENT to arrayOf(JUNGLE_COMMENT),
)

private class MonkeyHighlighter(
    private val lexerFactory: (CharSequence) -> Lexer,
    private val highlights: Map<IElementType, Array<TextAttributesKey>>,
) : SyntaxHighlighterBase() {
    // A fresh lexer per call: the platform start()s it with the real buffer.
    override fun getHighlightingLexer(): Lexer = lexerFactory("")

    override fun getTokenHighlights(tokenType: IElementType?): Array<TextAttributesKey> =
        tokenType?.let { highlights[it] } ?: emptyArray()
}

class MonkeyCSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    private val highlighter: SyntaxHighlighter =
        MonkeyHighlighter(::MonkeyCLexer, MONKEYC_HIGHLIGHTS)

    override fun getSyntaxHighlighter(
        project: com.intellij.openapi.project.Project?,
        virtualFile: com.intellij.openapi.vfs.VirtualFile?,
    ): SyntaxHighlighter = highlighter
}

class JungleSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    private val highlighter: SyntaxHighlighter =
        MonkeyHighlighter({ JungleLexer(it) }, JUNGLE_HIGHLIGHTS)

    override fun getSyntaxHighlighter(
        project: com.intellij.openapi.project.Project?,
        virtualFile: com.intellij.openapi.vfs.VirtualFile?,
    ): SyntaxHighlighter = highlighter
}
