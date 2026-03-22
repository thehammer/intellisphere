# Dyson: Implementation Guide

## Claude Code Handoff Document

This document is the implementation companion to the Dyson Architecture Design Document. It provides sequenced build phases, pinned dependencies, concrete specifications for v1 implementations, error handling contracts, and the decisions Claude Code should NOT make without human review.

---

## 1. Decision Log — Pre-Resolved

These decisions are settled. Claude Code should not revisit them.

| Decision | Resolution | Rationale |
|---|---|---|
| Sphere language | Rust (2021 edition) | Memory safety, zero-cost abstractions, tower middleware |
| Core language | Go 1.22+ | Simple concurrency, thin proxy |
| Internal transport | gRPC with protobuf3 | Typed contracts, streaming, language-agnostic |
| External API | HTTP/2 (Sphere exposes REST+WebSocket) | Browser/client compatibility |
| Config format | YAML (serde_yaml) | Human-readable, widely understood |
| Async runtime | tokio (multi-thread) | Industry standard for async Rust |
| Satellite transport | WebSocket over TLS | Browser-native, bidirectional |
| v1 filter strategy | Heuristic/regex, no ML models | Minimize dependencies, ship fast, upgrade later |
| Tool authoring SDK | TypeScript | Developer ergonomics across org |
| Container orchestration | Docker Compose (dev), Kubernetes-ready (prod) | Progressive complexity |

---

## 2. Decisions Requiring Human Review

Claude Code should stub or flag these, not resolve them independently:

- **Secret provider for production**: Vault vs. AWS Secrets Manager vs. other. v1 uses env vars with a `SecretProvider` trait allowing swap-in later.
- **PII entity detection library**: v1 uses regex patterns. A future version may use `ort` (ONNX Runtime) or an external service. The `PIIRedactionFilter` interface must accommodate both.
- **Satellite WebSocket auth token lifetime and rotation policy**: v1 uses JWT with a configurable TTL. The specific TTL default (suggested: 60 min) should be reviewed.
- **Multi-tenancy model**: The design supports tenant isolation via policy, but whether Dyson instances are per-tenant or shared-with-isolation is a deployment decision.
- **WASM tool handler runtime**: Mentioned in the design doc as future. Do not implement in any phase. Leave `ToolZone::Wasm` as an enum variant that returns `Err(ToolError::NotImplemented)`.

---

## 3. Repository Structure

```
dyson/
├── README.md
├── LICENSE                          # Apache 2.0 or MIT — TBD by maintainer
├── docker-compose.yaml
├── docker-compose.dev.yaml          # dev overrides (hot reload, debug ports)
├── proto/
│   └── dyson/
│       └── v1/
│           └── dyson.proto          # single source of truth for gRPC contract
│
├── sphere/                          # Rust workspace
│   ├── Cargo.toml                   # workspace root
│   ├── Cargo.lock
│   ├── build.rs                     # tonic-build for proto compilation
│   ├── Dockerfile
│   ├── config/
│   │   ├── dyson.config.yaml        # default configuration
│   │   ├── dyson.config.dev.yaml    # dev overrides
│   │   └── policies/
│   │       └── default.yaml
│   └── src/
│       └── (module structure per design doc section 4)
│
├── core/                            # Go module
│   ├── go.mod
│   ├── go.sum
│   ├── Dockerfile
│   ├── cmd/core/main.go
│   ├── internal/
│   │   └── (module structure per design doc section 6)
│   └── proto/dyson/v1/              # generated Go protobuf
│
├── sdk/                             # TypeScript tool authoring SDK
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       ├── index.ts
│       ├── define-tool.ts
│       ├── define-edge-tool.ts
│       ├── types.ts
│       └── manifest.ts              # generates JSON manifest from TS definition
│
├── satellite/                       # TypeScript Satellite client
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       ├── index.ts
│       ├── satellite.ts
│       ├── guardrails.ts
│       ├── transport.ts
│       └── types.ts
│
├── cli/                             # Go CLI tool
│   ├── go.mod
│   ├── go.sum
│   └── cmd/dyson/
│       ├── main.go
│       ├── tool_create.go
│       ├── tool_validate.go
│       ├── tool_test.go
│       └── tool_register.go
│
├── examples/
│   ├── hello-world/                 # minimal: one tool, default config
│   ├── slack-integration/           # server-side tool with OAuth
│   └── browser-dom/                 # Satellite with DOM inspection
│
└── tests/
    ├── e2e/                         # full-stack integration tests
    └── fixtures/
```

---

## 4. Pinned Dependencies

### 4.1 Sphere (Rust) — Cargo.toml

