# syntax=docker/dockerfile:1.7

# -----------------------------------------------------------------------------
# Stage 1: Build static binary
# -----------------------------------------------------------------------------
FROM golang:1.24-alpine AS builder

# Install build dependencies
RUN apk add --no-cache ca-certificates git tzdata

WORKDIR /build

# Cache dependency layer
COPY go.mod go.sum ./
RUN go mod download && go mod verify

# Copy source tree
COPY cmd/ ./cmd/
COPY gen/ ./gen/
COPY internal/ ./internal/

# Build arguments for binary version metadata
ARG VERSION=v0.1.0
ARG GIT_COMMIT=unknown
ARG BUILD_TIME=unknown

# Compile static binary with zero cgo and symbol stripping
RUN CGO_ENABLED=0 GOOS=linux GOARCH=amd64 \
    go build -trimpath \
    -ldflags="-s -w -extldflags '-static' \
              -X main.version=${VERSION} \
              -X main.gitCommit=${GIT_COMMIT} \
              -X main.buildTime=${BUILD_TIME}" \
    -o /bin/signaling-coordinator ./cmd/coordinator

# -----------------------------------------------------------------------------
# Stage 2: Secure, minimal runtime environment
# -----------------------------------------------------------------------------
FROM alpine:3.21 AS runtime

# Install TLS certificates and timezone data
RUN apk add --no-cache ca-certificates tzdata

# Create dedicated unprivileged user (UID/GID 10001)
RUN addgroup -g 10001 -S appgroup && \
    adduser -u 10001 -S appuser -G appgroup -s /sbin/nologin -D

# Copy compiled binary from builder stage
COPY --from=builder --chown=appuser:appgroup /bin/signaling-coordinator /usr/local/bin/signaling-coordinator

# Default environment configuration (Twelve-Factor App)
ENV PORT=7880 \
    SFU_GRPC_ADDR=127.0.0.1:50051 \
    OBS_LOG_LEVEL=info \
    OBS_LOG_FORMAT=json \
    OBS_LOG_RAW_SDP=false \
    OBS_TRACING_ENABLED=false \
    OBS_TRACE_SAMPLE_RATE=1.0 \
    OBS_METRICS_ENABLED=true

# Expose signaling gateway & metrics port
EXPOSE 7880

# Switch to unprivileged runtime user
USER appuser:appgroup

# Liveness probe against healthz endpoint
HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://127.0.0.1:7880/healthz || exit 1

ENTRYPOINT ["/usr/local/bin/signaling-coordinator"]
