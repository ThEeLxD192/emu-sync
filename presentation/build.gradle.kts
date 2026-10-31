plugins {
    id("emusync.kotlin-jvm")
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.compose)
    alias(libs.plugins.kotlin.compose)
}

dependencies {
    implementation(project(":application"))
    implementation(project(":domain"))
    implementation(compose.desktop.currentOs)
    implementation(compose.material3)
    implementation(compose.materialIconsExtended)
    implementation(libs.coroutines.core)
    testImplementation(project(":data:local"))
    testImplementation(project(":data:drive"))
    testImplementation(project(":data:updates"))
    testImplementation(project(":platform:desktop"))
    testImplementation(libs.ktor.client.core)
    testImplementation(libs.ktor.client.cio)
    testImplementation(libs.ktor.client.content.negotiation)
    testImplementation(libs.ktor.serialization.json)
    testImplementation(libs.serialization.json)
    testImplementation(libs.ktor.client.mock)
    testImplementation(testFixtures(project(":data:drive")))
}
