import org.jetbrains.compose.desktop.application.dsl.TargetFormat

val ktorVersion: String by project
val serializationVersion: String by project
val coroutinesVersion: String by project

plugins {
    kotlin("jvm")
    kotlin("plugin.serialization")
    id("org.jetbrains.compose")
    id("org.jetbrains.kotlin.plugin.compose")
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
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:$coroutinesVersion")
    testImplementation("io.ktor:ktor-client-mock:$ktorVersion")
}

tasks.withType<Test> {
    useJUnitPlatform()
}

compose.desktop {
    application {
        mainClass = "MainKt"

        // Force UI scale to 1.0 so gamescope (Steam Deck Game Mode) doesn't
        // cause the JVM to render at 2× DPI, which cuts off dialogs.
        jvmArgs += listOf("-Dsun.java2d.uiScale=1.0")

        buildTypes.release.proguard {
            isEnabled.set(false)
        }

        nativeDistributions {
            targetFormats(TargetFormat.AppImage)
            packageName = "EmuSync"

            // Include the HTTP server module needed by the OAuth loopback flow
            modules("jdk.httpserver")
            packageVersion = "0.1.0"
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
            }
        }
    }
}


