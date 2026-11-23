package com.monkey.idea

import com.intellij.lang.Language

class MonkeyCLanguage private constructor() : Language("MonkeyC") {
    companion object {
        @JvmField
        val INSTANCE = MonkeyCLanguage()
    }
}

class JungleLanguage private constructor() : Language("Jungle") {
    companion object {
        @JvmField
        val INSTANCE = JungleLanguage()
    }
}
