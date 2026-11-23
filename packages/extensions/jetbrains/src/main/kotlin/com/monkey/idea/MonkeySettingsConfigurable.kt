package com.monkey.idea

import com.intellij.openapi.options.Configurable
import com.intellij.openapi.options.ConfigurationException
import com.intellij.openapi.ui.TextFieldWithBrowseButton
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBCheckBox
import com.intellij.util.ui.FormBuilder
import java.awt.BorderLayout
import javax.swing.JComponent
import javax.swing.JPanel

class MonkeySettingsConfigurable : Configurable {
    private val cliPathField = TextFieldWithBrowseButton().apply {
        text = MonkeySettings.cliPath
    }
    private val lintCheckBox = JBCheckBox("Run lint inspections", MonkeySettings.lintEnabled)
    private var panel: JPanel? = null

    override fun getDisplayName() = "Monkey C"

    override fun createComponent(): JComponent {
        val form = FormBuilder.createFormBuilder()
            .addLabeledComponent(JBLabel("monkey CLI path:"), cliPathField)
            .addComponent(lintCheckBox)
            .addComponentFillVertically(JPanel(), 0)
            .panel
        panel = JPanel(BorderLayout()).apply { add(form, BorderLayout.NORTH) }
        return panel!!
    }

    override fun isModified(): Boolean =
        cliPathField.text != MonkeySettings.cliPath ||
            lintCheckBox.isSelected != MonkeySettings.lintEnabled

    @Throws(ConfigurationException::class)
    override fun apply() {
        MonkeySettings.cliPath = cliPathField.text.trim().ifEmpty { "monkey" }
        MonkeySettings.lintEnabled = lintCheckBox.isSelected
    }

    override fun reset() {
        cliPathField.text = MonkeySettings.cliPath
        lintCheckBox.isSelected = MonkeySettings.lintEnabled
    }

    override fun disposeUIResources() {
        panel = null
    }
}
