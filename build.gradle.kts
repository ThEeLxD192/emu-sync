import org.jetbrains.compose.desktop.application.dsl.TargetFormat

val ktorVersion = providers.gradleProperty("ktorVersion").get()
val serializationVersion = providers.gradleProperty("serializationVersion").get()
val coroutinesVersion = providers.gradleProperty("coroutinesVersion").get()

plugins {
    kotlin("jvm")
    kotlin("plugin.serialization")
    id("org.jetbrains.compose")
    id("org.jetbrains.kotlin.plugin.compose")
}

kotlin {
    jvmToolchain(25)
}

dependencies {
    // Compose Desktop
    implementation(compose.desktop.currentOs)
    implementation(compose.material3)
    implementation(compose.materialIconsExtended)

    // Ktor Client
    implementation("io.ktor:ktor-client-core:$ktorVersion")
    implementation("io.ktor:ktor-client-cio:$ktorVersion")
    implementation("io.ktor:ktor-client-content-negotiation:$ktorVersion")
    implementation("io.ktor:ktor-serialization-kotlinx-json:$ktorVersion")

    // Kotlin Coroutines
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:$coroutinesVersion")

    // Kotlin Serialization
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:$serializationVersion")

    // Test dependencies
    testImplementation(kotlin("test"))
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.2")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:$coroutinesVersion")
    testImplementation("io.ktor:ktor-client-mock:$ktorVersion")
}

tasks.withType<Test> {
    useJUnitPlatform()
    jvmArgs("--enable-native-access=ALL-UNNAMED")
    testLogging {
        exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
    }
}

compose.desktop {
    application {
        mainClass = "MainKt"
        jvmArgs += "--enable-native-access=ALL-UNNAMED"
        javaHome = javaToolchains.launcherFor {
            languageVersion.set(JavaLanguageVersion.of(25))
        }.get().metadata.installationPath.asFile.absolutePath

        buildTypes.release.proguard {
            isEnabled.set(false)
        }

        nativeDistributions {
            targetFormats(TargetFormat.AppImage, TargetFormat.Msi, TargetFormat.Exe)
            packageName = "EmuSync"

            // Include the HTTP server module needed by the OAuth loopback flow
            modules("jdk.httpserver")
            packageVersion = "1.1.0"
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


