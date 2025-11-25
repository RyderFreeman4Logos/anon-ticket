# Building a Defensively Designed Kernel: Inside `anon_ticket_domain`

In the `anon-ticket` architecture, **`anon_ticket_domain`** is not just a bag of shared code; it is the **sovereign kernel** that enforces the system's invariants. By centralizing validation, security primitives, and core interfaces here, we ensure that every component—from the public API to the background monitor—speaks the exact same language and adheres to the same security constraints.

This document explores the design philosophy behind the domain layer, highlighting how defensive programming and type safety create a robust foundation for the entire system.

## 1. Type-Driven Security

We reject "string typing." In privacy-critical systems, passing around raw `String` or `&str` is a liability. The domain crate uses the **Newtype Pattern** to turn validation rules into type constraints.

### The `PaymentId` Primitive
A Payment ID (PID) isn't just a string; it's an encrypted 8-byte (16 hex character) identifier with high entropy, optimized for on-chain privacy and compact storage.
- **Parsing as Validation**: You cannot create a `PaymentId` without passing through `validate_pid`. This means if a function accepts a `PaymentId`, it is **mathematically impossible** for it to receive malformed data.
- **Canonicalization**: PIDs are case-insensitive but stored canonically (lowercase). The type handles this normalization internally, so downstream logic never has to worry about `AbCd` vs `abcd` mismatches.

```rust
// impossible to create an invalid PID instance
let pid = PaymentId::parse("invalid").expect_err("fails validation");

// once created, it's guaranteed correct
fn claim(pid: PaymentId) { ... }
```

## 2. Determinism & Entropy

The system is designed to be **stateless** where possible. We avoid generating random tokens that need to be stored alongside their secrets. Instead, we derive them.

### SHA3-256 Derived Tokens
The `ServiceToken` is deterministically derived from the `PaymentId` and the Monero transaction ID (`txid`) using **SHA3-256**.
- **Idempotency**: Because the token is a pure function of the payment inputs, the system is naturally idempotent. If a user claims the same payment twice, we re-derive the same token.
- **Recovery**: We don't need to "backup" tokens. As long as we have the payment record (PID + TXID), we can always recover the service token.
- **Auditability**: The derivation logic is centralized in `model/mod.rs`, ensuring that the API and any future auditing tools produce identical results.

## 3. Intelligent Defense: Bloom-First, Positive-Only Hints

With users arriving via Tor (no IP-based throttling), the front door must reject random PIDs cheaply and without letting attackers pollute state. The current design replaces negative caching with a **Bloom-only, positive cache** posture:
- **Zero False Negatives**: A Bloom filter (sized via `API_PID_BLOOM_ENTRIES`/`API_PID_BLOOM_FP_RATE`) sits ahead of storage. A Bloom negative returns 404 immediately—no cache writes, no DB touch.
- **Positive-Only Cache**: `moka` stores known-good PIDs (prewarmed from storage/monitor and updated on confirmed payments). Unknown probes never enter the cache or Bloom, preventing attacker-supplied PIDs from consuming memory.
- **DoS Degradation Path**: If the Bloom false-positive rate drifts upward (from attackers funding many unique PIDs), the system gracefully degrades to more DB lookups; correctness is preserved because we never allow false negatives.
- **Operational Hooks**: Sizing and telemetry live in the API bootstrap (`estimate_bloom_bytes`, `api_redeem_bloom_db_miss_total`), and the embedded monitor updates Bloom/cache as soon as payments are ingested.

## 4. Unified Observability

In a distributed system (even a modular monolith), consistent telemetry is critical. If the Monitor logs in JSON and the API logs in plain text, debugging is a nightmare.

The `services::telemetry` module provides a **"One-Line" initialization**:
- **Standardization**: It configures `tracing-subscriber` and `metrics-exporter-prometheus` with identical formats and labeled metrics across all binaries.
- **Env-Driven**: Log levels (`LOG_FILTER`) and metrics endpoints (`METRICS_ADDRESS`) are configured via environment variables, parsed uniformly by `TelemetryConfig`.

This ensures that adding a new binary to the workspace automatically inherits the production-grade observability stack.

## 5. Storage Abstraction

We use `async_trait` to define strict contracts for persistence:
- `PaymentStore`: Handles the lifecycle of Monero transfers.
- `TokenStore`: Manages the issuance and revocation of service credentials.
- `MonitorStateStore`: Tracks the blockchain synchronization cursor.

By decoupling the *interface* from the *implementation* (SeaORM), we gain:
- **Testability**: We can easily inject mock stores for unit testing domain logic.
- **Flexibility**: Switching databases (e.g., from SQLite to Postgres) or adding a caching layer (Decorator pattern) requires zero changes to the business logic consuming these traits.

---

*The `anon_ticket_domain` crate is more than a library; it is the blueprint of the system's integrity.*
