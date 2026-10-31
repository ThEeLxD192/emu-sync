import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
    id("emusync.kotlin-jvm")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.compose)
    alias(libs.plugins.kotlin.compose)
}

// Grouping projects (:data and :platform) have no build file or source sets.
val modules = subprojects.filter { it.buildFile.isFile }.map { it.path }

tasks.named("qualityCheck") {
    dependsOn(modules.map { "$it:qualityCheck" })
    dependsOn(gradle.includedBuild("build-logic").task(":check"))
}

tasks.named("test") { dependsOn(modules.map { "$it:test" }) }

tasks.register<Exec>("installGitHooks") {
    group = "build setup"
    description = "Installs the versioned Git hooks for this clone without replacing custom hooks."
    workingDir(rootDir)
    commandLine("bash", "scripts/install-git-hooks.sh")
}

val verifyGitHooks =
    tasks.register<Exec>("verifyGitHooks") {
        group = "verification"
        description = "Tests Git hook isolation and installation in disposable repositories."
        workingDir(rootDir)
        commandLine("bash", "scripts/tests/git-hooks-test.sh")
    }

val verifyGradleConventions =
    tasks.register<Exec>("verifyGradleConventions") {
        group = "verification"
        description =
            "Tests JVM conventions, aggregation and configuration cache in a disposable build."
        workingDir(rootDir)
        commandLine("bash", "scripts/tests/gradle-conventions-test.sh")
    }

tasks.named("check") {
    dependsOn(modules.map { "$it:check" })
    dependsOn(verifyGitHooks)
    dependsOn(verifyGradleConventions)
}

dependencies {
    implementation(project(":domain"))
    implementation(compose.desktop.currentOs)
    implementation(compose.material3)
    implementation(compose.materialIconsExtended)
    implementation(libs.ktor.client.core)
    implementation(libs.ktor.client.cio)
    implementation(libs.ktor.client.content.negotiation)
    implementation(libs.ktor.serialization.json)
    implementation(libs.coroutines.core)
    implementation(libs.serialization.json)
    testImplementation(libs.ktor.client.mock)
}

compose.desktop {
    application {
        mainClass = "MainKt"
        jvmArgs += "--enable-native-access=ALL-UNNAMED"
        javaHome =
            javaToolchains
                .launcherFor {
                    languageVersion.set(JavaLanguageVersion.of(libs.versions.java.get().toInt()))
                }
                .get()
                .metadata
                .installationPath
                .asFile
                .absolutePath

        buildTypes.release.proguard {
            isEnabled.set(false)
        }

        nativeDistributions {
            targetFormats(TargetFormat.AppImage, TargetFormat.Msi, TargetFormat.Exe)
            packageName = "EmuSync"

            // Include the HTTP server module needed by the OAuth loopback flow
            modules("jdk.httpserver")
            packageVersion = "1.1.1"
            description = "Centralized game launcher and save sync manager"
            vendor = "EmuSync"

            linux {
                iconFile.set(project.file("src/main/resources/icon.png"))
                shortcut = true
                appRelease = "1"
                appCategory = "Game"
                menuGroup = "Game"
            }
            windows {
                iconFile.set(project.file("src/main/resources/icon.ico"))
                menu = true
                shortcut = false
                dirChooser = true
                menuGroup = "EmuSync"
                upgradeUuid = "a7b3c291-5f4e-4b28-98e1-d63f5e824c90"
            }
        }
    }
}
