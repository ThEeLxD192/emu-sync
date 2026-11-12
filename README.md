# EmuSync

A modern, offline-first save synchronization manager and universal launcher for emulators and native PC games, built natively in **Rust** (egui/wgpu), **Google Drive API**, and **Steam Deck / Steam Shortcut** integration.

---

## Features

- 🎮 **Universal Game Launcher**: Launch emulator ROMs and native PC games directly from a sleek dark-themed UI.
- ☁️ **Google Drive Save Sync**: Automatic pre-play download and post-play upload of game saves with folder organization per emulator/game.
- 🔄 **Persistent Sync History**: Content fingerprints and Drive metadata identify local and cloud changes across app restarts, even when clocks disagree.
- 📴 **Offline Play**: Play without an internet connection and keep progress locally; synchronize again when connected to reconcile changes.
- ⚔️ **Conflict Resolution and Recovery**: Asks which progress to keep when both copies changed and preserves a local recovery copy before restoring cloud content.
- 🚀 **In-App Auto-Updater**: Built-in update detector against GitHub Releases with background download and in-place restart for AppImage builds.
- 🕹️ **Steam Deck & Controller Layouts**: One-click registration as Non-Steam games into `shortcuts.vdf` for Steam Big Picture / Game Mode and custom controller profiles.
- 🐳 **Container Build**: Package the Linux `.AppImage` on a Debian Bookworm baseline with Docker.

---

## Architecture

- **UI**: egui on winit and wgpu, with no JVM or WebView.
- **Runtime & HTTP**: Tokio for background coordination; reqwest with Rustls for Google Drive (OAuth 2.0 loopback flow) and GitHub Releases.
- **Auto-Update Engine**: GitHub Releases API integration with atomic AppImage swap and restart.
- **Steam Integration**: Binary VDF parser/serializer for Steam's `shortcuts.vdf` and launcher scripts.
- **Workspace**: a Cargo workspace under `rust/`, split by capability: `domain` (models, sync rules and contracts), `application` (use cases), `data-local`, `data-drive`, `data-updates`, `platform-desktop` (adapters), `presentation` (UI) and `desktop-app` (startup and composition). `cargo xtask` verifies the dependency direction between layers.

---

## Getting Started

### Prerequisites

- **Rust** with the toolchain pinned in `rust-toolchain.toml` (installed automatically by `rustup`) for development, or only **Docker** and **Git** to build the Linux AppImage.
- **Linux x86_64** (SteamOS, Ubuntu, Arch, Fedora, Debian) or **Windows 10/11 x86_64**

---

### Configuration

When running EmuSync for the first time, a basic `config.json` is **automatically generated** if one does not exist:
* **Windows (.msi):** `%APPDATA%\emusync\config.json`
* **Windows portable (.zip):** a `portable.txt` file next to `EmuSync.exe` keeps `config.json`, history and backups in that folder. A portable copy does not update itself; replace the folder with the newer ZIP.
* **Linux (AppImage / package):** `~/.config/emusync/config.json`
* **AppImage Portable Mode:** If a `config.json` exists in the same folder as your `.AppImage`, it is automatically detected and used.
* **Development / CLI:** If a `config.json` is present in the working directory, it takes priority.

You can configure Google Drive credentials directly inside the app by clicking **"Connect Drive"**, or edit `config.json` manually:

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

### Code Quality

```bash
# Install the versioned pre-commit hook once per clone
./scripts/install-git-hooks.sh

# Formatting, Clippy and architecture checks, without tests
cargo xtask quality

# The same checks plus the full test suite (also required by CI)
cargo xtask check
```

To keep the machine responsive, limit parallelism locally, for example
`CARGO_BUILD_JOBS=2 nice -n 19 cargo test --workspace --locked -- --test-threads=2`
(add `ionice -c3` if the disk is saturated).

The hook checks an isolated copy of the staged commit with `cargo xtask quality` when
Rust sources, manifests or quality configuration change. Unstaged edits remain untouched.
Documentation-only and image-only commits skip the check. The hook never formats or
stages files and does not run tests. Git hooks can be bypassed and must be installed in
each clone; configure GitHub branch protection to require the Linux and Windows CI jobs.
The installer refuses to replace existing custom hook settings or active hooks. To
uninstall it, run `git config --local --unset core.hooksPath`.

---

### Google Drive OAuth 2.0 Setup

