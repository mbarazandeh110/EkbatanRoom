# EkbatanRoom Signaling Coordinator

High-performance Go control plane, dual-gateway signaling server, and room coordinator for the EkbatanRoom WebRTC SFU ecosystem.

---

## Architecture Overview

`signaling-coordinator` bridges client WebRTC signaling protocols with the low-level Rust media engine (`sfu-media-node`). It is built strictly adhering to **Hexagonal Architecture (Ports and Adapters)**, the **Actor Model**, and **Twelve-Factor App** principles.

```mermaid
flowchart TD
    subgraph Clients["WebRTC Clients"]
        LKClient["LiveKit Client SDKs<br/>(Web / iOS / Android)"]
        ORTCClient["Native ORTC Clients<br/>(PWA / Custom)"]
    end

    subgraph Inbound["Inbound Adapters (Dual Gateway)"]
        LKAdapter["LiveKitWSHandler<br/>(/rtc)"]
        ORTCAdapter["NativeOrtcHandler<br/>(/ws/ortc)"]
    end

    subgraph Core["Core Application & Domain (Hexagonal Core)"]
        RManager["RoomManager<br/>(Lifecycle & Routing)"]
        subgraph Actors["Actor Model (One per Room)"]
            RActor["RoomActor<br/>(Channel Mailbox / Lock-free)"]
            PSession["ParticipantSession<br/>(Stable ⇄ WaitingForAnswer)"]
            GlareQueue["Glare Prevention Queue<br/>(Mid-negotiation buffer)"]
            SDPSlots["Unified Plan Slot Manager<br/>(Port 0 Inactivation & Recycling)"]
        end
    end

    subgraph Outbound["Outbound Adapters"]
        GRPCAdapter["MediaNodeClient (gRPC)<br/>(proto/sfu/v1/media_engine.proto)"]
        TelemetryAdapter["Telemetry & APM<br/>(OTel / Slog / VictoriaMetrics)"]
    end

    subgraph MediaNode["Media Plane"]
        RustSFU["sfu-media-node (Rust)<br/>libmediasoup-sys / WebRTC Worker"]
    end

    subgraph Observability["Observability Platform"]
        VM["VictoriaMetrics / Prometheus<br/>(/metrics)"]
        Jaeger["OpenTelemetry Collector<br/>(W3C traceparent)"]
    end

    LKClient -->|WebSocket (Protobuf)| LKAdapter
    ORTCClient -->|WebSocket (JSON)| ORTCAdapter

    LKAdapter -->|RoomCoordinatorPort| RManager
    ORTCAdapter -->|RoomCoordinatorPort| RManager

    RManager --> RActor
    RActor --> PSession
    PSession --> GlareQueue
    PSession --> SDPSlots

    RActor -->|RouterEnginePort<br/>TransportEnginePort<br/>MediaStreamPort| GRPCAdapter
    GRPCAdapter -->|gRPC + W3C Tracing| RustSFU

    LKAdapter -.->|Metrics & Spans| TelemetryAdapter
    RActor -.->|Metrics & Spans| TelemetryAdapter
    GRPCAdapter -.->|Metrics & Spans| TelemetryAdapter

    TelemetryAdapter --> VM
    TelemetryAdapter --> Jaeger
```

---

## Key Features

1. **Dual Gateway Architecture**:
   - `/rtc`: Fully compliant with LiveKit signaling protocol (Protocol v12), enabling off-the-shelf LiveKit client SDKs (JS, React, Flutter, Swift, Kotlin).
   - `/ws/ortc`: Native ORTC signaling skeleton providing zero-SDP capability.
2. **Actor Model per Room (`RoomActor`)**:
   - Sequential, lock-free message processing via single-goroutine channel mailboxes eliminating data races (`go test -race` clean).
3. **Deterministic WebRTC State Machine**:
   - State transition enforcement: `Stable` &harr; `WaitingForAnswer`.
   - Mid-negotiation glare protection: Tracks published during in-flight renegotiations are queued and flushed once the peer returns to `Stable`.