```toml
[package]
name = "dyson-sphere"
version = "0.1.0"
edition = "2021"
rust-version = "1.77"

[dependencies]
# Async runtime
tokio = { version = "1.37", features = ["full"] }

# gRPC
tonic = { version = "0.11", features = ["tls"] }
prost = "0.12"

# HTTP server (external API + WebSocket)
axum = { version = "0.7", features = ["ws", "macros"] }
tower = { version = "0.4", features = ["full"] }
tower-http = { version = "0.5", features = ["cors", "trace", "timeout"] }
hyper = { version = "1.3", features = ["full"] }

# HTTP client (ScopedHttpClient for tool execution)
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"

# JSON Schema validation
jsonschema = "0.18"

# Protobuf timestamp/duration types
prost-types = "0.12"

# Regex (v1 filters)
regex = "1.10"

# URL parsing (ScopedHttpClient domain validation)
url = "2.5"

# Tracing / audit
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-opentelemetry = "0.24"
opentelemetry = { version = "0.23", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.16", features = ["tonic"] }

# Time
chrono = { version = "0.4", features = ["serde"] }

# UUID (request/session IDs)
uuid = { version = "1.8", features = ["v4"] }

# JWT validation
jsonwebtoken = "9.3"

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Config
figment = { version = "0.10", features = ["yaml", "env"] }

# Crypto (hashing, HMAC for Satellite session tokens)
sha2 = "0.10"
hmac = "0.12"

[build-dependencies]
tonic-build = "0.11"

[dev-dependencies]
tokio-test = "0.4"
wiremock = "0.6"           # mock HTTP for tool handler tests
tower-test = "0.4"         # tower::Service testing utilities
tempfile = "3.10"
assert_matches = "1.5"
```

### 4.2 Core (Go) — go.mod

```go
module github.com/dyson-framework/dyson/core

go 1.22

require (
    google.golang.org/grpc v1.63.2
    google.golang.org/protobuf v1.34.1
    github.com/sashabaranov/go-openai v1.24.0
    github.com/liushuangls/go-anthropic/v2 v2.3.0
    go.uber.org/zap v1.27.0
    github.com/caarlos0/env/v11 v11.0.1
)
```

### 4.3 SDK (TypeScript) — package.json (partial)

```json
{
  "name": "@dyson/sdk",
  "version": "0.1.0",
  "type": "module",
  "dependencies": {
    "ajv": "^8.13.0",
    "zod": "^3.23.0",
    "typescript": "^5.4.0"
  }
}
```

### 4.4 Satellite (TypeScript) — package.json (partial)

```json
{
  "name": "@dyson/satellite",
  "version": "0.1.0",
  "type": "module",
  "dependencies": {
    "typescript": "^5.4.0",
    "zod": "^3.23.0"
  }
}
```

### 4.5 Protobuf Toolchain

```
protoc: v26.1
protoc-gen-go: v1.34.1
protoc-gen-go-grpc: v1.3.0
tonic-build: 0.11 (Rust, via build.rs)
buf: v1.31.0 (optional, for linting/breaking change detection)
```

---

## 5. Phased Implementation Plan

### Phase 1 — Skeleton & Core Loop

**Goal**: A working end-to-end loop: external request → Sphere → Core → LLM API → response back through Sphere. One builtin filter. One hello-world tool. No Satellite.

**Duration estimate**: 1–2 focused sessions.

**Tasks (in order):**

