package com.monkey.idea

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage

@State(name = "MonkeySettings", storages = [Storage("monkey.xml")])
class MonkeySettings : PersistentStateComponent<MonkeySettings> {
    var cliPath: String = "monkey"
    var lintEnabled: Boolean = true

    override fun getState(): MonkeySettings = this
    override fun loadState(state: MonkeySettings) {
        cliPath = state.cliPath
        lintEnabled = state.lintEnabled
    }

    companion object {
        val instance: MonkeySettings
            get() = ApplicationManager.getApplication().getService(MonkeySettings::class.java)

        var cliPath: String
            get() = instance.cliPath
            set(value) {
                instance.cliPath = value
            }

        var lintEnabled: Boolean
            get() = instance.lintEnabled
            set(value) {
                instance.lintEnabled = value
            }
    }
}
