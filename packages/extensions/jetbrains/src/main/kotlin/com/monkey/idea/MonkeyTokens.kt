package com.monkey.idea

import com.intellij.psi.tree.IElementType

/** Token types produced by [MonkeyCLexer]. */
object MonkeyCTokens {
    val IDENT = IElementType("MONKEYC_IDENT", MonkeyCLanguage.INSTANCE)
    val KEYWORD = IElementType("MONKEYC_KEYWORD", MonkeyCLanguage.INSTANCE)
    val NUMBER = IElementType("MONKEYC_NUMBER", MonkeyCLanguage.INSTANCE)
    val STRING = IElementType("MONKEYC_STRING", MonkeyCLanguage.INSTANCE)
    val CHAR = IElementType("MONKEYC_CHAR", MonkeyCLanguage.INSTANCE)
    val LINE_COMMENT = IElementType("MONKEYC_LINE_COMMENT", MonkeyCLanguage.INSTANCE)
    val BLOCK_COMMENT = IElementType("MONKEYC_BLOCK_COMMENT", MonkeyCLanguage.INSTANCE)
    val OPERATOR = IElementType("MONKEYC_OPERATOR", MonkeyCLanguage.INSTANCE)
    val BAD_CHARACTER = IElementType("MONKEYC_BAD_CHARACTER", MonkeyCLanguage.INSTANCE)
}

/** Token types produced by [JungleLexer]. */
object JungleTokens {
    val KEY = IElementType("JUNGLE_KEY", JungleLanguage.INSTANCE)
    val VALUE = IElementType("JUNGLE_VALUE", JungleLanguage.INSTANCE)
    val OPERATOR = IElementType("JUNGLE_OPERATOR", JungleLanguage.INSTANCE)
    val COMMENT = IElementType("JUNGLE_COMMENT", JungleLanguage.INSTANCE)
}