```
Phase 1.1 — Protobuf & project scaffolding
├── Create repo structure per section 3
├── Write dyson.proto (per design doc section 5)
├── Scaffold Rust workspace with Cargo.toml (deps per section 4.1)
├── Scaffold Go module with go.mod (deps per section 4.2)
├── build.rs: tonic-build compiles proto
├── Go: protoc generates dyson.pb.go and dyson_grpc.pb.go
├── Verify: both sides compile against shared proto
└── Commit: "chore: project scaffolding and proto contract"

Phase 1.2 — LLM Core (Go)
├── Implement LLMAdapter interface
├── Implement AnthropicAdapter
│   ├── Complete() — unary completion
│   ├── CompleteStream() — streaming via server-side gRPC stream
│   └── Health() — validate API key works
├── Implement gRPC server (dyson.LLMCore service)
├── Implement config loading (env vars: LLM_PROVIDER, ANTHROPIC_API_KEY)
├── Dockerfile: minimal Go binary, read-only fs, cap-drop ALL
├── Test: send a hardcoded message, get a completion back via grpcurl
└── Commit: "feat(core): LLM Core with Anthropic adapter"

Phase 1.3 — Sphere skeleton
├── main.rs: tokio runtime, config loading, axum server
├── Config loader: parse dyson.config.yaml via figment
├── core_client/grpc.rs: tonic client connecting to Core on dyson-internal
├── Ingress gate (stub): accept all requests, no auth (Phase 2)
├── Pipeline chain (minimal): execute filters in order, pass-through if empty
├── Single builtin filter: InputSanitizationFilter
│   ├── Strip control characters (U+0000–U+001F except \n \r \t)
│   ├── Normalize unicode (NFC normalization)
│   ├── Reject null bytes
│   └── Configurable via YAML
├── Tool interceptor (stub): if Core returns tool_calls, log and return error
│   result "no tools registered" — correct behavior for Phase 1
├── Outbound pipeline (stub): pass-through
├── External API:
│   ├── POST /v1/chat — accepts messages, runs through pipeline, calls Core
│   ├── POST /v1/chat/stream — SSE streaming variant
│   └── GET /health — aggregates Sphere + Core health
├── Dockerfile: Rust release binary
├── docker-compose.yaml: sphere + core on dyson-internal network
├── Test: curl POST /v1/chat with a message, get LLM response back
└── Commit: "feat(sphere): skeleton with input sanitization filter"

Phase 1.4 — Tool registry & hello-world tool
├── Tool registry: in-memory HashMap<String, ToolRegistration>
├── Tool manifest loader: deserialize JSON manifests from config dir
├── Tool interceptor: match Core tool_calls against registry
│   ├── Validate params against JSON schema
│   ├── Check required_scopes (stub: all scopes granted in Phase 1)
│   ├── Execute handler
│   └── Return result to Core as tool_result message
├── ScopedHttpClient: basic impl with domain allowlist + auth header injection
├── Hello-world tool (built-in for testing):
│   ├── name: "dyson_echo"
│   ├── params: { message: string }
│   ├── handler: returns { echo: message, timestamp: now }
│   ├── No external HTTP call — pure function
│   └── Purpose: validate the full tool call loop
├── System prompt injection: Sphere prepends tool definitions to the
│   system message sent to Core, so the LLM knows which tools are available
├── Multi-turn tool loop: Sphere handles the Core→tool→Core→response cycle
│   ├── Core returns stop_reason=TOOL_USE with tool_calls
│   ├── Sphere executes tools, collects results
│   ├── Sphere sends new CompletionRequest with tool_result messages appended
│   ├── Repeat until Core returns stop_reason=END_TURN
│   └── Max iterations: configurable (default: 10), hard cap: 25
├── Test: send a message that triggers dyson_echo, verify full loop
└── Commit: "feat(sphere): tool registry and execution loop"
```

**Phase 1 exit criteria:**
- `docker compose up` starts Sphere + Core
- `curl -X POST http://localhost:8080/v1/chat -d '{"messages":[...]}' ` returns an LLM response
- Input sanitization filter is active and testable
- Tool call loop works end-to-end with dyson_echo
- All control characters stripped, unicode normalized

---

### Phase 2 — Security Hardening

**Goal**: Full inbound/outbound filter chain, auth, rate limiting, policy engine, audit trail. The Sphere becomes a real security boundary.

**Tasks (in order):**