4. **Unified Plan SDP Slot Recycling**:
   - Removed or muted tracks are inactivated (`port 0`, `a=inactive`) rather than deleted, preventing `m=` line index drift.
   - Deterministic answer DTLS negotiation with explicit `a=setup:passive`.
5. **Enterprise Observability & Security**:
   - Structured JSON logging (`log/slog`) with automatic trace context correlation.
   - Automatic security redaction for sensitive keys (`token`, `access_token`, `ice_pwd`, `secret`, `credential`).
   - SDP payload redaction unless `OBS_LOG_RAW_SDP=true` and level is `debug`.
   - Low-cardinality Prometheus / VictoriaMetrics metrics exporter on `/metrics`.
   - Zero media hot-path interference (no per-packet logging or unthrottled audio tick spans).

---

## Directory Structure

```
signaling-coordinator/
├── cmd/
│   └── coordinator/
│       └── main.go                  # Service composition & Constructor DI
├── gen/
│   └── proto/sfu/v1/                # Generated Go gRPC & Protobuf stubs
├── internal/
│   ├── domain/                      # Pure domain models (zero external dependencies)
│   │   ├── errors.go                # Domain errors
│   │   ├── models.go                # Pure data structures (TransportInfo, IceServer, etc.)
│   │   ├── participant.go           # Participant aggregate
│   │   ├── room.go                  # Room aggregate
│   │   └── sdp_slot.go              # Unified Plan SDP slot manager
│   ├── ports/                       # Segregated Interface Ports (ISP)
│   │   ├── inbound.go               # RoomCoordinatorPort, PeerResponderPort
│   │   └── outbound.go              # RouterEnginePort, TransportEnginePort, MediaStreamPort
│   ├── application/                 # Application use cases & Actor engine
│   │   ├── participant_session.go   # State machine & Glare queue
│   │   ├── room_actor.go            # Room actor & mailbox processor
│   │   ├── room_manager.go          # Room lifecycle coordinator
│   │   └── room_actor_test.go       # Actor & 2-peer publish/subscribe tests
│   └── adapters/
│       ├── inbound/
│       │   ├── livekit/             # /rtc LiveKit adapter & SDP synthesizer
│       │   │   ├── responder.go     # LiveKit protobuf responder
│       │   │   ├── sdp_mapper.go    # SDP parser & synthesizer (pion/sdp)
│       │   │   ├── ws_handler.go    # /rtc WebSocket gateway
│       │   │   ├── sdp_mapper_test.go
│       │   │   └── ws_handler_test.go
│       │   └── native_ortc/         # /ws/ortc Native ORTC adapter skeleton
│       │       └── ortc_handler.go
│       └── outbound/
│           ├── media_node_grpc/     # Outbound gRPC adapter to sfu-media-node
│           │   ├── client.go
│           │   └── client_test.go
│           └── telemetry/           # Observability, Logging, APM & Metrics
│               ├── config.go        # OBS_* environment variable loader
│               ├── logger.go        # Slog with trace injection & security redaction
│               ├── metrics.go       # Low-cardinality Prometheus collectors
│               ├── tracer.go        # OpenTelemetry tracer provider & W3C propagation
│               └── telemetry_test.go
├── proto/sfu/v1/
│   └── media_engine.proto           # Authoritative gRPC contract
├── Dockerfile                       # Production multi-stage Alpine container
├── Makefile                         # Automation targets
├── go.mod
└── go.sum
```

---

## Configuration

All configuration is provided via environment variables (Twelve-Factor App):

