#!/usr/bin/env bash
# Validate the real conventions in a disposable project; never mutate application sources.
set -euo pipefail

source_root=$(cd "$(dirname "$0")/../.." && pwd)
fixture_root=$(mktemp -d "${TMPDIR:-/tmp}/emusync-gradle-tests.XXXXXXXX")
trap 'rm -rf -- "$fixture_root"' EXIT

fail() {
    echo "FAIL: $*" >&2
    for log_file in "$fixture_root"/*.log; do
        [[ ! -f "$log_file" ]] || tail -n 60 "$log_file" >&2
    done
    exit 1
}

cd "$source_root"
cp gradlew gradlew.bat settings.gradle.kts build.gradle.kts gradle.properties "$fixture_root/"
cp -R gradle config "$fixture_root/"
mkdir -p "$fixture_root/build-logic"
cp build-logic/settings.gradle.kts build-logic/build.gradle.kts "$fixture_root/build-logic/"
cp -R build-logic/src "$fixture_root/build-logic/"
for module in domain application data/local data/drive data/updates platform/desktop presentation desktopApp; do
    mkdir -p "$fixture_root/$module"
    cp "$module/build.gradle.kts" "$fixture_root/$module/"
done

cd "$fixture_root"
cat > domain/build.gradle.kts <<'GRADLE'
plugins {
    id("emusync.kotlin-jvm")
    `java-test-fixtures`
}

dependencies {
    testFixturesImplementation(libs.coroutines.core)
}
GRADLE
mkdir -p domain/src/{main,test,testFixtures}/kotlin
cat > domain/src/main/kotlin/ProductionValue.kt <<'KOTLIN'
fun productionValue(value: String) = value.trim()
KOTLIN
cat > domain/src/testFixtures/kotlin/FixtureValue.kt <<'KOTLIN'
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch

fun fixture_value(value: String) = value.uppercase()

// Deliberate typed finding: this function is analyzed but never invoked.
fun unsafeFixtureJob() = GlobalScope.launch {}
KOTLIN
cat > domain/src/test/kotlin/ConventionTest.kt <<'KOTLIN'
import kotlin.test.assertEquals
import org.junit.jupiter.api.Test

class ConventionTest {
    @Test
    fun `root test executes module tests with their fixtures`() {
        assertEquals("production", productionValue(" production "))
        assertEquals("FIXTURE", fixture_value("fixture"))
    }
}
KOTLIN

# Use exact root paths to prove explicit aggregation, rather than Gradle's task selector.
if ! bash ./gradlew :test :qualityCheck --configuration-cache --no-daemon > first.log 2>&1; then
    fail 'root test and qualityCheck did not execute successfully'
fi
report=domain/build/test-results/test/TEST-ConventionTest.xml
[[ -f "$report" ]] || fail 'root test did not produce the module JUnit report'
grep -q 'tests="1"' "$report" || fail 'module test was not discovered'
grep -q 'failures="0"' "$report" || fail 'module test failed'
for source_set in main test testFixtures; do
    report="domain/build/reports/detekt/$source_set.sarif"
    [[ -f "$report" ]] || fail "$source_set was not analyzed"
done
report=domain/build/reports/detekt/testFixtures.sarif
grep -q '"ruleId": "detekt.coroutines.GlobalCoroutineUsage"' "$report" || fail 'typed fixture safety rule was not applied'
if grep -q '"ruleId": "detekt.naming.FunctionNaming"' "$report"; then
    fail 'test fixture analysis did not use the test configuration'
fi
javap -verbose domain/build/classes/kotlin/testFixtures/FixtureValueKt.class > bytecode.log
grep -q 'major version: 69' bytecode.log || fail 'test fixtures did not target Java 25'
echo 'PASS: exact root tasks test modules and analyze main, test and fixtures with Java 25'

if ! bash ./gradlew :test :qualityCheck --configuration-cache --no-daemon > cached.log 2>&1; then
    fail 'cached root tasks failed'
fi
grep -q 'Configuration cache entry reused' cached.log || fail 'configuration cache was not reused'
echo 'PASS: module and fixture verification reuses the configuration cache'

sed -i 's/assertEquals("FIXTURE"/assertEquals("wrong"/' domain/src/test/kotlin/ConventionTest.kt
if bash ./gradlew :test --no-daemon > failed-test.log 2>&1; then
    fail 'root test accepted a failing module test'
fi
grep -q 'failures="1"' domain/build/test-results/test/TEST-ConventionTest.xml ||
    fail 'the expected module assertion did not fail'
echo 'PASS: a module assertion failure fails the exact root test task'

printf 'fun unformatted( )= "fixture"\n' > domain/src/testFixtures/kotlin/Unformatted.kt
if bash ./gradlew :qualityCheck --no-daemon > failed-format.log 2>&1; then
    fail 'root qualityCheck accepted an unformatted fixture'
fi
grep -q ':domain:spotlessKotlinCheck FAILED' failed-format.log ||
    fail 'fixture formatting did not block root qualityCheck'
echo 'PASS: an unformatted test fixture blocks exact root qualityCheck'