```
Phase 2.1 — Authentication & identity
├── AuthStrategy trait implementation
├── JwtAuthStrategy: validate RS256 JWTs via JWKS endpoint
│   ├── Use jsonwebtoken crate
│   ├── JWKS caching with TTL (default: 1 hour)
│   └── Extract sub, roles, scopes into Identity
├── ApiKeyAuthStrategy: static API key validation (dev/testing)
├── Identity propagation: attach to PipelineContext for all downstream use
├── Config: auth strategies declared in YAML
├── Test: reject requests without valid JWT, accept with valid JWT
└── Commit: "feat(sphere): authentication and identity propagation"

Phase 2.2 — Rate limiting
├── Token bucket rate limiter (in-memory, per-identity)
├── Global rate limiter
├── Per-tool rate limiter
├── Rate limit headers in response (X-RateLimit-Remaining, etc.)
├── Config: limits declared in YAML per section 10 of design doc
└── Commit: "feat(sphere): rate limiting"

Phase 2.3 — Inbound filters (all v1 implementations)
├── ContentClassifierFilter (regex/heuristic v1)
│   ├── Pattern list (configurable):
│   │   ├── "ignore (all |any )?(previous|prior|above) (instructions|prompts|rules)"
│   │   ├── "you are now (a |in )?"
│   │   ├── "disregard (all |any )?(previous|prior|earlier)"
│   │   ├── "new (instructions|rules|prompt|persona):"
│   │   ├── "system:\s" (fake system message injection)
│   │   ├── "pretend (you are|to be|that)"
│   │   ├── "\bDAN\b" (Do Anything Now)
│   │   ├── "jailbreak"
│   │   ├── "bypass (your |all )?(rules|restrictions|filters|safety)"
│   │   └── Custom patterns from config
│   ├── Modes: block (reject request), flag (annotate context, continue),
│   │   log (emit audit event, continue)
│   ├── Sensitivity levels: low (exact match), medium (case-insensitive),
│   │   high (fuzzy/substring)
│   └── NOTE: v1 is regex only. Interface allows future swap to ML classifier.
│       The filter receives the full message and returns Result<LLMMessage, FilterError>.
│       A future impl can call an external classification API internally.
│
├── PIIRedactionFilter (regex v1)
│   ├── Entity patterns:
│   │   ├── EMAIL: standard RFC 5322 simplified regex
│   │   ├── PHONE: US format + international prefix patterns
│   │   ├── SSN: \b\d{3}-\d{2}-\d{4}\b
│   │   ├── CREDIT_CARD: Luhn-valid 13-19 digit sequences with optional separators
│   │   └── Custom patterns from config
│   ├── Strategies:
│   │   ├── mask: "john@email.com" → "[EMAIL_REDACTED]"
│   │   ├── replace: "john@email.com" → "user@example.com"
│   │   └── remove: strip entirely
│   └── Annotation: adds detected entity count to PipelineContext
│       for downstream audit
│
├── TopicGuardrailFilter (keyword v1)
│   ├── Allowed topics list (if set, reject anything not matching)
│   ├── Blocked topics list (reject anything matching)
│   ├── Matching: keyword presence + configurable threshold
│   └── NOTE: v1 is keyword-based. Same interface swap path as content classifier.
│
├── TokenBudgetFilter
│   ├── Token counting: use tiktoken-rs crate for accurate counting
│   │   ├── Add dependency: tiktoken-rs = "0.5"
│   │   └── Model-aware tokenizer selection
│   ├── Per-request input token limit
│   ├── Per-session cumulative token limit
│   └── Session token tracking in PipelineContext
│
├── ConversationBoundaryFilter
│   ├── Max turns per session
│   ├── Max context window tokens (triggers summarization hint or rejection)
│   └── Session reset policy (hard reject vs. warn)
│
├── Test: each filter independently with unit tests
├── Test: full chain with integration test (multiple filters, verify order)
└── Commit: "feat(sphere): complete inbound filter chain"

Phase 2.4 — Tool security hardening
├── Param sanitization: run ParamSanitizers before execution
├── Schema validation: strict JSON Schema validation with jsonschema crate
│   ├── additionalProperties: false enforced by default
│   ├── Reject unknown fields
│   └── Type coercion disabled
├── Authorization: check identity.scopes against tool.required_scopes
├── Authorization: check identity.roles against tool.required_roles
├── Tool policies: evaluate ToolPolicy list before execution
├── Timeout enforcement: tokio::time::timeout wrapping handler execution
├── Result validation: validate handler return against result_schema if present
├── Result sanitization: run ResultSanitizers on handler output
│   ├── Builtin: StripInjectionPatterns — same regex set as ContentClassifierFilter
│   │   applied to string fields in tool results
│   └── Builtin: TruncateResult — enforce max result size
├── ScopedHttpClient hardening:
│   ├── Domain validation (already in Phase 1)
│   ├── Request body size limit
│   ├── Response body size limit (streaming read with cap)
│   ├── TLS-only enforcement (reject http:// URLs)
│   ├── Redirect following: disabled by default (configurable)
│   ├── Timeout per request
│   └── Full audit logging of all HTTP calls
├── Error containment: tool handler panics caught, converted to error results
│   (use std::panic::catch_unwind for sync, tokio::task::JoinHandle for async)
└── Commit: "feat(sphere): tool security hardening"

Phase 2.5 — Outbound filters
├── PIILeakDetectionFilter
│   ├── Same entity patterns as inbound PIIRedactionFilter
│   ├── Actions: redact (replace in output), block (reject entire response), flag
│   └── Detects PII the LLM may have generated (not just echoed)
│
├── InjectionEchoFilter
│   ├── Compare outbound response against flagged injection patterns from inbound
│   ├── If the LLM's response contains phrases that were flagged as injection
│   │   attempts in the input, this indicates the LLM is executing injected instructions
│   ├── Uses annotations from ContentClassifierFilter (flag mode) stored in
│   │   PipelineContext
│   ├── Actions: block, flag, log
│   └── NOTE: This is why ContentClassifierFilter has a "flag" mode — it marks
│       patterns that InjectionEchoFilter checks for on the way out
│
├── ResultSizeEnforcementFilter
│   ├── Max characters in response
│   ├── Max tokens in response
│   └── Truncation strategy: hard cut vs. ask LLM to summarize
│
├── ResponseClassifierFilter (regex v1)
│   ├── Detect harmful/off-topic output patterns
│   ├── Configurable pattern lists
│   └── Actions: block, redact, flag
│
├── HallucinationFlagFilter (v1: simple heuristic)
│   ├── If response references tool results, check that the referenced
│   │   data exists in the conversation context
│   ├── v1 approach: flag responses that contain specific data patterns
│   │   (URLs, numbers, dates) not present in any tool result or user message
│   └── Low confidence — flag only, never block in v1
│
└── Commit: "feat(sphere): outbound filter chain"

Phase 2.6 — Policy engine
├── Policy loader: deserialize YAML policy files
├── Policy evaluation:
│   ├── Session-scoped policies (max tool calls per session)
│   ├── Identity-scoped policies (token budgets per identity per hour)
│   ├── Tool-scoped policies (require MFA, require roles, max results)
│   └── Tenant isolation policies (enforce param matches identity.metadata)
├── Policy actions: terminate_session, throttle, block, flag, log
├── Policy evaluation happens at:
│   ├── Ingress Gate (identity-scoped)
│   ├── Tool Interceptor (tool-scoped)
│   └── Session manager (session-scoped, checked after each turn)
└── Commit: "feat(sphere): declarative policy engine"

Phase 2.7 — Audit trail
├── AuditEvent struct (per design doc section 12)
├── AuditEmitter: async event emission via tracing crate
│   ├── Structured JSON format
│   ├── Correlation: session_id + request_id on every event
│   └── Sensitive tool calls: params/results redacted automatically
│       (enforced by RedactedParams type — cannot be logged as raw)
├── AuditSink trait + implementations:
│   ├── StdoutSink (tracing-subscriber JSON layer)
│   ├── FileSink (rotating file output)
│   └── OtlpSink (OpenTelemetry export via tracing-opentelemetry)
├── Audit events emitted at every phase transition:
│   ├── request_received, filter_applied, filter_blocked
│   ├── completion_requested, completion_received
│   ├── tool_call_intercepted, tool_call_authorized, tool_call_denied
│   ├── tool_call_executed, tool_call_timeout
│   ├── response_filtered, response_sent
│   └── policy_evaluated, policy_violated
└── Commit: "feat(sphere): structured audit trail"
```

