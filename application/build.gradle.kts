plugins {
    id("emusync.kotlin-jvm")
}

dependencies {
    implementation(project(":domain"))
    implementation(libs.coroutines.core)
}
