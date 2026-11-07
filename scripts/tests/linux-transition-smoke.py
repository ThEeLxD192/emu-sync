#!/usr/bin/env python3
"""Run the opt-in Kotlin-to-Rust AppImage transition test in an isolated directory.

Requires Linux, FUSE, a Hyprland Wayland session, and two development AppImages.
No release service or real configuration is used.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]


def descendant(pid: int, ancestor: int) -> bool:
    for _ in range(30):
        if pid == ancestor:
            return True
        try:
            with open(f"/proc/{pid}/status", encoding="utf-8") as status:
                parent = next(line for line in status if line.startswith("PPid:"))
            pid = int(parent.split()[1])
        except (OSError, StopIteration, ValueError):
            return False
        if pid <= 1:
            return False
    return False


def close_window(window: dict) -> None:
    command = f'hl.dsp.window.close({{ window = "address:{window["address"]}" }})'
    subprocess.run(["hyprctl", "dispatch", command], check=True, stdout=subprocess.DEVNULL)


def run(kotlin: Path, rust: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="emusync-r8-transition-") as directory:
        scratch = Path(directory)
        config = scratch / "config.json"
        ready = scratch / "ready"
        finished = scratch / "finished"
        config.write_text("{}", encoding="utf-8")
        env = os.environ.copy()
        env.pop("APPIMAGE_EXTRACT_AND_RUN", None)
        env.update(
            EMUSYNC_R8_TRANSITION="1",
            EMUSYNC_R8_KOTLIN_IMAGE=str(kotlin),
            EMUSYNC_R8_RUST_IMAGE=str(rust),
            EMUSYNC_R8_LAUNCH_READY=str(ready),
            EMUSYNC_R8_LAUNCH_FINISHED=str(finished),
            EMUSYNC_CONFIG=str(config),
        )
        log_path = scratch / "gradle.log"
        with log_path.open("w", encoding="utf-8") as log:
            process = subprocess.Popen(
                [str(ROOT / "gradlew"), ":platform:desktop:test", "--tests",
                 "com.emusync.update.UpdateRestartTest", "--no-daemon", "--rerun-tasks"],
                cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT,
            )
            window = None
            try:
                deadline = time.monotonic() + 180
                while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.1)
                if not ready.exists():
                    raise RuntimeError("Kotlin launcher did not signal readiness")
                lines = ready.read_text(encoding="utf-8").splitlines()
                parent, target = int(lines[0]), lines[1]
                deadline = time.monotonic() + 15
                while window is None and process.poll() is None and time.monotonic() < deadline:
                    clients = json.loads(subprocess.check_output(["hyprctl", "-j", "clients"]))
                    window = next((client for client in clients
                                   if client.get("title") == "EmuSync"
                                   and descendant(client["pid"], parent)), None)
                    if window is None:
                        time.sleep(0.1)
                if window is None:
                    raise RuntimeError("Updated Rust window did not appear")
                values = Path(f'/proc/{window["pid"]}/environ').read_bytes().split(b"\0")
                variables = dict(item.split(b"=", 1) for item in values if b"=" in item)
                appimage = os.fsdecode(variables.get(b"APPIMAGE", b""))
                appdir = os.fsdecode(variables.get(b"APPDIR", b""))
                if appimage != target or not Path(appdir).name.startswith(".mount_"):
                    raise RuntimeError("The updated target did not run through a FUSE mount")
                close_window(window)
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    clients = json.loads(subprocess.check_output(["hyprctl", "-j", "clients"]))
                    if all(client.get("address") != window["address"] for client in clients):
                        break
                    time.sleep(0.1)
                else:
                    raise RuntimeError("The updated Rust window did not close")
                window = None
                finished.touch()
                code = process.wait(timeout=90)
                if code != 0:
                    raise RuntimeError(f"Gradle exited with status {code}")
                if config.read_text(encoding="utf-8") != "{}":
                    raise RuntimeError("Synthetic configuration changed")
                print("Kotlin installer replaced the isolated AppImage and launched Rust via FUSE.")
            except Exception:
                if window is not None:
                    close_window(window)
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=15)
                log.flush()
                print(log_path.read_text(encoding="utf-8", errors="replace")[-4000:])
                raise


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kotlin_appimage", type=Path)
    parser.add_argument("rust_appimage", type=Path)
    args = parser.parse_args()
    kotlin, rust = args.kotlin_appimage.resolve(strict=True), args.rust_appimage.resolve(strict=True)
    if not kotlin.is_file() or not rust.is_file():
        parser.error("Both AppImages must be files")
    run(kotlin, rust)


if __name__ == "__main__":
    main()
