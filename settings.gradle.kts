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

// H2 prepares empty JVM modules. Production code moves in H3-H7.
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
