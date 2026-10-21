---
name: webrtc-gate-testing
description: Guidance for building automated End-to-End WebRTC gate tests with Playwright and the official livekit-client package.
---

# Skill: WebRTC Gate Testing with Playwright + livekit-client

1. In `playwright.config.ts` pass these Chromium flags so headless media streams work:
   - `--use-fake-ui-for-media-stream`
   - `--use-fake-device-for-media-stream`
   - `--allow-loopback-in-peer-connection`

2. Create a small Vite + TypeScript test page that exposes the LiveKit `Room` instance on `window.lkRoom`. Playwright scripts can then call:
   - `room.localParticipant.setCameraEnabled(true)`
   - `room.localParticipant.setMicrophoneEnabled(true)`
   - and read real `RTCPeerConnection.getStats()` values (bytesSent, bytesReceived, frameWidth, frameHeight, iceConnectionState).

3. Keep the official `livekit-client` version pinned in package.json. Never apply source patches.

4. Each test file must produce a clear Pass/Fail with the collected evidence (signaling log excerpts + RTP counters).
