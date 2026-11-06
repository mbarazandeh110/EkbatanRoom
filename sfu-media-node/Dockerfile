# ==============================================================================
# Multi-stage Dockerfile for sfu-media-node (Rust + Mediasoup SFU)
# ==============================================================================

# --- Stage 1: Builder ---
FROM rust:1.85.0-bookworm AS builder

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

# Optional Cargo features (e.g. enterprise) can be passed via --build-arg CARGO_FEATURES=enterprise
ARG CARGO_FEATURES=""

# Build release binary (Community edition default, or Enterprise when requested)
RUN if [ -n "$CARGO_FEATURES" ]; then \
        cargo build --release --locked --features "$CARGO_FEATURES"; \
    else \
        cargo build --release --locked; \
    fi

# --- Stage 2: Minimal Runtime ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libstdc++6 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create unprivileged application user and group
RUN groupadd -g 10001 sfu && \
    useradd -u 10001 -g sfu -s /bin/false -M -d /nonexistent sfu

WORKDIR /app

# Copy compiled binary from builder
COPY --from=builder /usr/src/app/target/release/sfu-media-node /usr/local/bin/sfu-media-node

# SFU networking and runtime environment defaults
ENV MEDIASOUP_LISTEN_IP=0.0.0.0 \
    MEDIASOUP_ANNOUNCED_ADDRESS=auto \
    RTC_PORT_RANGE=40000-49999 \
    GRPC_PORT=50051 \
    OBS_LOG_LEVEL=info \
    OBS_LOG_FORMAT=json \
    OBS_METRICS_ENABLED=true \
    OBS_METRICS_ADDR=0.0.0.0:9090

# Expose gRPC control plane port, Prometheus metrics port, and RTC media UDP range
EXPOSE 50051/tcp
EXPOSE 9090/tcp
EXPOSE 40000-49999/udp

# Healthcheck: checks /metrics if enabled, otherwise verifies the process is alive and listening on GRPC_PORT
HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD sh -c 'if [ "$OBS_METRICS_ENABLED" != "false" ]; then PORT="${OBS_METRICS_ADDR##*:}"; PORT="${PORT:-9090}"; curl -sf "http://127.0.0.1:${PORT}/metrics" >/dev/null || exit 1; else HEX_PORT=$(printf "%04X" "${GRPC_PORT:-50051}"); grep -qi ":${HEX_PORT} " /proc/net/tcp /proc/net/tcp6 || exit 1; fi'

# Run as non-root user
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/sfu-media-node"]
