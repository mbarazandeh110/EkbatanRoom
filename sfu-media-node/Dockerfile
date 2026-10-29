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
    RTC_MIN_PORT=40000 \
    RTC_MAX_PORT=49999 \
    GRPC_PORT=50051 \
    OBS_LOG_LEVEL=info \
    OBS_LOG_FORMAT=json \
    OBS_METRICS_ENABLED=true \
    OBS_METRICS_ADDR=0.0.0.0:9090

# Expose gRPC control plane port, Prometheus metrics port, and RTC media UDP range
EXPOSE 50051/tcp
EXPOSE 9090/tcp
EXPOSE 40000-49999/udp

# Healthcheck via Prometheus metrics endpoint (skips check if metrics are disabled, extracts port dynamically)
HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD sh -c 'PORT="${OBS_METRICS_ADDR##*:}"; PORT="${PORT:-9090}"; [ "$OBS_METRICS_ENABLED" = "false" ] || curl -f "http://localhost:${PORT}/metrics" || exit 1'

# Run as non-root user
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/sfu-media-node"]
