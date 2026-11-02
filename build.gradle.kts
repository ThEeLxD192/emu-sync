import org.gradle.api.artifacts.ExternalModuleDependency
import org.gradle.api.artifacts.ProjectDependency

plugins {
    id("emusync.kotlin-jvm")
    // Declare shared plugin versions once for the module classpaths.
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.compose) apply false
    alias(libs.plugins.kotlin.compose) apply false
}

// Grouping projects (:data and :platform) have no build file or source sets.
val modules = subprojects.filter { it.buildFile.isFile }.map { it.path }

val verifyModuleArchitecture =
    tasks.register<VerifyModuleArchitecture>("verifyModuleArchitecture") {
        group = "verification"
        description = "Checks the production module graph and prevents root sources."
        projectDependencies.convention(emptyMap())
        libraryDependencies.convention(emptyMap())
        rootSources.from(fileTree("src"))
    }

allprojects {
    if (buildFile.isFile) {
        val owner = path
        afterEvaluate {
            val production = configurations.filter {
                it.name in
                    setOf(
                        "api",
                        "implementation",
                        "compileOnly",
                        "runtimeOnly",
                        "compileClasspath",
                        "runtimeClasspath",
                    )
            }
            val edges =
                production
                    .flatMap {
                        it.allDependencies.withType(ProjectDependency::class.java).map { dependency
                            ->
                            dependency.path
                        }
                    }
                    .distinct()
                    .sorted()
            val libraries =
                production
                    .flatMap { configuration ->
                        configuration.allDependencies
                            .withType(ExternalModuleDependency::class.java)
                            .map { "${it.group}:${it.name}" }
                    }
                    .distinct()
                    .sorted()
            verifyModuleArchitecture.configure {
                projectDependencies.put(owner, edges)
                libraryDependencies.put(owner, libraries)
            }
        }
    }
}

tasks.named("qualityCheck") {
    dependsOn(verifyModuleArchitecture)
    dependsOn(modules.map { "$it:qualityCheck" })
    dependsOn(gradle.includedBuild("build-logic").task(":check"))
}

tasks.named("test") { dependsOn(modules.map { "$it:test" }) }

// CI passes Git Bash's native absolute path so Windows never selects the WSL launcher.
val bashExecutable = providers.environmentVariable("EMUSYNC_BASH").orElse("bash")

tasks.register<Exec>("installGitHooks") {
    group = "build setup"
    description = "Installs the versioned Git hooks for this clone without replacing custom hooks."
    workingDir(rootDir)
    commandLine(bashExecutable.get(), "scripts/install-git-hooks.sh")
}

val verifyGitHooks =
    tasks.register<Exec>("verifyGitHooks") {
        group = "verification"
        description = "Tests Git hook isolation and installation in disposable repositories."
        workingDir(rootDir)
        commandLine(bashExecutable.get(), "scripts/tests/git-hooks-test.sh")
    }

val verifyGradleConventions =
    tasks.register<Exec>("verifyGradleConventions") {
        group = "verification"
        description =
            "Tests JVM conventions, aggregation and configuration cache in a disposable build."
        workingDir(rootDir)
        commandLine(bashExecutable.get(), "scripts/tests/gradle-conventions-test.sh")
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
