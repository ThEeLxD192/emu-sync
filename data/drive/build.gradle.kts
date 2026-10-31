plugins {
    id("emusync.kotlin-jvm")
    `java-test-fixtures`
    alias(libs.plugins.kotlin.serialization)
}

dependencies {
    implementation(project(":domain"))
    implementation(libs.coroutines.core)
    implementation(libs.serialization.json)
    implementation(libs.ktor.client.core)
    implementation(libs.ktor.client.cio)
    implementation(libs.ktor.client.content.negotiation)
    implementation(libs.ktor.serialization.json)
    testImplementation(libs.ktor.client.mock)
    testFixturesImplementation(libs.ktor.client.core)
    testFixturesImplementation(libs.ktor.client.mock)
    testFixturesImplementation(libs.ktor.client.content.negotiation)
    testFixturesImplementation(libs.ktor.serialization.json)
    testFixturesImplementation(libs.serialization.json)
}
