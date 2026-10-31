import dev.detekt.gradle.Detekt
import dev.detekt.gradle.extensions.FailOnSeverity

plugins {
    base
    id("dev.detekt")
    id("com.diffplug.spotless")
}

val catalog = extensions.getByType<VersionCatalogsExtension>().named("libs")
val ktfmtVersion = catalog.findVersion("ktfmt").get().requiredVersion

spotless {
    kotlin {
        target("src/**/*.kt")
        ktfmt(ktfmtVersion).kotlinlangStyle()
    }
    kotlinGradle {
        target("*.gradle.kts")
        if (project == rootProject) {
            target("*.gradle.kts", "build-logic/*.gradle.kts", "build-logic/src/**/*.gradle.kts")
        }
        ktfmt(ktfmtVersion).kotlinlangStyle()
    }
}

detekt {
    buildUponDefaultConfig.set(true)
    config.setFrom(rootProject.file("config/detekt.yml"))
    failOnSeverity.set(FailOnSeverity.Warning)
    // Findings remain informative; formatting still blocks the build.
    ignoreFailures.set(true)
}

tasks.withType<Detekt>().configureEach {
    if (name.contains("Test")) {
        config.setFrom(
            rootProject.file("config/detekt.yml"),
            rootProject.file("config/detekt-test.yml"),
        )
    }
    reports {
        html.required.set(true)
        sarif.required.set(true)
    }
}

// JVM compilation tasks include fixtures and type resolution. The SourceSet tasks
// repeat the same Kotlin sources without type resolution, so leave them out.
val typedAnalysis =
    tasks.withType<Detekt>().matching {
        it.name != "detekt" && !it.name.endsWith("SourceSet")
    }
val qualityCheck =
    tasks.register("qualityCheck") {
        group = "verification"
        description = "Checks Kotlin formatting and analyzes all JVM source sets."
        dependsOn("spotlessCheck")
        dependsOn(typedAnalysis)
    }

tasks.named("check") { dependsOn(qualityCheck) }

tasks.named("detekt") {
    dependsOn(typedAnalysis)
    enabled = false
}