1. Go to the [Google Cloud Console](https://console.cloud.google.com/).
2. Create a project and enable the **Google Drive API**.
3. Under **OAuth consent screen**, set user type to **External** and add the `https://www.googleapis.com/auth/drive.file` scope.
4. Set **Publishing status** to **Production** (so refresh tokens never expire).
5. Under **Credentials**, create an **OAuth client ID** of type **Desktop App**.
6. Copy the **Client ID** and **Client Secret** into your `config.json`.
7. Launch EmuSync and click **"Connect Drive"** to authenticate via browser.

EmuSync intentionally requests the limited `drive.file` scope. It can discover and
synchronize files created or explicitly managed by EmuSync, but it cannot browse or
import arbitrary files uploaded manually or created by another application. Expanding
that access would require a dedicated Google Picker flow and a separate permission review.

The OAuth client secret and refresh token are stored in `config.json` so portable installs
continue to work. EmuSync writes this file atomically and restricts it to the current user
(`0600`) on POSIX systems. Treat the file as sensitive: do not publish it, attach it to bug
reports, or commit it to source control.

OAuth results update only the current credentials, preserving library changes made while
the browser login is pending. Disconnecting Drive or replacing its credentials invalidates
pending authorizations. Changing the OAuth client clears the previous client's refresh
token. Access is refreshed again after playing, before uploading saves, so long game
sessions do not reuse the access token obtained before launch. Temporary network failures
preserve the refresh token; a rejected grant clears only the token that was rejected.

Synchronization records the last agreed local SHA-256 fingerprint and the observed Drive
file identity, checksum and version in `config.json.sync-state.json` beside the configuration.
Google documents these fields in the [Drive file reference](https://developers.google.com/workspace/drive/api/reference/rest/v3/files).
This history is local to each device. Content changes are detected independently of local
clock ordering. When both sides have changed, EmuSync asks which copy to keep. On the first
sync, two existing copies also require a choice unless their checksums prove equal.
Checking the status alone does not advance synchronization history.

Before replacing a different local save with cloud content, EmuSync retains the original
under `config.json.sync-backups/<location-key>/<content-hash>/<filename>`. The adjacent
`source.json` identifies the original local path. Backups are deduplicated by content and
retained until you remove them. To recover one, close the game and copy the desired backup
to its original path, then synchronize and choose the local save if prompted. These files
may contain personal game data and should stay out of source control.

Downloads are staged and checked before replacement; uploads use a stable snapshot.
Changes made to a local save during a transfer remain pending instead of being marked
synchronized. A failed restore, unreadable history, or failure to preserve a recovery copy
stops launch so the problem can be resolved. Network failures while checking Drive still
allow offline play. Choosing a local copy before launch finishes that resolution before the
game starts; it does not authorize overwriting other conflicts after playing.

Existing saves are updated with a single streaming media request guarded by `If-Match`.
The updater reads the explicit [Drive v2 file ETag](https://developers.google.com/workspace/drive/api/reference/rest/v2/files),
checks it against the version or content observed during planning, and never retries a
rejected precondition without checking again. Listing and initial resumable creations
continue to use v3. If an update fails partway through, an explicit retry starts the
conditional update again. Missing or weak version tokens stop the update. This protection
applies to devices running this version of EmuSync.

Drive permits several files with the same name. If two devices initially create the same
save, both remote files remain intact. Divergent or unverifiable duplicates stop automatic
selection; resolve those copies in Drive before synchronizing again. Identical duplicates
can be compared safely. Local progress and the previous sync history survive a rejected
conditional update.

Steam launcher and ROM signal paths include a stable SHA-256 identity of the full category
name, so punctuation, case and non-Latin names do not cause shared files. On startup,
registered emulator shortcuts migrate without changing their Steam app IDs or preferences.
Unambiguous old scripts forward to the new launcher and preserve their last ROM selection.
If an old script belonged to colliding names, it stops with a request to restart Steam;
the migrated shortcuts then use their separate launchers. Generated legacy scripts are
retained in `.legacy.bak` files.

Windows emulator shortcuts now use Windows PowerShell and a JSON launch manifest.
Arguments, including empty strings, quotes, backslashes and shell metacharacters, are
passed to the native process as data. Legacy batch wrappers remain as encoded loaders.
The emulator target must be a native executable; `.bat` and `.cmd` targets are rejected.

When a cloud rename would merge two existing folders, EmuSync checks for duplicate file
and folder names before moving anything. Duplicates block the rename and preserve both
copies; resolve them in Google Drive before retrying. Failed moves trigger an attempt to
restore previously moved files. The source folder is retained after a successful merge,
so files added there by another device during the operation remain recoverable.

Regrouping an emulator that shares its cloud folder with another entry keeps its saves
in that folder by setting an explicit `cloudFolder`. Explicit attempts to migrate a shared
folder are blocked. Grouped native games move only their own game subfolder.

Typed `saveTargets` also support an optional `cloudPath`. For a file, this is its exact
remote name, including nested components such as `profiles/slot1/save.dat`. For a
directory, it is the prefix used for that directory's remote files:

```json
"saveTargets": [
  {"path": "/local/progress", "type": "directory", "cloudPath": "progress"},
  {"path": "/local/trophies", "type": "directory", "cloudPath": "trophies"}
]
```

Both directories can contain `save.dat`: their remote names will be `progress/save.dat`
and `trophies/save.dat`. Keep the same cloud paths on every device while choosing the
appropriate local paths. The add/edit dialogs expose this as **Cloud path**. Newly
configured sets of multiple directories receive separate `save-target-N` prefixes
automatically; the saved prefixes remain stable when local paths are changed.

Existing configurations keep their previous cloud layout. If files from several legacy
directories cannot be assigned safely, synchronization stops before restoring them.
The notice identifies the configured paths, unavailable folders, duplicate relative
names, and cloud files without a local match. A folder can exist but still lack the
files needed to identify its old cloud saves; check its contents on the affected device.
Restore the cloud files individually, or reconnect the original local directories before
assigning separate cloud paths. Assigning a new prefix does not relocate existing remote
files. Individual restores retain the exact remote names and synchronize only the listed
files; choosing **The entire folder** instead includes future files in that folder.

Library selection ignores late scans from previously selected categories. Save-changing
operations (play, manual sync, cloud migrations and post-play retries) share an exclusive
coordinator: repeated clicks are ignored rather than launching queued sessions. Cancelling
a save setup closes only that request, and callbacks from old dialogs cannot resolve a
new request or replay a completed retry. Category names must be non-empty and unique.

Update checks and downloads have their own coordinator. A late check cannot replace a
download or a ready installer. Downloads use unique staging files, preserve existing files
on failure, and verify the response length and release asset size when provided. An app
restart is blocked during an active game or save operation. Cover lookup and decoding run
outside the UI thread; cached covers are keyed by path, size and modification time.

---

### Building & Running

#### Run in development

```bash
cargo run -p emusync-desktop-app -- --config /tmp/emusync-dev/config.json
```

Use `--config` with a synthetic configuration and isolated save data while developing.

#### Linux AppImage (Docker)

Only Docker and Git are needed; Rust and the packaging tools run inside the container:

```bash
./scripts/build-appimage-container.sh ./EmuSync-x86_64.AppImage
./EmuSync-x86_64.AppImage
```

The output path must not exist yet. Set `CARGO_BUILD_JOBS` to limit compilation
parallelism. The binary is built on Debian Bookworm with pinned `appimagetool` and
AppImage runtime versions. The AppImage carries window-system libraries that the host
may lack and uses the host's graphics drivers.

With Rust, `appimagetool` and `appstreamcli` installed locally,
`APPIMAGETOOL=/path/to/appimagetool ./scripts/build-appimage.sh <output>` packages a
binary built on the host instead.

#### Windows installers

`scripts/build-windows-installers.sh <output-dir>` builds `EmuSync-x86_64.msi` (WiX 4) and
`EmuSync-x86_64-portable.zip` on native Windows (Git Bash). The MSI shares the UpgradeCode of
the Kotlin installers, so it upgrades 1.x installs in place. `scripts/verify-windows-installers.ps1`
installs, upgrades from a previous MSI, reinstalls and removes it and checks the ZIP; CI runs it
on every change. It modifies the machine, so use it only on a disposable one.

---

## CI/CD & Automated Releases

- **CI Validation (`.github/workflows/ci.yml`)**: On Linux and Windows, runs `cargo xtask check` and the script tests; on Linux it also builds the AppImage.
- **Windows Packages (`.github/workflows/windows-packages.yml`)**: Builds the MSI and the portable ZIP, then installs, upgrades from a previous release, reinstalls and uninstalls the MSI on the runner and checks each step. Run it by hand from any branch (Actions tab) to download the packages; it also runs on pull requests that touch the packaging files, and the release workflow runs it first.
- **Publish Release (`.github/workflows/release.yml`)**: Manual trigger (`workflow_dispatch`) on `main` that:
  1. Prompts for the SemVer release type (`patch`, `minor`, `major`).
  2. Bumps the Cargo workspace version and creates the git tag (`vX.Y.Z`).
  3. Builds `EmuSync-x86_64.AppImage`, and `EmuSync-x86_64.msi` plus `EmuSync-x86_64-portable.zip` after verifying them on Windows (upgrade from the newest release with an MSI, reinstall, uninstall).
  4. Creates the GitHub Release with automated changelogs and attaches all three. The release is not published if any build or verification fails.

---

## License

MIT License. See [LICENSE](LICENSE) for details.
