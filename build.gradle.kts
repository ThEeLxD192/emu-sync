plugins {
    id("emusync.kotlin-jvm")
    // Declare shared plugin versions once for the module classpaths.
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.compose) apply false
    alias(libs.plugins.kotlin.compose) apply false
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

// Preserve exact root entry points for scripts, CI and local builds.
listOf(
        "run",
        "runDistributable",
        "createDistributable",
        "createReleaseDistributable",
        "packageReleaseAppImage",
        "packageReleaseMsi",
        "packageReleaseExe",
    )
    .forEach { name ->
        tasks.register(name) { dependsOn(":desktopApp:$name") }
    }
