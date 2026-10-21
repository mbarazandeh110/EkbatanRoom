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
name: twelve-factor-signaling-coordinator
description: Twelve-Factor specialization for the Go signaling coordinator and LiveKit protocol adapter.
---
# Twelve-Factor — `signaling-coordinator`

Apply `twelve-factor-engineering` plus this specialization. The coordinator owns signaling/control-plane logic; RTP media processing stays in `sfu-media-node`.

- **I Codebase:** maintain a single source of truth for protobuf and reproducible generation. Keep adapters, room/participant coordination and media-engine client boundaries explicit.
- **II Dependencies:** keep `go.mod`/`go.sum` consistent; pin code-generation tools and compatibility-critical client/protocol versions.
- **III Config:** validate listener and media-node endpoints, auth keys/issuer/audience, TLS, deadlines, limits, metrics and logging. Inject secrets, redact errors. Do not assume fixed `mid`, room IDs or endpoints as universal protocol rules.
- **IV Backing services:** all outbound RPCs have deadlines; define retryable errors and bounded safe retries. Specify idempotency for duplicate/conflicting resource creation. Handle late events and media-node outages without orphaned resources.
- **V Build/release/run:** build immutable versioned binaries/images; do not download dependencies or regenerate code at startup.
- **VI Processes:** document room/participant state ownership and restart behavior. In-memory state is not shared between replicas; define affinity/routing or explicit external coordination if scaling horizontally.
- **VII Port binding:** document WebSocket/HTTP, health, metrics and internal/admin listeners; protect control-plane endpoints and define TLS/proxy assumptions.
- **VIII Concurrency:** serialize conflicting SDP operations per participant and ordered state transitions per room. Bound queues/goroutines, use deadlines and overload behavior. Replicas require explicit ownership/routing.
- **IX Disposability:** on SIGTERM mark unready, stop new joins/resource creation, finish or cancel in-flight operations within deadlines and clean owned media resources. Handle duplicate close, disconnect races, late responses and partial failure.
- **X Dev/prod parity:** test against the exact pinned official LiveKit client and shared contracts. Join/WebSocket/SDP success alone does not prove compatibility.
- **XI Logs:** propagate correlation IDs across WebSocket, room/participant operations and gRPC calls. Redact tokens and sensitive SDP/candidate material.
- **XII Admin:** room inspection/cleanup is explicit, protected, auditable and scoped; cleanup should be idempotent where possible.

## Acceptance checklist
- [ ] Required config validates and secrets are redacted.
- [ ] Every outbound RPC has deadline/error/retry semantics.
- [ ] Duplicate/conflicting requests are deterministic.
- [ ] Participant SDP operations are serialized.
- [ ] Room ownership and replica-routing assumptions are documented.
- [ ] Late events/partial failures do not leave orphaned media resources.
- [ ] Shutdown stops new work and performs bounded cleanup.
- [ ] Logs correlate services without leaking credentials.
- [ ] Pinned official SDK and real media tests demonstrate compatibility.
