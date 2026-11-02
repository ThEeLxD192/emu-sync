plugins {
    kotlin("jvm")
    id("emusync.quality")
}

val catalog = extensions.getByType<VersionCatalogsExtension>().named("libs")

kotlin {
    jvmToolchain(catalog.findVersion("java").get().requiredVersion.toInt())
}

dependencies {
    testImplementation(catalog.findLibrary("kotlin-test").get())
    testImplementation(catalog.findLibrary("junit-jupiter").get())
    testImplementation(catalog.findLibrary("coroutines-test").get())
    testRuntimeOnly(catalog.findLibrary("junit-platform-launcher").get())
}

tasks.withType<Test>().configureEach {
    useJUnitPlatform()
    jvmArgs("--enable-native-access=ALL-UNNAMED")
    testLogging {
        exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
    }
}
