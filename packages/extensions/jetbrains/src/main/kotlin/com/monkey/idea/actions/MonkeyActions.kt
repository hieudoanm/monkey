package com.monkey.idea.actions

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.fileTypes.FileType
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.Messages
import com.intellij.openapi.vfs.LocalFileSystem
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.psi.PsiDocumentManager
import com.monkey.idea.JungleFileType
import com.monkey.idea.MonkeyCFileType
import com.monkey.idea.MonkeySettings
import java.io.File
import java.nio.file.Files
import java.util.concurrent.TimeUnit

private fun notify(
    project: Project,
    message: String,
    type: NotificationType = NotificationType.INFORMATION,
) {
    ApplicationManager.getApplication().invokeLater {
        NotificationGroupManager.getInstance().getNotificationGroup("Monkey C")
            .createNotification(message, type).notify(project)
    }
}

internal fun notifyError(project: Project, message: String) =
    notify(project, message, NotificationType.ERROR)

internal fun notifyInfo(project: Project, message: String) =
    notify(project, message, NotificationType.INFORMATION)

/** Runs a `monkey` CLI command off the EDT and reports the outcome. */
internal fun runCli(
    project: Project,
    args: List<String>,
    workingDir: File? = null,
    onSuccess: ((String) -> Unit)? = null,
) {
    val cmd = listOf(MonkeySettings.cliPath) + args
    ApplicationManager.getApplication().executeOnPooledThread {
        try {
            val pb = ProcessBuilder(cmd)
            workingDir?.let { pb.directory(it) }
            pb.redirectErrorStream(true)
            val proc = pb.start()
            val output = proc.inputStream.bufferedReader().readText()
            val finished = proc.waitFor(10, TimeUnit.MINUTES)
            val code = if (finished) proc.exitValue() else -1
            ApplicationManager.getApplication().invokeLater {
                if (code == 0) {
                    onSuccess?.invoke(output)
                    if (onSuccess == null) {
                        notifyInfo(project, "monkey ${args.firstOrNull() ?: ""} succeeded")
                    }
                } else {
                    notifyError(
                        project,
                        "monkey ${args.take(2).joinToString(" ")} failed (exit $code):\n" +
                            output.take(800),
                    )
                }
            }
        } catch (e: Exception) {
            ApplicationManager.getApplication().invokeLater {
                notifyError(project, "Failed to run monkey: ${e.message}")
            }
        }
    }
}

/** VirtualFile of the current editor when it is a Monkey C/Jungle file. */
internal fun currentMonkeyFile(e: AnActionEvent): VirtualFile? {
    val file = e.getData(CommonDataKeys.PSI_FILE)?.virtualFile
        ?: e.getData(CommonDataKeys.VIRTUAL_FILE)
    if (file == null) return null
    return if (isMonkeyFile(file.fileType)) file else null
}

internal fun isMonkeyFile(type: FileType): Boolean =
    type == MonkeyCFileType.MONKEY_C_FILE_TYPE || type == JungleFileType.JUNGLE_FILE_TYPE

/** Project commands run against the project base directory when present. */
internal fun projectDir(e: AnActionEvent): File? =
    e.project?.basePath?.let { File(it) }

/** Base class for `monkey <cmd> [project-path]` actions. */
abstract class MonkeyProjectAction(
    text: String,
    description: String,
    private val args: List<String>,
) : AnAction(text, description, null) {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val dir = projectDir(e)
        val fullArgs = if (dir != null) args + dir.absolutePath else args
        runCli(project, fullArgs, dir)
    }

    override fun update(e: AnActionEvent) {
        e.presentation.isEnabledAndVisible = e.project != null
    }
}

class BuildPrgAction : MonkeyProjectAction(
    "Build PRG", "Compile the project to bin/<name>.prg", listOf("build"))

class RunSimulatorAction : MonkeyProjectAction(
    "Build and Run", "Build and launch the app in the Connect IQ simulator",
    listOf("run"))

class TestAction : MonkeyProjectAction(
    "Test", "Lint, format-check, and build the project's Toybox.Test suites",
    listOf("test"))

