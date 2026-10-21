---
name: twelve-factor-engineering
description: Shared Twelve-Factor App engineering checklist for all EkbatanRoom repositories.
---
# Twelve-Factor Engineering — Shared Skill

Use alongside the repository's `GEMINI.md`, rules, blueprint, ADRs, and project-specific skills. Refer to https://12factor.net/. For every meaningful change, assess all 12 factors as Compliant, Not applicable, Exception, or Needs work. Inspect first, make focused changes, add automated checks where practical, and report evidence. Do not claim compliance merely because a project uses containers or environment variables.

## Factors
1. **Codebase:** one authoritative version-controlled codebase per project; define one source of truth for shared contracts, especially protobuf.
2. **Dependencies:** declare and lock dependencies; Rust `Cargo.lock`, Go `go.mod`/`go.sum`, JS lockfile and deterministic CI installs. Pin compatibility-critical SDK/browser versions.
3. **Config:** inject deploy-specific config at runtime; validate required settings; keep secrets out of source, examples, errors and logs.
4. **Backing services:** use explicit endpoints/interfaces; configure TLS, credentials and deadlines; bound retries and retry only safe operations.
5. **Build, release, run:** separate build, release configuration and runtime; produce immutable, versioned artifacts; do not download/build/mutate binaries implicitly at startup.
6. **Processes:** process memory is ephemeral and not shared across replicas. Stateful product concepts are allowed, but ownership, persistence/reconstruction and failure semantics must be explicit.
7. **Port binding:** document and configure listeners; distinguish public, internal, health, metrics and admin interfaces; avoid accidental exposure.
8. **Concurrency:** scale according to workload and state model; define capacity, ownership, affinity, backpressure and overload behavior. More replicas do not automatically make stateful resources scalable.
9. **Disposability:** support predictable startup and bounded graceful shutdown; handle termination, stop new work, cancel/drain in-flight work and release resources. Document session impact.
10. **Dev/prod parity:** align runtimes, dependencies, contracts and behavior across local, CI, staging and production; document unavoidable differences.
11. **Logs:** structured logs to stdout/stderr; correlate requests; keep logs distinct from metrics/traces; never log secrets or unnecessary personal data.
12. **Admin processes:** run one-off tasks using the same code/dependencies/config model; make destructive tasks explicit, scoped, auditable and safely repeatable where possible.

## Workflow
- Read current implementation and project rules before changing code.
- Consider all factors, even if some are not applicable.
- Prefer small changes; document justified exceptions and risks in the blueprint or an ADR.
- Add practical CI checks: format, lint, tests, dependency/security checks, config validation, container build and contract checks as relevant.
- Do not add databases, brokers, service meshes or orchestration solely to satisfy Twelve-Factor.
- Do not label a stateful WebRTC workload stateless by ignoring room/transport lifecycle.

## Required task report
- Factors assessed and status.
- Evidence: files, commands and tests actually run.
- Exceptions and reasons.
- Unverified assumptions and risks.
- Startup, shutdown, scaling, observability, security and rollback impact.

---
name: twelve-factor-sfu-media-node
description: Twelve-Factor specialization for the Rust mediasoup SFU media node.
---
# Twelve-Factor — `sfu-media-node`

Apply `twelve-factor-engineering` plus this specialization. Keep this project a media plane: do not move LiveKit JWT, WebSocket signaling, or LiveKit-specific SDP policy into the Rust media engine.

- **I Codebase:** maintain one authoritative protobuf contract; generated code is generated, never manually divergent.
- **II Dependencies:** honor `Cargo.toml`/`Cargo.lock`; make native toolchain/system requirements reproducible; test mediasoup upgrades against integration tests.
- **III Config:** validate bind/advertised addresses, ICE/network settings, media port ranges, resource limits, timeouts, metrics and log settings. Do not assume host networking, interfaces or IPs.
- **IV Backing services:** coordinator gRPC calls have deadlines/cancellation. Define retry ownership and duplicate-RPC behavior; avoid unbounded retries or duplicate resource creation.
- **V Build/release/run:** build a reproducible, versioned immutable artifact; runtime config is external; fail startup clearly on invalid network configuration.
- **VI Processes:** routers, transports, producers, consumers and workers are process-local unless explicitly persisted. Document that node/worker failure may interrupt sessions. Define ownership and idempotent cleanup after disconnect, timeout, partial RPC failure and worker crash.
- **VII Port binding:** document and isolate gRPC, health and metrics listeners. Distinguish bind addresses from advertised ICE/media addresses.
- **VIII Concurrency:** size by measured CPU, memory, packet rate, bitrate, transport count and latency—not connection count alone. Bound resources and queues; define overload behavior and resource ownership. Validate routing to the node owning each resource.
- **IX Disposability:** on SIGTERM, become unready, stop accepting resource creation, close workers/resources within a bounded deadline, and document active-session impact. Test shutdown, worker failure and repeated close.
- **X Dev/prod parity:** test pinned mediasoup/runtime versions and real ICE/DTLS/RTP paths where feasible; document NAT, firewall, MTU and advertised-IP differences.
- **XI Logs:** structured lifecycle/RPC logs with correlation and node/worker/resource IDs; no secrets or raw sensitive payloads; packet-level logs off by default.
- **XII Admin:** diagnostics and cleanup are explicit, protected and scoped; use dry-run for destructive cleanup when feasible.

## Acceptance checklist
- [ ] Invalid required config fails startup safely.
- [ ] Listener exposure and advertised media addresses are documented.
- [ ] Duplicate create/close and partial failures have deterministic cleanup.
- [ ] Deadlines/cancellation do not leak media resources.
- [ ] SIGTERM marks unready and finishes within a documented deadline.
- [ ] Worker/node failure semantics are documented.
- [ ] Capacity claims have reproducible benchmark/load-test evidence.
- [ ] Logs have correlation context and no secrets.
