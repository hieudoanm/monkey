import org.jetbrains.intellij.platform.gradle.IntelliJPlatformType

plugins {
    kotlin("jvm") version "2.4.21"
    id("org.jetbrains.intellij.platform") version "2.20.1"
}

group = "io.github.hieudoanm.monkey.idea"
version = "0.0.1"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

kotlin {
    jvmToolchain(21)
}

dependencies {
    intellijPlatform {
        create(IntelliJPlatformType.IntellijIdeaCommunity, "2024.3")
    }
    implementation("com.google.code.gson:gson:2.15.0")
    testImplementation("junit:junit:4.13.2")
}

tasks.test {
    useJUnit()
}

intellijPlatform {
    pluginConfiguration {
        id = "io.github.hieudoanm.monkey.idea"
        name = "Monkey C"
        version = "0.0.1"
        description = """
            Monkey C and Jungle support for Connect IQ development:
            file types, syntax highlighting, lint diagnostics via the
            `monkey` CLI, and build/run/test/export actions.
        """.trimIndent()
        vendor {
            name = "hieudoanm"
        }
    }
}
