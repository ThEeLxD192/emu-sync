# Builds the Linux AppImage on Debian Bookworm; only Docker is required on the host.
# Stage "build": EmuSync binary and runtime fallback libraries.
FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS build
# Optional: limits Cargo parallelism when passed with --build-arg.
ARG CARGO_BUILD_JOBS
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY rust ./rust
COPY xtask ./xtask
RUN cargo build --release --locked -p emusync-desktop-app && \
    install -m 755 target/release/emusync-desktop-app /usr/local/bin/EmuSync

# Window-system libraries only. Graphics loaders and drivers remain supplied by the host.
COPY packaging/linux/probe-shared-library.c ./packaging/linux/probe-shared-library.c
RUN apt-get update && apt-get install -y --no-install-recommends \
      libwayland-client0 libwayland-cursor0 libwayland-egl1 \
      libxkbcommon0 libxkbcommon-x11-0 libxcb-xkb1 libxcb1 \
      libxau6 libxdmcp6 libbsd0 libmd0 libffi8 && \
    mkdir -p /opt/emusync-runtime/usr/bin /opt/emusync-runtime/usr/lib/emusync-fallback \
      /opt/emusync-runtime/usr/share/doc /opt/emusync-runtime/usr/share/emusync && \
    cc -O2 -Wall -Wextra -Werror packaging/linux/probe-shared-library.c -ldl \
      -o /opt/emusync-runtime/usr/bin/emusync-probe-library && \
    for library in libffi.so.8 libmd.so.0 libbsd.so.0 libXau.so.6 libXdmcp.so.6 \
      libxcb.so.1 libxcb-xkb.so.1 libxkbcommon.so.0 libxkbcommon-x11.so.0 \
      libwayland-client.so.0 libwayland-cursor.so.0 libwayland-egl.so.1; do \
      mkdir -p "/opt/emusync-runtime/usr/lib/emusync-fallback/$library" && \
      install -m 644 "$(readlink -f "/usr/lib/x86_64-linux-gnu/$library")" \
        "/opt/emusync-runtime/usr/lib/emusync-fallback/$library/$library" && \
      printf '%s\n' "$library" >> /opt/emusync-runtime/usr/share/emusync/fallback-libraries.txt; \
    done && \
    for package in libwayland-client0 libwayland-cursor0 libwayland-egl1 \
      libxkbcommon0 libxkbcommon-x11-0 libxcb-xkb1 libxcb1 \
      libxau6 libxdmcp6 libbsd0 libmd0 libffi8; do \
      mkdir -p "/opt/emusync-runtime/usr/share/doc/$package" && \
      cp "/usr/share/doc/$package/copyright" "/opt/emusync-runtime/usr/share/doc/$package/"; \
    done && \
    mkdir -p /opt/emusync-runtime/usr/share/doc/emusync-runtime && \
    dpkg-query -W -f='${Package}=${Version}\n' \
      libwayland-client0 libwayland-cursor0 libwayland-egl1 \
      libxkbcommon0 libxkbcommon-x11-0 libxcb-xkb1 libxcb1 \
      libxau6 libxdmcp6 libbsd0 libmd0 libffi8 \
      > /opt/emusync-runtime/usr/share/doc/emusync-runtime/debian-packages.txt && \
    rm -rf /var/lib/apt/lists/*

# Stage "tools": pinned packaging tools, cached independently of the sources.
FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS tools
ARG APPIMAGETOOL_VERSION=1.9.1
ARG APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
ARG APPIMAGE_RUNTIME_VERSION=20251108
ARG APPIMAGE_RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
RUN apt-get update && apt-get install -y --no-install-recommends appstream && \
    rm -rf /var/lib/apt/lists/* && \
    mkdir -p /opt/appimage && cd /opt/appimage && \
    curl -fsSL -o appimagetool.AppImage \
      "https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-x86_64.AppImage" && \
    curl -fsSL -o runtime-x86_64 \
      "https://github.com/AppImage/type2-runtime/releases/download/${APPIMAGE_RUNTIME_VERSION}/runtime-x86_64" && \
    printf '%s  %s\n' "$APPIMAGETOOL_SHA256" appimagetool.AppImage "$APPIMAGE_RUNTIME_SHA256" runtime-x86_64 | \
      sha256sum -c - && \
    chmod +x appimagetool.AppImage && \
    ./appimagetool.AppImage --appimage-extract >/dev/null && \
    mv squashfs-root appimagetool && rm appimagetool.AppImage

# Stage "package": assembles /out/EmuSync-x86_64.AppImage.
FROM tools AS package
WORKDIR /src
COPY --from=build /usr/local/bin/EmuSync /usr/local/bin/EmuSync
COPY --from=build /opt/emusync-runtime /opt/emusync-runtime
COPY packaging ./packaging
COPY scripts/build-appimage.sh ./scripts/build-appimage.sh
RUN mkdir /out && \
    EMUSYNC_BINARY=/usr/local/bin/EmuSync EMUSYNC_RUNTIME_DIR=/opt/emusync-runtime \
    APPIMAGETOOL=/opt/appimage/appimagetool/AppRun APPIMAGE_RUNTIME_FILE=/opt/appimage/runtime-x86_64 \
      bash scripts/build-appimage.sh /out/EmuSync-x86_64.AppImage
