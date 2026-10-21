# ==============================================================================
# Multi-stage Dockerfile for sfu-media-node (Rust + Mediasoup SFU)
# ==============================================================================

# --- Stage 1: Builder ---
FROM rust:1-bookworm AS builder

WORKDIR /usr/src/app

ENV PIP_BREAK_SYSTEM_PACKAGES=1 \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

# Install rustfmt component (required by planus-codegen in mediasoup-sys build)
RUN rustup component add rustfmt

# Install native dependencies required for building mediasoup C++ worker
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    python3-pip \
    meson \
    ninja-build \
    build-essential \
    pkg-config \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install python invoke package globally for mediasoup-sys build
RUN pip3 install --no-cache-dir invoke

# Copy manifest and sources
COPY Cargo.toml Cargo.lock* build.rs ./
COPY proto ./proto
COPY src ./src

# Build release binary
RUN cargo build --release

# --- Stage 2: Minimal Runtime ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

# Create unprivileged application user and group
RUN groupadd -g 10001 sfu && \
    useradd -u 10001 -g sfu -s /bin/false -M -d /nonexistent sfu

WORKDIR /app

# Copy compiled binary from builder
COPY --from=builder /usr/src/app/target/release/sfu-media-node /usr/local/bin/sfu-media-node

# SFU networking and runtime environment defaults
ENV MEDIASOUP_LISTEN_IP=0.0.0.0 \
    RTC_MIN_PORT=40000 \
    RTC_MAX_PORT=49999 \
    GRPC_PORT=50051 \
    RUST_LOG=info

# Expose gRPC control plane port and RTC media UDP range
EXPOSE 50051/tcp
EXPOSE 40000-49999/udp

# Run as non-root user
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/sfu-media-node"]
