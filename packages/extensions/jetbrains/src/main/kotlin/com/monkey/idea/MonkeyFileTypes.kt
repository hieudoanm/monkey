package com.monkey.idea

import com.intellij.openapi.fileTypes.LanguageFileType
import com.intellij.openapi.util.IconLoader
import javax.swing.Icon

class MonkeyCFileType private constructor() : LanguageFileType(MonkeyCLanguage.INSTANCE) {
    override fun getName() = "Monkey C"
    override fun getDescription() = "Monkey C source file"
    override fun getDefaultExtension() = "mc"
    override fun getIcon(): Icon = ICON

    companion object {
        @JvmField
        val MONKEY_C_FILE_TYPE = MonkeyCFileType()
        private val ICON: Icon =
            IconLoader.getIcon("/icons/monkey-c.png", MonkeyCFileType::class.java)
    }
}

class JungleFileType private constructor() : LanguageFileType(JungleLanguage.INSTANCE) {
    override fun getName() = "Jungle"
    override fun getDescription() = "Connect IQ Jungle project file"
    override fun getDefaultExtension() = "jungle"
    override fun getIcon(): Icon = ICON

    companion object {
        @JvmField
        val JUNGLE_FILE_TYPE = JungleFileType()
        private val ICON: Icon =
            IconLoader.getIcon("/icons/monkey-c.png", JungleFileType::class.java)
    }
}
