plugins {
    id("emusync.kotlin-jvm")
    alias(libs.plugins.kotlin.serialization)
}

dependencies {
    implementation(project(":domain"))
    implementation(libs.coroutines.core)
    implementation(libs.serialization.json)
}
