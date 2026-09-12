#!/usr/bin/env bash
set -e

BUMP_TYPE="${1:-patch}"

APP_INFO_FILE="src/main/kotlin/com/emusync/AppInfo.kt"
BUILD_GRADLE_FILE="build.gradle.kts"

if [ ! -f "$APP_INFO_FILE" ]; then
    echo "Error: $APP_INFO_FILE not found." >&2
    exit 1
fi

# Extract current version from AppInfo.kt
CURRENT_VERSION=$(grep -oP 'VERSION = "\K[^"]+' "$APP_INFO_FILE")
if [ -z "$CURRENT_VERSION" ]; then
    echo "Error: Could not extract current version from $APP_INFO_FILE" >&2
    exit 1
fi

IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
MAJOR="${MAJOR:-0}"
MINOR="${MINOR:-1}"
PATCH="${PATCH:-0}"

case "$BUMP_TYPE" in
    patch|fix)
        PATCH=$((PATCH + 1))
        ;;
    minor)
        MINOR=$((MINOR + 1))
        PATCH=0
        ;;
    major)
        MAJOR=$((MAJOR + 1))
        MINOR=0
        PATCH=0
        ;;
    *)
        echo "Unknown bump type: $BUMP_TYPE. Use patch, minor, or major." >&2
        exit 1
        ;;
esac

NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}"
echo "Bumping version: $CURRENT_VERSION -> $NEW_VERSION ($BUMP_TYPE)"

# Update AppInfo.kt
sed -i -E "s/VERSION = \"[^\"]+\"/VERSION = \"${NEW_VERSION}\"/" "$APP_INFO_FILE"

# Update build.gradle.kts
sed -i -E "s/packageVersion = \"[^\"]+\"/packageVersion = \"${NEW_VERSION}\"/" "$BUILD_GRADLE_FILE"

echo "Updated $APP_INFO_FILE and $BUILD_GRADLE_FILE to $NEW_VERSION"

# If in GitHub Actions, export output
if [ -n "$GITHUB_OUTPUT" ]; then
    echo "new_version=${NEW_VERSION}" >> "$GITHUB_OUTPUT"
    echo "tag_name=v${NEW_VERSION}" >> "$GITHUB_OUTPUT"
fi