**Phase 2 exit criteria:**
- Unauthenticated requests are rejected
- All 6 inbound filters operational and configurable
- All 5 outbound filters operational and configurable
- Tool calls require valid scopes/roles
- ScopedHttpClient blocks non-allowlisted domains
- Policy engine evaluates YAML-declared policies
- Audit trail captures every phase transition with structured events
- Full integration test: prompt injection attempt → detected by content classifier → flagged → LLM response checked by injection echo filter → flagged in audit

---

### Phase 3 — Satellite (Edge Execution)

**Goal**: Browser-based tools via Keri pattern. Satellite proposes, Sphere adjudicates.

**Tasks (in order):**

```
Phase 3.1 — Satellite WebSocket endpoint
├── Sphere: axum WebSocket upgrade handler at /satellite
├── Authentication flow:
│   ├── Client requests session token: POST /v1/satellite/session
│   │   ├── Requires valid JWT (same auth as chat API)
│   │   ├── Returns: { session_token, expires_at, trust_budget }
│   │   ├── Session token: HMAC-SHA256(session_id + identity.sub + expires_at, server_secret)
│   │   └── Trust budget initialized per config defaults
│   ├── Client opens WebSocket: GET /satellite?token={session_token}
│   │   ├── Sphere validates token signature and expiry
│   │   ├── Binds WebSocket to session and identity
│   │   └── Rejects if token invalid, expired, or already connected
│   └── Heartbeat: Sphere sends ping every 30s, expects pong within 10s
│       ├── Missed pong: session terminated
│       └── Satellite can also send ping (browser WebSocket API support)
├── Session lifecycle:
│   ├── Created: POST /v1/satellite/session
│   ├── Connected: WebSocket upgrade succeeds
│   ├── Active: tool proposals flowing
│   ├── Suspended: trust budget exhausted (can reconnect with new budget if policy allows)
│   └── Terminated: TTL expired, suspicion threshold, explicit disconnect
└── Commit: "feat(sphere): Satellite WebSocket endpoint and session management"

Phase 3.2 — Trust budget & adjudicator
├── TrustBudget struct (per design doc section 7.4)
│   ├── Server-side tracking only — Satellite receives read-only snapshots
│   ├── Deduction on every proposal (pre-execution)
│   └── Suspicion score incremented on:
│       ├── Proposal for unregistered tool (+0.3)
│       ├── Oversized result (+0.2)
│       ├── Rate limit exceeded (+0.1)
│       ├── Adjudication rule failure (+0.2)
│       └── Injection detected in result (+0.5)
├── Adjudicator:
│   ├── Receives ToolProposalResponse from Satellite
│   ├── Step 1: Deduct trust budget (reject if exhausted)
│   ├── Step 2: Validate result size against edgeConstraints.maxResultBytes
│   ├── Step 3: Run adjudication rules defined on the edge tool registration
│   ├── Step 4: Run result through inbound pipeline (full filter chain)
│   │   ├── This is critical — Satellite results are untrusted input
│   │   ├── Content classifier checks for injection in DOM content
│   │   ├── PII redaction strips sensitive data from DOM snapshots
│   │   └── Same filters, same config as user messages
│   ├── Step 5: Accept, redact, or reject
│   └── Emit audit events at each step
├── Send AdjudicationResult back to Satellite (for UI feedback)
└── Commit: "feat(sphere): trust budget and Satellite adjudicator"

Phase 3.3 — Satellite TypeScript client
├── @dyson/satellite package
├── DysonSatellite class:
│   ├── Constructor: sphereUrl, tools, guardrails config
│   ├── connect(): fetch session token, open WebSocket
│   ├── Receive ToolProposalRequest from Sphere
│   ├── Local guardrail pre-validation
│   ├── Execute edge tool handler
│   ├── Local guardrail result sanitization
│   ├── Send ToolProposalResponse
│   ├── Receive AdjudicationResult
│   ├── Reconnection logic: exponential backoff, max 5 retries
│   └── Events: onConnect, onDisconnect, onTrustBudgetUpdate, onError
├── SatelliteGuardrails class (per design doc section 7.6):
│   ├── Pre-validation: tool allowlist, rate limit
│   ├── Result sanitization: strip redacted attributes, enforce size, scrub selectors
│   └── Advisory only — Sphere validates server-side regardless
└── Commit: "feat(satellite): TypeScript client library"

Phase 3.4 — Edge tool SDK & example
├── @dyson/satellite-sdk package
├── defineEdgeTool() function (per design doc section 7.7)
├── Edge tool manifest generation (JSON, consumed by Sphere)
├── Example: dom_inspect tool
│   ├── Query elements by CSS selector
│   ├── Return specified properties
│   ├── Respect excludeSelectors and redactAttributes
│   ├── Enforce maxElements and maxDomDepth
│   └── Adjudication rules: no-script-content, result-depth-limit
├── Example: console_read tool
│   ├── Return last N console messages
│   ├── Filter by level (log, warn, error)
│   └── Truncate individual messages to max length
├── Integration test: browser → Satellite → Sphere → Core → tool_call →
│   Satellite executes DOM read → adjudicator validates → Core receives result
└── Commit: "feat(satellite-sdk): edge tool SDK and examples"

Phase 3.5 — Sphere routing: server vs. Satellite tools
├── Tool interceptor update: check tool.zone
│   ├── ToolZone::Sphere → execute locally via handler (existing path)
│   ├── ToolZone::Satellite → route to connected Satellite via WebSocket
│   │   ├── Find active Satellite session for this identity
│   │   ├── If no Satellite connected: return error result to Core
│   │   │   "Tool requires browser connection. No Satellite session active."
│   │   ├── If Satellite connected: send ToolProposalRequest, await response
│   │   ├── Timeout: edgeConstraints.timeoutMs (default 5000ms)
│   │   └── On timeout: return error result, increment suspicion
│   └── ToolZone::Wasm → return Err(ToolError::NotImplemented)
└── Commit: "feat(sphere): unified tool routing across zones"
```

