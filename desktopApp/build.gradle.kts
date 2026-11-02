import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
    id("emusync.kotlin-jvm")
    alias(libs.plugins.compose)
    alias(libs.plugins.kotlin.compose)
}

dependencies {
    implementation(project(":presentation"))
    implementation(project(":application"))
    implementation(project(":domain"))
    implementation(project(":data:local"))
    implementation(project(":data:drive"))
    implementation(project(":data:updates"))
    implementation(project(":platform:desktop"))
    implementation(compose.desktop.currentOs)
    implementation(compose.material3)
    implementation(libs.ktor.client.core)
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
            packageVersion = "1.2.0"
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
