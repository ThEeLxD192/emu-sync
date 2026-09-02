# Official Eclipse Temurin image with Java 21 (LTS) on Ubuntu Jammy
FROM eclipse-temurin:21-jdk-jammy

# Install required system tools for Compose Desktop packaging and AppImage generation
RUN apt-get update && apt-get install -y --no-install-recommends \
    binutils \
    fakeroot \
    file \
    wget \
    curl \
    libasound2 \
    libgl1-mesa-glx \
    libx11-6 \
    libxext6 \
    libxrender1 \
    libxtst6 \
    libxi6 \
    libfuse2 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install appimagetool (extracted to work inside containers without requiring FUSE)
RUN wget -q https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage -O /tmp/appimagetool.AppImage && \
    chmod +x /tmp/appimagetool.AppImage && \
    cd /tmp && /tmp/appimagetool.AppImage --appimage-extract && \
    mv /tmp/squashfs-root /opt/appimagetool && \
    ln -s /opt/appimagetool/AppRun /usr/local/bin/appimagetool && \
    rm /tmp/appimagetool.AppImage

WORKDIR /app

# Copy Gradle configuration first to leverage Docker layer caching
COPY gradlew gradlew.bat settings.gradle.kts build.gradle.kts gradle.properties ./
COPY gradle ./gradle

# Ensure executable permissions for gradlew
RUN chmod +x gradlew

# Copy source code, resources, and build scripts
COPY src ./src
COPY scripts ./scripts
COPY config.example.json ./

RUN chmod +x scripts/build-dist.sh

# Run the build script to compile and package the AppImage directly to /out
CMD ["/app/scripts/build-dist.sh"]
