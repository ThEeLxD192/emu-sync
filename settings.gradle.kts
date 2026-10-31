pluginManagement {
    includeBuild("build-logic")
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "emu-sync"

// JVM modules follow the dependency graph documented in AGENTS.md.
include(
    ":domain",
    ":application",
    ":data:local",
    ":data:drive",
    ":data:updates",
    ":platform:desktop",
    ":presentation",
    ":desktopApp",
)