**Phase 3 exit criteria:**
- Satellite connects via WebSocket with session token auth
- Trust budget tracked server-side, decremented on each proposal
- Suspicion score terminates sessions on threshold breach
- Edge tool results pass through full inbound filter chain
- dom_inspect example works end-to-end
- Audit trail captures complete Satellite interaction flow

---

### Phase 4 — SDK, CLI & Developer Experience

```
Phase 4.1 — TypeScript tool SDK (@dyson/sdk)
├── defineTool() and defineEdgeTool() functions
├── Manifest generation: TS definition → JSON manifest
├── Type generation: JSON Schema → TypeScript types
├── Validation: schema validation at definition time
└── Commit: "feat(sdk): TypeScript tool authoring SDK"

Phase 4.2 — CLI (Go)
├── dyson tool create <name> [--zone satellite]
│   └── Scaffolds directory with tool definition template
├── dyson tool validate <path>
│   └── Validates manifest against Dyson requirements
├── dyson tool test <path> --params '...' --identity '...'
│   └── Runs tool handler locally with mock context
├── dyson tool register <path> --target <sphere-admin-url>
│   └── Uploads manifest to running Sphere via admin API
├── dyson health --target <sphere-url>
│   └── Reports Sphere + Core + Satellite health
└── Commit: "feat(cli): Dyson CLI tool"

Phase 4.3 — Examples & documentation
├── examples/hello-world: minimal setup walkthrough
├── examples/slack-integration: real external API tool
├── examples/browser-dom: Satellite with DOM tools
├── README.md: quickstart, architecture overview, links
└── Commit: "docs: examples and quickstart guide"
```

