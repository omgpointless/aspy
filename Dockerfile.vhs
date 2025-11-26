# Dockerfile.vhs - Docker image for recording anthropic-spy demo with VHS
#
# This builds on the VHS image (which has ffmpeg, ttyd, etc.) and adds Rust.
# We then build the project inside the container so VHS can record it.

FROM ghcr.io/charmbracelet/vhs:latest

# Install Rust toolchain
# VHS image is Debian-based, so we use rustup
# --allow-releaseinfo-change handles Debian codename transitions
RUN apt-get update --allow-releaseinfo-change && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust via rustup (non-interactive)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Set working directory
WORKDIR /app

# Copy project files
# We'll do this at build time so the image contains the built binary
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/

# Build release binary (this takes a while but only happens once)
RUN cargo build --release

# The binary is now at /app/target/release/anthropic-spy
# VHS can run it directly

# Default command - run VHS with a tape file
# The tape file will be mounted at runtime
ENTRYPOINT ["vhs"]
