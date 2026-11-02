plugins {
    `kotlin-dsl`
}

kotlin {
    jvmToolchain(libs.versions.java.get().toInt())
}

dependencies {
    implementation(libs.kotlin.gradle.plugin)
    implementation(libs.detekt.gradle.plugin)
    implementation(libs.spotless.gradle.plugin)
}