---

## 6. Error Handling Contracts

Every component has a defined error behavior. Claude Code should implement these exactly.

### 6.1 Core Errors

| Scenario | Behavior |
|---|---|
| LLM API returns HTTP 429 (rate limit) | Core returns gRPC RESOURCE_EXHAUSTED with retry-after in metadata |
| LLM API returns HTTP 500+ | Core retries once after 1s, then returns gRPC UNAVAILABLE |
| LLM API returns HTTP 401 | Core returns gRPC UNAUTHENTICATED (API key invalid) |
| LLM API timeout (30s default) | Core returns gRPC DEADLINE_EXCEEDED |
| Invalid request from Sphere | Core returns gRPC INVALID_ARGUMENT with details |
| Provider adapter not found | Core returns gRPC UNIMPLEMENTED |
| Stream interrupted mid-completion | Core sends CompletionFinished with partial usage, stop_reason MAX_TOKENS |

### 6.2 Sphere Errors

| Scenario | Behavior | HTTP Status |
|---|---|---|
| Auth failure | Return error, no audit (avoid DoS on audit system) | 401 |
| Rate limit exceeded | Return error with Retry-After header | 429 |
| Inbound filter rejects (block mode) | Return error with filter name, emit audit event | 400 |
| Inbound filter rejects (flag mode) | Continue, annotate context, emit audit event | — |
| Core unreachable | Retry once after 500ms, then return error | 503 |
| Core returns error | Map gRPC status to HTTP status, return to client | varies |
| Tool not in registry | Return error result to Core (not to external client) | — |
| Tool param validation fails | Return error result to Core with validation details | — |
| Tool authorization fails | Return error result to Core, emit audit event | — |
| Tool handler panics | Catch panic, return error result to Core, emit audit | — |
| Tool handler timeout | Abort via signal, return timeout error to Core | — |
| Tool handler HTTP error | Return error result to Core (handler's HTTP call failed) | — |
| ScopedHttpClient domain blocked | Return error to handler, emit audit event | — |
| Outbound filter rejects | Return error to external client, emit audit event | 422 |
| Policy violation | Action per policy (terminate, throttle, block, flag) | varies |
| Max tool loop iterations reached | Return last LLM response with warning annotation | 200 |

### 6.3 Satellite Errors

| Scenario | Behavior |
|---|---|
| Invalid session token on connect | Reject WebSocket upgrade with 401 |
| Expired session token | Reject WebSocket upgrade with 401 |
| Trust budget exhausted | Send AdjudicationResult(accepted=false, reason="budget_exhausted"), close connection |
| Suspicion threshold exceeded | Send AdjudicationResult(accepted=false, reason="session_terminated"), close connection |
| Satellite disconnects mid-proposal | Return timeout error result to Core, clean up session |
| Proposal timeout (no response within deadline) | Return timeout error result to Core, increment suspicion +0.1 |
| Adjudication rule fails | Return error result to Core, increment suspicion per rule weight |
| Oversized result | Reject before running adjudication rules, increment suspicion +0.2 |
| Heartbeat missed | Terminate session, clean up |

### 6.4 Error Result Format to Core

When a tool execution fails for any reason, the Sphere sends a tool result back to the Core with `is_error: true`. The error message is structured but does NOT leak internal details:

```json
{
  "call_id": "tc_123",
  "is_error": true,
  "result_json": "{\"error\":\"tool_execution_failed\",\"code\":\"AUTHORIZATION_DENIED\",\"message\":\"Insufficient permissions to execute this tool\"}"
}
```

Error codes returned to the LLM:
- `TOOL_NOT_FOUND` — tool name not in registry
- `VALIDATION_FAILED` — params don't match schema
- `AUTHORIZATION_DENIED` — identity lacks required scopes/roles
- `POLICY_VIOLATION` — policy engine rejected
- `EXECUTION_TIMEOUT` — handler exceeded timeout
- `EXECUTION_ERROR` — handler returned an error
- `SATELLITE_UNAVAILABLE` — no active Satellite session for edge tool
- `SATELLITE_TIMEOUT` — Satellite didn't respond in time
- `ADJUDICATION_REJECTED` — Satellite result failed adjudication
- `BUDGET_EXHAUSTED` — trust budget depleted

These codes are visible to the LLM (so it can reason about failures) but do NOT contain stack traces, internal IPs, secret names, or implementation details.

---

## 7. Testing Strategy

### Unit Tests

Every filter, sanitizer, and policy rule gets unit tests with:
- Happy path (valid input passes through)
- Rejection path (invalid input is caught)
- Edge cases (empty strings, unicode, very large inputs, nested structures)

### Integration Tests

End-to-end tests using `docker compose -f docker-compose.test.yaml`:

```
test_full_loop          — message in, completion out
test_tool_loop          — message triggers tool, tool executes, completion completes
test_injection_blocked  — known injection pattern is blocked by content classifier
test_injection_flagged  — flagged injection is caught by outbound echo filter
test_pii_redacted       — PII in input is masked before reaching Core
test_pii_leak_caught    — PII in LLM output is caught by outbound filter
test_auth_rejected      — request without valid JWT is rejected
test_rate_limited       — excessive requests get 429
test_tool_authz_denied  — tool call without required scope is denied
test_tool_timeout       — slow tool handler is aborted
test_scoped_client      — tool HTTP call to non-allowlisted domain is blocked
test_policy_violation   — policy engine terminates session on budget exceeded
test_satellite_loop     — Satellite connects, receives proposal, returns result
test_satellite_budget   — trust budget depletes, session terminated
test_satellite_injection— injection in DOM content caught by adjudicator
```

### Mock LLM for Testing

The Core supports an `MOCK` provider that returns deterministic responses:

```go
// core/internal/adapter/mock.go

type MockAdapter struct {
    responses []MockResponse  // loaded from fixture file
    callIndex int
}

// Returns responses in sequence. Useful for testing multi-turn tool loops.
func (m *MockAdapter) Complete(ctx context.Context, req *pb.CompletionRequest) (*pb.CompletionResponse, error) {
    if m.callIndex >= len(m.responses) {
        return &pb.CompletionResponse{
            Content: proto.String("Mock: no more responses"),
            StopReason: pb.StopReason_STOP_END_TURN,
        }, nil
    }
    resp := m.responses[m.callIndex]
    m.callIndex++
    return resp.ToProto(), nil
}
```

---

## 8. Non-Functional Requirements

| Requirement | Target | Measured How |
|---|---|---|
| Filter chain latency (all 6 inbound) | < 5ms p99 | Benchmark via criterion |
| Tool call overhead (registry + validation + authz) | < 2ms p99 | Benchmark |
| Satellite round-trip (proposal → adjudication) | < 100ms p99 (excl. network) | Integration test timing |
| Memory per session | < 1MB baseline | tokio-console profiling |
| Core container image size | < 20MB | Docker image size |
| Sphere container image size | < 30MB | Docker image size |
| Config reload | Hot reload without restart (SIGHUP) | Manual test |
| Startup time (Sphere) | < 2s to healthy | Health check timing |

---

## 9. Files Claude Code Should Generate First

In dependency order. Each file should be complete and compilable before moving to the next.

```
1.  proto/dyson/v1/dyson.proto
2.  core/go.mod
3.  core/cmd/core/main.go
4.  core/internal/adapter/adapter.go
5.  core/internal/adapter/anthropic.go
6.  core/internal/server/grpc.go
7.  core/Dockerfile
8.  sphere/Cargo.toml
9.  sphere/build.rs
10. sphere/src/main.rs
11. sphere/src/config/mod.rs + loader.rs
12. sphere/src/core_client/grpc.rs
13. sphere/src/pipeline/context.rs
14. sphere/src/pipeline/chain.rs
15. sphere/src/pipeline/inbound/traits.rs
16. sphere/src/pipeline/inbound/input_sanitization.rs
17. sphere/src/tools/registry.rs
18. sphere/src/tools/interceptor.rs
19. sphere/src/tools/executor.rs
20. sphere/src/tools/scoped_client.rs
21. sphere/src/ingress/gate.rs
22. sphere/src/errors.rs
23. sphere/Dockerfile
24. docker-compose.yaml
25. sphere/config/dyson.config.yaml
```

After these 25 files exist and compile, Phase 1 is structurally complete.
