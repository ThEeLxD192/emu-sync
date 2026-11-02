import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.provider.MapProperty
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction

/** Checks production edges independently of compilation, including projects without sources. */
abstract class VerifyModuleArchitecture : DefaultTask() {
    @get:Input abstract val projectDependencies: MapProperty<String, List<String>>

    @get:Input abstract val libraryDependencies: MapProperty<String, List<String>>

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val rootSources: ConfigurableFileCollection

    @TaskAction
    fun verify() {
        val adapters = setOf(":data:local", ":data:drive", ":data:updates", ":platform:desktop")
        val allowed =
            mapOf(
                ":" to emptySet(),
                ":domain" to emptySet(),
                ":application" to setOf(":domain"),
                ":presentation" to setOf(":application", ":domain"),
                ":desktopApp" to adapters + setOf(":application", ":domain", ":presentation"),
            ) + adapters.associateWith { setOf(":domain") }
        val violations =
            projectDependencies.get().flatMap { (owner, dependencies) ->
                val permitted = allowed[owner]
                if (permitted == null) listOf("Unknown production module: $owner")
                else dependencies.filterNot { it in permitted }.map { "$owner -> $it" }
            }
        val libraryViolations =
            libraryDependencies.get().flatMap { (owner, dependencies) ->
                dependencies
                    .filter { coordinate ->
                        val ktor = coordinate.startsWith("io.ktor:")
                        val compose =
                            coordinate.startsWith("androidx.compose") ||
                                coordinate.startsWith("org.jetbrains.compose")
                        val serialization =
                            coordinate.startsWith("org.jetbrains.kotlinx:kotlinx-serialization")
                        when (owner) {
                            ":domain",
                            ":application" -> ktor || compose || serialization
                            ":data:local",
                            ":platform:desktop" -> ktor || compose
                            ":data:drive",
                            ":data:updates" -> compose
                            ":presentation" -> ktor
                            else -> false
                        }
                    }
                    .map { "$owner -> $it" }
            }
        check(violations.isEmpty() && libraryViolations.isEmpty()) {
            "Forbidden production dependencies: ${(violations + libraryViolations).joinToString()}. See AGENTS.md."
        }
        check(rootSources.isEmpty) {
            "Root source files are forbidden; move them to their owning module."
        }
    }
}