| Variable | Default | Description |
|---|---|---|
| `PORT` | `7880` | HTTP/WebSocket signaling server port |
| `SFU_GRPC_ADDR` | `127.0.0.1:50051` | Target address of `sfu-media-node` gRPC server |
| `OBS_LOG_LEVEL` | `info` | Minimum log level (`debug`, `info`, `warn`, `error`) |
| `OBS_LOG_FORMAT` | `json` | Log output format (`json`, `text`) |
| `OBS_LOG_RAW_SDP` | `false` | When `true` AND log level is `debug`, logs raw SDP offers/answers |
| `OBS_TRACING_ENABLED` | `false` | Enable OpenTelemetry distributed tracing |
| `OBS_TRACE_SAMPLE_RATE` | `1.0` | Head-based trace sampling ratio (`0.0` to `1.0`) |
| `OBS_OTLP_ENDPOINT` | `""` | OTLP gRPC collector endpoint (e.g. `otel-collector:4317`) |
| `OBS_METRICS_ENABLED` | `true` | Expose VictoriaMetrics / Prometheus metrics on `/metrics` |

---

## HTTP & WebSocket Endpoints

| Endpoint | Protocol | Description |
|---|---|---|
| `/rtc` | WebSocket (Binary/Protobuf) | LiveKit signaling gateway (compatible with LiveKit Web/Mobile SDKs) |
| `/ws/ortc` | WebSocket (JSON/Binary) | Native ORTC signaling gateway |
| `/healthz` | HTTP GET | Liveness and readiness probe (returns `200 ok`) |
| `/metrics` | HTTP GET | Prometheus / VictoriaMetrics scrapable metrics exporter |

---

## Development & Testing

### Prerequisites

- **Go**: 1.23 or newer
- **Protoc**: 3.20+ (only required if modifying `.proto` files)
- **Docker**: For containerized deployment

### Quickstart

```bash
# Display help and all available Makefile targets
make help

# Run unit tests across all packages
make test

# Run all tests with race condition detector
make test-race

# Run test coverage analysis
make test-coverage

# Build static binary into ./bin/signaling-coordinator
make build

# Run coordinator locally
make run
```

### Regenerating Protobuf

```bash
make proto
```

---

## Docker Deployment

Build and run using the optimized, unprivileged multi-stage Docker image:

```bash
# Build production Docker image
make docker-build

# Run container locally on port 7880
make docker-run
```

Or run manually:

```bash
docker run --rm -it \
  -p 7880:7880 \
  -e SFU_GRPC_ADDR=sfu-media-node:50051 \
  -e OBS_LOG_LEVEL=info \
  -e OBS_LOG_FORMAT=json \
  -e OBS_METRICS_ENABLED=true \
  ekbatan-room/signaling-coordinator:latest
```

---

## Production Observability & APM

### 1. Prometheus / VictoriaMetrics Metrics (`/metrics`)

The following low-cardinality metrics are exported:

- `signaling_active_rooms`: Current number of active rooms (Gauge).
- `signaling_active_participants{protocol="livekit|native_ortc"}`: Active participants by gateway (Gauge).
- `signaling_ws_messages_total{protocol="...", type="...", status="..."}`: Total WebSocket messages processed (Counter).
- `signaling_sdp_negotiation_duration_seconds{role="publisher|subscriber"}`: Histogram of SDP offer/answer cycle latency (Histogram).
- `signaling_sdp_glare_queued_total`: Total number of renegotiation offers queued to prevent glare (Counter).
- `signaling_grpc_client_duration_seconds{method="...", status="..."}`: Histogram of outbound gRPC calls to media node (Histogram).
- `signaling_errors_total{type="..."}`: Total signaling errors partitioned by type (Counter).

> **Cardinality Guard:** High-cardinality keys (`room_id`, `peer_id`, `transport_id`, `request_id`) are **strictly prohibited** from metric labels. They are recorded exclusively in structured JSON log fields and OpenTelemetry span attributes.

### 2. Distributed Tracing

Outbound gRPC connections to `sfu-media-node` automatically attach W3C `traceparent` headers via `otelgrpc.NewClientHandler()`, propagating distributed traces end-to-end between the Go control plane and the Rust SFU media plane.

---

## License

Internal proprietary software of the EkbatanRoom project.
