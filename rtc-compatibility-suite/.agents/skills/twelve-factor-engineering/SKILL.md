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
name: twelve-factor-rtc-compatibility-suite
description: Twelve-Factor specialization for Playwright tests against the official LiveKit client.
---
# Twelve-Factor — `rtc-compatibility-suite`

Apply `twelve-factor-engineering` plus this specialization. This is a test workload, not a production media/signaling service. Optimize for reproducibility, isolation, evidence and safe cleanup.

- **I Codebase:** version test scenarios, fixtures and assertions. Never patch the official `livekit-client` to make tests pass.
- **II Dependencies:** pin `livekit-client`, Playwright, browser versions and package dependencies with lockfiles; use deterministic CI installation.
- **III Config:** inject target URLs, disposable credentials, room prefixes, timeouts, browser settings and artifact paths. Validate early; never commit production tokens.
- **IV Backing services:** check coordinator, media node and TURN prerequisites. Distinguish environment setup failures from compatibility failures; use bounded timeouts, not blind retries.
- **V Build/release/run:** separate install/build/setup/run; record commit, suite, SDK, browser and target-environment versions; publish reports as artifacts.
- **VI Processes:** each job is isolated and rerunnable; use unique room/participant identities; do not rely on prior process state.
- **VII Port binding:** local fixture ports are configurable and collision-safe; do not expose debug/fixture servers outside the test environment.
- **VIII Concurrency:** parallel workers use isolated rooms and participants. Bound worker count by browser/network/SFU capacity; separate correctness gates from load tests.
- **IX Disposability:** always teardown clients, rooms, tracks, browser contexts and created resources on pass/failure/timeout. Capture diagnostics before cleanup when useful; report cleanup failures without masking the original failure.
- **X Dev/prod parity:** test the exact pinned official SDK and controlled browser. Test deployed-like TLS/WebSocket/ICE/TURN paths where feasible. Join, WebSocket or SDP success alone is not proof of media compatibility.
- **XI Logs:** collect Playwright traces, browser console/network errors, correlation IDs and RTP stats/equivalent media-flow evidence. Redact credentials and sensitive SDP/candidates; bound artifact retention/size.
- **XII Admin:** setup/cleanup scripts are explicit and scoped to resources created by the current run; never delete arbitrary rooms or participants.

## Required compatibility evidence
For each release gate record SDK/browser versions, commit/build, target environment, join result, publish/subscribe result, real media-flow evidence (RTP stats or equivalent), ICE restart/reconnect results, and failure diagnostics. Classify root cause only when evidence supports it. Use staged gates: join; real bidirectional media; reconnect/ICE restart; concurrency/churn; then advanced features such as simulcast.

## Acceptance checklist
- [ ] SDK, browser, Playwright and dependencies are pinned.
- [ ] Config is injected and secrets are redacted.
- [ ] Each run has isolated resources and bounded teardown.
- [ ] Reports identify versions and include useful diagnostics.
- [ ] Assertions prove real media flow, not only signaling success.
- [ ] Parallel runs cannot corrupt shared room/participant state.
- [ ] Cleanup is limited to the current test run's resources.
