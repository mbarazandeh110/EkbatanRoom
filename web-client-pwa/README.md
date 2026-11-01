# EkbatanRoom Web Client PWA (`web-client-pwa`)

An enterprise-grade, progressive web application (PWA) and video conferencing client built with **Next.js 15 (App Router)**, **React 19**, **TypeScript** (`strict: true`), **Zustand**, and **Tailwind CSS**.

This repository is designed around a **Dual-Protocol Hexagonal Frontend Architecture**, completely decoupling the user interface and reactive state management from underlying WebRTC media transport SDKs.

---

## Table of Contents

- [Architectural Overview](#architectural-overview)
- [Key Features](#key-features)
- [Hexagonal Layout & SOLID Principles](#hexagonal-layout--solid-principles)
- [Granular State Management (Zero Re-render Lag)](#granular-state-management-zero-re-render-lag)
- [Mobile & iOS Safari Resilience](#mobile--ios-safari-resilience)
- [Adaptive Viewport & Bandwidth Optimization](#adaptive-viewport--bandwidth-optimization)
- [Observability, Security & Metric Cardinality](#observability-security--metric-cardinality)
- [Accessibility (WCAG 2.2 AA)](#accessibility-wcag-22-aa)
- [Directory Structure](#directory-structure)
- [Environment Variables](#environment-variables)
- [Getting Started](#getting-started)
- [Testing & Quality Verification](#testing--quality-verification)
- [Deployment & Twelve-Factor Compliance](#deployment--twelve-factor-compliance)

---

## Architectural Overview

The frontend architecture strictly enforces inward dependency flow (`components/adapters -> application/store -> ports -> domain`):

```
+--------------------------------------------------------------------------+
|                       Components & Next.js App Router                    |
|          (PreJoinLobby, VideoGrid, VideoTile, ControlBar, Pages)         |
+------------------------------------+-------------------------------------+
                                     |
                                     v
+------------------------------------+-------------------------------------+
|                  Application & Granular Zustand Stores                   |
|   (RoomSessionController, AdaptiveViewportManager, useRoomStore, etc.)   |
+------------------------------------+-------------------------------------+
                                     |
                                     v
+------------------------------------+-------------------------------------+
|                        Segregated Ports (ISP)                            |
|     (RtcConnectionPort, LocalMediaControlPort, RemoteSubscriptionPort)   |
+------------------------------------+-------------------------------------+
                                     |
                                     v
+------------------------------------+-------------------------------------+
|                     Pure Domain Models & Errors                          |
|         (RoomState, ParticipantModel, TrackDescriptor, DomainError)      |
+--------------------------------------------------------------------------+
```

### Dual-Protocol Engine
The application communicates with the media engine exclusively through the abstracted `RtcEnginePort`. Concrete implementations are decoupled into the infrastructure/adapter layer:
1. **LiveKit Protocol Adapter (`/rtc`):** Uses the exact pinned `livekit-client` (`2.23.0`) matching the black-box `rtc-compatibility-suite`.
2. **Mediasoup Native ORTC Adapter (`/ws/ortc`):** Swappable skeleton conforming to `RtcEnginePort` for native ORTC WebSocket signaling.
3. **Mock Adapter:** Full in-memory mock engine for unit testing and offline development with zero SDK coupling.

---

## Key Features

- **Decoupled Hexagonal Core:** No WebRTC SDK imports inside `domain/`, `ports/`, `store/`, or `components/`.
- **Zero Re-render Lag in High-Density Rooms:** High-frequency audio levels and active speaker updates are isolated to atomic Zustand selectors without re-rendering the whole video grid.
- **Client-Side Adaptive Stream:** Real-time DOM width and visibility monitoring via `IntersectionObserver` and `ResizeObserver`, automatically pausing off-screen feeds and requesting optimal Simulcast layers (`low` / `medium` / `high`).
- **iOS Safari & Mobile Hardening:** Pre-Join Lobby ensures explicit user interaction to resume `AudioContext` (bypassing autoplay blocks) and requests `navigator.wakeLock` to prevent screen sleep.
- **Production Observability & QoE:** Anomaly-driven quality monitoring, metric label cardinality guards, and automated sanitization of JWTs, credentials, and raw SDP.
- **Accessible & Responsive:** WCAG 2.2 AA compliant with ARIA live regions, keyboard navigation shortcuts, and focus indicators.

---

## Hexagonal Layout & SOLID Principles

- **Single Responsibility Principle (SRP):** Each module has a focused domain responsibility:
  - `AdaptiveViewportManager` computes debounced viewport sizes and quality selections.
  - `useMediaTrackAttachment` manages `<video>` track bindings and memory teardown.
  - `RoomSessionController` orchestrates lifecycle transitions.
- **Open/Closed Principle (OCP):** New media transport protocols can be plugged into `src/adapters/` without touching application or UI code.
- **Liskov Substitution Principle (LSP):** `LiveKitEngineAdapter`, `MediasoupOrtcEngineAdapter`, and `MockRtcEngineAdapter` are fully interchangeable implementations of `RtcEnginePort`.
- **Interface Segregation Principle (ISP):** Port interfaces are segregated into `RtcConnectionPort`, `LocalMediaControlPort`, `RemoteSubscriptionPort`, and `RtcEventsPort`.
- **Dependency Inversion Principle (DIP):** High-level application modules depend upon port abstractions, instantiated via `src/adapters/factory.ts`.

---

## Granular State Management (Zero Re-render Lag)

Global reactive state is split into three independent Zustand stores:

1. **`useRoomStore` (`src/store/use_room_store.ts`):**
   - Tracks connection status (`disconnected`, `connecting`, `connected`, `reconnecting`, `failed`), room identifier, layout mode (`grid` | `speaker`), and participant ID lists.
2. **`useParticipantsStore` (`src/store/use_participants_store.ts`):**
   - Normalized participant models and track descriptors.
   - Provides atomic selectors:
     - `useParticipant(id)`
     - `useParticipantTrack(id, source)`
     - `useParticipantQuality(id)`
3. **`useTelemetryStore` (`src/store/use_telemetry_store.ts`):**
   - High-frequency store receiving audio levels (`0.0` - `1.0`) and active speaker IDs.
   - Atomic selectors ensure that an active speaker border change only re-renders the specific `VideoTile`, preventing video canvas drops.

---

## Mobile & iOS Safari Resilience

To guarantee 100% stability on iOS Safari and mobile browsers:

1. **Mandatory Pre-Join Lobby:**
   - The user checks their camera and microphone preview before joining.
   - The explicit "Join Room" button triggers `AudioContext.resume()`, satisfying iOS autoplay requirements for remote audio playback.
   - The lobby stream is stopped before joining the live room to release hardware encoders.
2. **Screen WakeLock:**
   - Requests `navigator.wakeLock.request('screen')` on room entry to prevent mobile screens from dimming or sleeping during calls.
3. **HTML5 `<video>` Attributes:**
   - All video tags enforce `playsInline autoPlay` (and `muted` for the local user).

---

## Adaptive Viewport & Bandwidth Optimization

To minimize client CPU and network bandwidth consumption:

1. **`useAdaptiveVideoViewport` Hook:**
   - Attaches `IntersectionObserver` and `ResizeObserver` to each remote participant tile.
2. **Dynamic Quality Layer Calculation:**
   - Element width `< 280px`: requests `VideoQualityLayer.LOW` (180p).
   - Element width `280px - 640px`: requests `VideoQualityLayer.MEDIUM` (360p).
   - Element width `> 640px` (or pinned/fullscreen): requests `VideoQualityLayer.HIGH` (720p/1080p).
3. **Off-Screen Pause:**
   - When a video tile scrolls out of view or changes page, the feed is automatically paused via `subscriptionPort.setTrackEnabled(trackId, false)`.

---

## Observability, Security & Metric Cardinality

The telemetry pipeline implemented in `src/adapters/telemetry/configurable_telemetry_adapter.ts` enforces:

1. **Configuration via Environment Variables:**
   - Log level (`trace`, `debug`, `info`, `warn`, `error`).
   - Format (`json` for production, `pretty` for local development).
2. **Media Hot-Path Protection:**
   - Never logs or generates spans per RTP packet or unthrottled audio level tick.
3. **Strict Metric Cardinality Guards:**
   - `room_id`, `peer_id`, `transport_id`, `request_id`, and `user_id` are strictly prohibited in Prometheus/VictoriaMetrics metric labels (retained only in structured JSON logs and trace spans).
4. **Automated Security Redaction:**
   - Redacts JWTs, Bearer headers, `access_token`, `ice_pwd`, and passwords to `[REDACTED]`.
   - Raw SDP is redacted to `[REDACTED_SDP]` unless `OBS_LOG_RAW_SDP=true` at `debug` level.
5. **Anomaly-Driven QoE Reporting:**
   - Automatically records warnings when packet loss exceeds 2%, RTT exceeds 250ms, video freezes occur, or connection state changes.

---

## Accessibility (WCAG 2.2 AA)

- **Keyboard Shortcuts:** Global shortcuts allow toggling the microphone (`m`) and camera (`v`) without clicking.
- **ARIA Attributes:** All interactive buttons in `ControlBar` specify explicit `aria-label`, `aria-pressed`, and `focus-visible:ring-2` focus rings.
- **Live Announcements:** An `aria-live="polite"` region alerts screen readers to state changes (e.g., mute status, layout changes).

---

## Directory Structure

```
src/
├── domain/                        # Pure domain models & typed errors
│   ├── models/
│   │   ├── room.ts                # RoomState, ConnectionStatus, LayoutMode
│   │   ├── participant.ts         # ParticipantModel, ConnectionQualityLevel
│   │   └── media.ts               # TrackDescriptor, TrackSource, VideoQualityLayer
│   └── errors.ts                  # Domain error classes
├── ports/                         # Segregated abstract ports (ISP & DIP)
│   ├── rtc_engine_port.ts         # Segregated RtcConnection, MediaControl, Subscription
│   ├── media_devices_port.ts      # Device enumeration, getUserMedia, WakeLock, AudioContext
│   └── telemetry_port.ts          # Logging, APM tracing, QoE snapshot port
├── application/                   # Use cases & business logic
│   ├── room_session_controller.ts # Room lifecycle, join/leave, track publishing
│   └── adaptive_viewport_manager.ts # Adaptive layer calculation & debounced updates
├── store/                         # Reactive Zustand stores
│   ├── use_room_store.ts          # Room connection status and participant IDs
│   ├── use_participants_store.ts  # Normalized participant state & atomic selectors
│   └── use_telemetry_store.ts     # High-frequency audio levels and active speakers
├── adapters/                      # Concrete infrastructure implementations
│   ├── rtc_livekit/
│   │   └── livekit_adapter.ts     # Production adapter using livekit-client 2.23.0
│   ├── rtc_ortc/
│   │   └── mediasoup_ortc_adapter.ts # Native ORTC WebSocket skeleton
│   ├── mock/
│   │   └── mock_rtc_engine_adapter.ts # Pure in-memory mock engine
│   ├── browser_media/
│   │   └── browser_media_adapter.ts   # Browser Web APIs (AudioContext, WakeLock)
│   ├── telemetry/
│   │   └── configurable_telemetry_adapter.ts # Configurable logs, metrics, redaction
│   └── factory.ts                 # Adapter factory
├── components/                    # UI Components
│   ├── lobby/
│   │   └── PreJoinLobby.tsx       # Pre-join preview, device selection, join action
│   ├── stage/
│   │   ├── VideoGrid.tsx          # Responsive multi-party grid & speaker layout
│   │   ├── VideoTile.tsx          # Participant video tile with atomic selectors
│   │   ├── useMediaTrackAttachment.ts # Safe video element track binding
│   │   └── useAdaptiveVideoViewport.ts # Intersection & Resize observers
│   └── controls/
│       └── ControlBar.tsx         # Accessible control bar with keyboard shortcuts
└── app/                           # Next.js 15 App Router
    ├── page.tsx                   # Landing / room entry
    ├── room/[roomId]/page.tsx     # Conference room page
    ├── api/healthz/route.ts       # Health endpoint (Kubernetes liveness/readiness)
    ├── api/config/route.ts        # Runtime configuration endpoint (Twelve-Factor)
    ├── layout.tsx                 # Root HTML layout
    └── globals.css                # Tailwind directives
```

---

## Environment Variables

| Variable | Description | Default |
|---|---|---|
| `NEXT_PUBLIC_RTC_PROTOCOL` | WebRTC Engine protocol (`livekit`, `ortc`, `mock`) | `livekit` |
| `NEXT_PUBLIC_LIVEKIT_WS_URL` | WebSocket URL for LiveKit signaling (`/rtc`) | `ws://localhost:7880` |
| `NEXT_PUBLIC_ORTC_WS_URL` | WebSocket URL for Native ORTC signaling (`/ws/ortc`) | `ws://localhost:8080/ws/ortc` |
| `OBS_LOG_LEVEL` | Logging level (`trace`, `debug`, `info`, `warn`, `error`) | `info` |
| `OBS_LOG_FORMAT` | Log formatting (`json` for production, `pretty` for dev) | `json` |
| `OBS_LOG_RAW_SDP` | Whether to log raw SDP text at `debug` level (`true`/`false`) | `false` |
| `OBS_METRICS_ENABLED` | Enable Prometheus/VictoriaMetrics metrics | `true` |
| `OBS_TRACING_ENABLED` | Enable OpenTelemetry tracing | `false` |
| `OBS_TRACE_SAMPLE_RATE` | Tracing sampling rate (`0.0` - `1.0`) | `0.1` |

---

## Getting Started

### Prerequisites

- Node.js `>= 20.0.0` (Tested with Node v24)
- npm `>= 10.0.0`

### Installation

```bash
git clone <repository-url>
cd web-client-pwa
npm install
```

### Development Server

```bash
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) to access the landing page.

---

## Testing & Quality Verification

The test suite runs with **Vitest** and **React Testing Library**:

```bash
# Run all unit and component tests
npm run test

# Run tests in watch mode
npm run test:watch

# Execute TypeScript strict typecheck
npm run typecheck

# Build optimized production bundle
npm run build
```

---

## Deployment & Twelve-Factor Compliance

1. **Factor III (Config):** The client dynamically resolves runtime configuration through `/api/config`, enabling a single container build to deploy seamlessly across dev, staging, and production environments.
2. **Factor VI (Stateless Processes):** Built with `output: 'standalone'` in `next.config.mjs` for lightweight Docker containerization.
3. **Factor XI (Logs):** All client and SSR events are output as structured JSON without sensitive credentials or high-cardinality label pollution.
4. **Health Check Probes:** Kubernetes liveness and readiness probes query `/api/healthz`, which returns service status and standard `X-Request-ID` headers.
