# EmuSync

A modern, offline-first save synchronization manager and universal launcher for emulators and native PC games, built with **Compose Multiplatform (Kotlin Desktop)**, **Google Drive API**, and **Steam Deck / Steam Shortcut** integration.

---

## Features

- 🎮 **Universal Game Launcher**: Launch emulator ROMs and native PC games directly from a sleek dark-themed UI.
- ☁️ **Google Drive Save Sync**: Automatic pre-play download and post-play upload of game saves with folder organization per emulator/game.
- 📴 **100% Offline-First**: Launch and play your games instantly without internet connection; saves are safely kept locally and synced whenever connection returns.
- ⚔️ **Smart Conflict Resolution**: Detects if offline progress is newer than cloud saves, presenting an intuitive resolution dialog.
- 🕹️ **Steam Deck & Controller Layouts**: One-click registration as Non-Steam games into `shortcuts.vdf` for Steam Big Picture / Game Mode and custom controller profiles.
- 🐳 **Hermetic Docker Build**: Package reproducible, standalone `.AppImage` binaries without requiring local toolchains.

---

## Architecture

- **UI Framework**: Jetpack Compose Multiplatform (Desktop JVM) with Kotlin Coroutines and StateFlow.
- **HTTP Engine**: Ktor Client with CIO engine, OAuth 2.0 loopback flow, and fast connection timeouts.
- **Steam Integration**: Binary VDF parser/serializer for Steam's `shortcuts.vdf` and launcher shell scripts.
- **Build System**: Gradle 8.11 with Java 21 LTS runtime and AppImage packaging toolchain.

---

## Getting Started

### Prerequisites

- **Java 21+** (Eclipse Temurin recommended) or **Docker**
- **Linux x86_64** (Ubuntu, Arch, SteamOS, Fedora, Debian)

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

#### 1. Quick Build via Docker (Recommended)

Generate a standalone `EmuSync-1.0.0-x86_64.AppImage` directly in the project root:

```bash
docker compose build && docker compose run --rm build
```

Then run the AppImage:

```bash
./EmuSync-1.0.0-x86_64.AppImage
```

#### 2. Local Gradle Build

```bash
# Run unit tests
./gradlew test

# Run app directly
./gradlew run
```

---

## License

MIT License. See [LICENSE](LICENSE) for details.
