# API Design: The "Dual-Listener" Fortress

The `anon_ticket_api` is the public face of the system. Its primary goal is to serve high-traffic redemption requests with minimal latency while keeping administrative functions completely inaccessible to the public internet.

To achieve this, we employ a **Dual-Listener Architecture**.

## 1. Architecture

The application bootstraps two distinct `actix-web` servers within the same process, sharing the same `AppState` (DB connection pool, Cache, Telemetry) but listening on different interfaces.

### 1.1. Public Listener
- **Purpose**: Serve end-users via Tor or public internet.
- **Exposure**: Typically binds to a Unix Socket (for Nginx/Tor proxying) or `0.0.0.0`.
- **Routes**:
    - `POST /api/v1/redeem`: Exchange a Payment ID for a Service Token.
    - `GET /api/v1/token/{token}`: Introspect token status/balance.
- **Security**: No privileged actions allowed. Heavy caching and rate-limiting ready.

### 1.2. Internal Listener
- **Purpose**: Serve monitoring agents (Prometheus) and admin scripts.
- **Exposure**: Binds to `127.0.0.1` or a private Unix Socket. **Never** exposed to the public.
- **Routes**:
    - `GET /metrics`: Prometheus scraping endpoint.
    - `POST /api/v1/token/{token}/revoke`: Administrative revocation of tokens.
- **Security**: Implicitly trusted by network topology. The revocation endpoint allows setting `abuse_score`, which is a privileged action.

## 2. State Management

We use a shared `AppState` wrapped in `Arc` to ensure data consistency across both listeners.

```rust
pub struct AppState {
    storage: SeaOrmStorage,        // Async DB Pool
    cache: Arc<InMemoryPidCache>,  // Shared Negative Cache
    telemetry: TelemetryGuard,     // Shared Metrics Registry
}
```

### Atomic Redemption
The `/redeem` handler relies on the Storage layer's `claim_payment` method, which uses `UPDATE ... RETURNING` to ensure that a payment can be claimed exactly once, even under high concurrency.

### Negative Caching
To prevent cache-penetration DoS attacks (where attackers flood the API with random PIDs to hammer the database), we maintain a short-lived `InMemoryPidCache`. It tracks PIDs that recently returned "Not Found" and blocks repeated lookups for a grace period (`500ms`).

## 3. Security Considerations

- **Revocation Isolation**: By hard-coding the revocation route into the `internal_server` builder, we eliminate the risk of misconfiguration exposing admin tools to the web.
- **Input Validation**: All handlers use strict types (`PaymentId`, `ServiceToken`) which enforce hex formatting and length constraints before business logic executes.