class ExportIqAction : MonkeyProjectAction(
    "Export .iq", "Package the app as bin/<name>.iq", listOf("export"))

class DebugAction : MonkeyProjectAction(
    "Debug", "Build and launch the SDK debugger (mdd)", listOf("debug"))

class CleanAction : MonkeyProjectAction(
    "Clean", "Remove the project's bin/ build output", listOf("clean"))

class DoctorAction : MonkeyProjectAction(
    "Doctor", "Check SDK, developer key, and project setup", listOf("doctor"))

class ProjectInfoAction : MonkeyProjectAction(
    "Project Info", "Show discovered project details", listOf("project", "info"))

/** `monkey format --stdin <name>` → replace the editor buffer. */
class FormatDocumentAction : AnAction(
    "Format Document", "Format the current Monkey C/Jungle file", null) {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val vFile = currentMonkeyFile(e) ?: return
        val document = FileDocumentManager.getInstance().getDocument(vFile) ?: return
        val text = document.text
        val fileName = vFile.name
        ApplicationManager.getApplication().executeOnPooledThread {
            try {
                val pb = ProcessBuilder(
                    MonkeySettings.cliPath, "format", "--stdin", fileName,
                ).redirectErrorStream(true)
                val proc = pb.start()
                proc.outputStream.bufferedWriter().use { it.write(text) }
                val formatted = proc.inputStream.bufferedReader().readText()
                val finished = proc.waitFor(30, TimeUnit.SECONDS)
                val code = if (finished) proc.exitValue() else -1
                ApplicationManager.getApplication().invokeLater {
                    if (code == 0 && formatted != text) {
                        com.intellij.openapi.command.WriteCommandAction.runWriteCommandAction(
                            project,
                        ) {
                            document.setText(formatted)
                        }
                        PsiDocumentManager.getInstance(project)
                            .commitDocument(document)
                        notifyInfo(project, "Formatted $fileName")
                    } else if (code != 0) {
                        notifyError(project, "monkey format failed:\n${formatted.take(500)}")
                    }
                }
            } catch (ex: Exception) {
                ApplicationManager.getApplication().invokeLater {
                    notifyError(project, "Failed to run monkey format: ${ex.message}")
                }
            }
        }
    }

    override fun update(e: AnActionEvent) {
        e.presentation.isEnabledAndVisible = currentMonkeyFile(e) != null
    }
}

/** `monkey new <dir> --name <name> --type <type>` via folder + dialogs. */
class NewProjectAction : AnAction(
    "New Monkey C Project", "Scaffold a new Connect IQ project", null) {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val dir = com.intellij.openapi.fileChooser.FileChooserFactory.getInstance()
            .createFileChooser(
                com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
                    .createSingleFolderDescriptor(),
                project, null,
            ).choose(project).firstOrNull() ?: return

        val name = Messages.showInputDialog(
            project, "Project name:", "New Monkey C Project", null,
        )?.trim()?.takeIf { it.isNotEmpty() } ?: return

        val types = arrayOf("watchface", "datafield", "watchapp", "widget", "barrel")
        val typeIdx = Messages.showDialog(
            project, "Project type:", "New Monkey C Project",
            arrayOf(*types), -1, Messages.getQuestionIcon(),
        )
        if (typeIdx < 0) return

        val args = listOf(
            "new", dir.path, "--name", name, "--type", types[typeIdx],
        )
        runCli(project, args, File(dir.path), onSuccess = { output ->
            val vDir = LocalFileSystem.getInstance()
                .refreshAndFindFileByIoFile(File(dir.path, name))
                ?: LocalFileSystem.getInstance()
                    .refreshAndFindFileByIoFile(File(dir.path))
            if (vDir != null) {
                com.intellij.openapi.fileEditor.FileEditorManager
                    .getInstance(project).openFile(vDir, true)
            }
            notifyInfo(project, "Created Monkey C project '$name'\n$output".trim())
        })
    }

    override fun update(e: AnActionEvent) {
        e.presentation.isEnabledAndVisible = e.project != null
    }
}
