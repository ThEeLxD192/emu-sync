# EmuSync

A modern, offline-first save synchronization manager and universal launcher for emulators and native PC games, built with **Compose Multiplatform (Kotlin Desktop)**, **Google Drive API**, and **Steam Deck / Steam Shortcut** integration.

---

## Features

- 🎮 **Universal Game Launcher**: Launch emulator ROMs and native PC games directly from a sleek dark-themed UI.
- ☁️ **Google Drive Save Sync**: Automatic pre-play download and post-play upload of game saves with folder organization per emulator/game.
- 🔄 **Bidirectional Timestamp Synchronization**: Google Drive server-side timestamps are mirrored to local files, ensuring seamless sync tracking across app restarts.
- 📴 **100% Offline-First**: Launch and play your games instantly without an internet connection; saves are safely kept locally and synced whenever connection returns.
- ⚔️ **Smart Conflict Resolution**: Detects if offline progress is newer than cloud saves, presenting an intuitive resolution dialog.
- 🚀 **In-App Auto-Updater**: Built-in update detector against GitHub Releases with background download and in-place restart for AppImage builds.
- 🕹️ **Steam Deck & Controller Layouts**: One-click registration as Non-Steam games into `shortcuts.vdf` for Steam Big Picture / Game Mode and custom controller profiles.
- 🐳 **Hermetic Docker Build**: Package reproducible, standalone `.AppImage` binaries without requiring local toolchains.

---

## Architecture

- **UI Framework**: Jetpack Compose Multiplatform (Desktop JVM) with Kotlin Coroutines and StateFlow.
- **HTTP Engine**: Ktor Client with CIO engine, OAuth 2.0 loopback flow, and resilient timeout handling.
- **Auto-Update Engine**: GitHub Releases API integration with atomic AppImage swap and restart.
- **Steam Integration**: Binary VDF parser/serializer for Steam's `shortcuts.vdf` and launcher shell scripts.
- **Build System**: Gradle 8.11 with Java 21 LTS runtime and AppImage packaging toolchain.

---

## Getting Started

### Prerequisites

- **Java 21+** (Eclipse Temurin recommended) or **Docker**
- **Linux x86_64** (SteamOS, Ubuntu, Arch, Fedora, Debian) or **Windows 10/11 x86_64**

---

### Configuration

Copy the example configuration to create your local config file:

```bash
cp config.example.json config.json
```

Edit `config.json` with your preferred emulators, paths, and Google Drive OAuth credentials:

```json
{
  "googleDrive": {
    "clientId": "YOUR_GOOGLE_CLIENT_ID.apps.googleusercontent.com",
    "clientSecret": "YOUR_GOOGLE_CLIENT_SECRET",
    "refreshToken": null
  },
  "entries": [
    {
      "type": "emulator",
      "name": "Game Boy Advance",
      "executablePath": "/usr/bin/mgba",
      "arguments": ["{ROM}"],
      "romsDirectory": "/home/deck/Emulation/roms/gba",
      "extensions": ["gba", "zip"],
      "savePaths": ["/home/deck/Emulation/saves/mgba"],
      "fullscreenArgs": "-f"
    },
    {
      "type": "native",
      "name": "Spelunky Classic",
      "executablePath": "/usr/bin/steam",
      "arguments": ["steam://rungameid/239350"],
      "savePaths": ["/home/deck/.local/share/spelunky/save.dat"],
      "waitForProcess": "Spelunky.exe"
    }
  ]
}
```

---

### Google Drive OAuth 2.0 Setup

1. Go to the [Google Cloud Console](https://console.cloud.google.com/).
2. Create a project and enable the **Google Drive API**.
3. Under **OAuth consent screen**, set user type to **External** and add the `https://www.googleapis.com/auth/drive.file` scope.
4. Set **Publishing status** to **Production** (so refresh tokens never expire).
5. Under **Credentials**, create an **OAuth client ID** of type **Desktop App**.
6. Copy the **Client ID** and **Client Secret** into your `config.json`.
7. Launch EmuSync and click **"Connect Drive"** to authenticate via browser.

---

### Building & Running

#### 1. Quick Build via Docker (Linux AppImage)

Generate a standalone `EmuSync-x86_64.AppImage` directly in the project root:

```bash
docker compose build && docker compose run --rm build
```

Then run the AppImage:

```bash
./EmuSync-x86_64.AppImage
```

#### 2. Local Gradle Build (Cross-Platform)

```bash
# Run unit tests
./gradlew test

# Run app directly in development mode
./gradlew run

# Package for Windows (MSI installer & EXE bootstrapper)
./gradlew packageReleaseMsi packageReleaseExe

# Package for Linux (AppImage)
./gradlew packageReleaseAppImage
```

---

## CI/CD & Automated Releases

EmuSync utilizes GitHub Actions for continuous integration and automated release packaging:

- **CI Validation (`.github/workflows/ci.yml`)**: Runs on every push and pull request to `main`, verifying unit tests (`./gradlew test`) and Compose Desktop packaging.
- **Publish Release (`.github/workflows/release.yml`)**: Interactive manual trigger (`workflow_dispatch`) that:
  1. Prompts for the SemVer release type (`patch`, `minor`, `major`).
  2. Bumps the application version and creates the git tag (`vX.Y.Z`).
  3. Builds `EmuSync-x86_64.AppImage` (Linux), `EmuSync-x86_64.msi`, and `EmuSync-x86_64.exe` (Windows) in parallel.
  4. Creates the GitHub Release with automated changelogs and attaches all platform installers as release assets.

---

## License

MIT License. See [LICENSE](LICENSE) for details.
