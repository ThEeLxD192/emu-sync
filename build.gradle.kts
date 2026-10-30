import dev.detekt.gradle.Detekt
import dev.detekt.gradle.extensions.FailOnSeverity
import org.jetbrains.compose.desktop.application.dsl.TargetFormat

val ktorVersion = providers.gradleProperty("ktorVersion").get()
val serializationVersion = providers.gradleProperty("serializationVersion").get()
val coroutinesVersion = providers.gradleProperty("coroutinesVersion").get()

plugins {
    kotlin("jvm")
    kotlin("plugin.serialization")
    id("org.jetbrains.compose")
    id("org.jetbrains.kotlin.plugin.compose")
    id("dev.detekt") version "2.0.0-alpha.6"
    id("com.diffplug.spotless") version "8.10.4"
}

kotlin {
    jvmToolchain(25)
}

spotless {
    kotlin {
        target("src/**/*.kt")
        ktfmt("0.64").kotlinlangStyle()
    }
    kotlinGradle {
        target("*.gradle.kts")
        ktfmt("0.64").kotlinlangStyle()
    }
}

detekt {
    buildUponDefaultConfig.set(true)
    config.setFrom("config/detekt.yml")
    failOnSeverity.set(FailOnSeverity.Warning)
    // Adoption phase: findings are reported; formatter violations still block the build.
    ignoreFailures.set(true)
}

tasks.withType<Detekt>().configureEach {
    if (name == "detektTest") config.setFrom("config/detekt.yml", "config/detekt-test.yml")
    reports {
        html.required.set(true)
        sarif.required.set(true)
    }
}

val qualityCheck =
    tasks.register("qualityCheck") {
        group = "verification"
        description = "Checks Kotlin formatting and analyzes production and test code."
        dependsOn("spotlessCheck", "detektMain", "detektTest")
    }

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

tasks.named("check") {
    dependsOn(qualityCheck)
    dependsOn(verifyGitHooks)
}

// The typed analysis tasks cover both source sets, including rules needing type resolution.
tasks.named("detekt") {
    dependsOn("detektMain", "detektTest")
    enabled = false
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
        javaHome =
            javaToolchains
                .launcherFor {
                    languageVersion.set(JavaLanguageVersion.of(25))
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
