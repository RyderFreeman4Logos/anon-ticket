# Anon-Ticket

Anon-Ticket is organized as a Cargo workspace so that the HTTP API, monitor
service, and domain primitives can evolve independently while sharing the same
lint/test configuration.

The repository is pinned to Rust 1.91.1 via `rust-toolchain.toml`, so installing
that toolchain plus `clippy`/`rustfmt` is sufficient to reproduce CI locally.

## Workspace Layout

| Path           | Crate Name           | Type | Responsibility |
| -------------- | -------------------- | ---- | -------------- |
| `crates/domain`  | `anon_ticket_domain`  | lib  | Core payment + token primitives shared by every binary. |
| `crates/api`     | `anon_ticket_api`     | bin  | Actix-based redemption and introspection HTTP surface. |
| `crates/monitor` | `anon_ticket_monitor` | bin  | Monero wallet monitor that imports qualifying transfers. |

### Domain Crate Internals

`anon_ticket_domain` is intentionally split into focused modules:

- `config`: environment hydration plus `ApiConfig`/`BootstrapConfig` loading contracts.
- `model`: strongly typed payment/service token IDs, record structs, and hashing helpers.
- `services::cache` / `services::telemetry`: PID cache abstractions, telemetry wiring, and abuse tracking utilities shared by binaries.
- `storage::traits`: async `PaymentStore`/`TokenStore`/`MonitorStateStore` definitions and shared error types.

Downstream crates can import only the module they need (for example `anon_ticket_domain::model::PaymentId`) while still benefiting from the crate-level re-exports for compatibility.

### API Crate Internals

- `application.rs`: loads config/telemetry, builds shared state, and wires Actix `HttpServer` instances (public + optional internal metrics listener).
- `state.rs`: centralizes the shared `AppState` (storage handle, PID cache, telemetry guard, abuse tracker) with accessor methods for handlers and tests.
- `handlers/`: `redeem.rs`, `token.rs`, and `metrics.rs` contain request/response DTOs plus the Actix handlers used by the routers.
- `tests.rs`: houses the Actix integration tests that exercise redemption, caching, and token revocation, keeping `main.rs` minimal.

### Monitor Crate Internals

- `rpc/`: JSON-RPC request/response types plus a `TransferSource` trait and its `RpcTransferSource` implementation so we can swap the backend during tests.
- `pipeline.rs`: ingestion logic that validates payment IDs, emits metrics, and persists qualifying transfers via the storage trait.
- `worker.rs`: the long-running loop that pulls batches from a `TransferSource`, advances the stored height cursor, and exposes the shared `MonitorError` type.
- `main.rs`: now limited to bootstrapping config/telemetry, wiring the SeaORM storage handle, and calling the worker with an RPC source.

### Storage Crate Internals

- `SeaOrmStorage` lives in `lib.rs` and exposes a `builder()` so future caching/sharding wrappers can intercept the underlying connection.
- `migration.rs`: contains table definitions and shared helpers to initialize the schema.
- `payment_store.rs`, `token_store.rs`, `monitor_state_store.rs`: implement each storage trait in isolation to keep the files focused.
- `builder.rs`: thin builder that accepts a database URL and applies migrations before constructing the storage handle.

## Developer Commands

Run the shared toolchain entry points from the workspace root:

```bash
cargo fmt --all
cargo clippy --workspace --all-features -- -D warnings
cargo test --all --all-features
```

Each crate has placeholder code wired through `anon_ticket_domain::workspace_ready_message`
so that the workspace builds end-to-end. Replace these stubs incrementally as the
`TODO.md` roadmap is executed.

## Environment Setup

1. Copy `.env.example` to `.env` and update the values per deployment target.
   These variables cover both binaries: `anon_ticket_api` requires
   `DATABASE_URL` / `API_BIND_ADDRESS` and optionally allows
   `API_UNIX_SOCKET` to override the TCP listener plus
   `API_INTERNAL_BIND_ADDRESS` / `API_INTERNAL_UNIX_SOCKET` for internal-only
   routes. `anon_ticket_monitor` enforces the full Monero RPC contract through
   `BootstrapConfig`. Optional telemetry knobs (`API_LOG_FILTER`,
   `API_METRICS_ADDRESS`, `API_ABUSE_THRESHOLD`, `MONITOR_LOG_FILTER`,
   `MONITOR_METRICS_ADDRESS`) fine-tune tracing verbosity and open Prometheus
   listeners without preventing startup if unset.
2. Store deployment-specific TOML/JSON secrets inside `config/` (see
   `config/README.md`). The folder is git-ignored to avoid committing secrets;
   document schemas or defaults instead of real credentials.
3. Run the shared commands listed above (`cargo fmt`, `cargo clippy`,
   `cargo test`) to validate changes.

## Storage Layer

The `anon_ticket_storage` crate implements the `PaymentStore`, `TokenStore`, and
`MonitorStateStore` traits from `anon_ticket_domain::storage` using SeaORM. It
defaults to SQLite (`features = ["sqlite"]`) so local development remains
dependency-free, while enabling PostgreSQL is as simple as rebuilding with:

```bash
cargo test -p anon_ticket_storage --no-default-features --features postgres
```
Set `TEST_POSTGRES_URL=postgres://user:pass@host/dbname` to point at a throwaway
database before running the Postgres command; tests will be skipped if the env
var is absent.

Both backends share the same schema: `payments` (PID primary key),
`service_tokens` (token primary key), and `monitor_state` (key/value for height
tracking). The storage adapter automatically runs migrations when connecting, so
crates can call `SeaOrmStorage::connect(<DATABASE_URL>)` and immediately receive
a handle that satisfies the domain traits.

## PID & Token Helpers

The `anon_ticket_domain` crate exposes `validate_pid` / `PaymentId::parse` to
enforce the security rule that every client-supplied PID is a 32-character hex
string. Use `derive_service_token(pid, txid)` to deterministically derive the
service token returned to clients—both helpers rely on SHA3-256 to match the
project's threat model.

## Redemption API

`anon_ticket_api` hosts an Actix-Web server with a single endpoint:

```
POST /api/v1/redeem
{
  "pid": "0123456789abcdef0123456789abcdef"
}
```

Responses:

- `200 OK` with `{ "status": "success", "service_token": "…", "balance": 123 }` when the
  payment exists and is unclaimed (a token record is inserted via the shared
  storage layer).
- `200 OK` with `{ "status": "already_claimed", ... }` when the payment was
  previously claimed; the API re-derives the deterministic token and returns it so
  clients can safely retry after transient failures.
- `400 Bad Request` if the PID is not a 32-char hex string.
- `404 Not Found` if the PID has never been observed.

The server uses `ApiConfig` to load `DATABASE_URL` / `API_BIND_ADDRESS` before
constructing `SeaOrmStorage`, so it stays decoupled from monitor-only
environment requirements. When `API_UNIX_SOCKET` is configured the HTTP server
binds to the provided Unix domain socket (cleaning up stale sockets) and
falls back to TCP otherwise. Optional observability env vars allow tuning the
log filter, metrics listener, and abuse-threshold used by the in-memory tracker
that logs suspicious PID probes.

### Internal API Listener

Set `API_INTERNAL_BIND_ADDRESS` or `API_INTERNAL_UNIX_SOCKET` to expose
internal-only routes (currently `/metrics`) on a dedicated TCP port or Unix
socket. This keeps operational/administrative endpoints away from Tor-exposed
listeners while still allowing the public API to operate over TCP or UDS. If
no internal listener is configured, `/metrics` remains available on the public
listener for backward compatibility.

### Token Introspection & Revocation

- `GET /api/v1/token/{token}` – returns the token status (`active`/`revoked`),
  amount, `issued_at`, optional `revoked_at`, and `abuse_score`.
- `POST /api/v1/token/{token}/revoke` – accepts `{ "reason": "...", "abuse_score": 5 }`
  to mark a service token as revoked; subsequent lookups report `revoked`.

### PID Cache

The API keeps an in-memory PID cache (`InMemoryPidCache`) that records PIDs
observed via storage lookups. Negative entries short-circuit repeated probes for
roughly 500ms without hitting the database, then the handler re-validates
against storage. Entries also expire after a short TTL (60s by default) so
legitimate clients can retry once the monitor catches up. The abstraction lives
in `anon_ticket_domain` so it can later be backed by Redis or a real Bloom
filter.

### Metrics & Abuse Detection

`anon_ticket_api` exposes Prometheus-compatible metrics at `GET /metrics`,
backed by the shared telemetry module. Set `API_METRICS_ADDRESS` if you prefer
the exporter to run on a dedicated port. The API increments counters for each
redeem/token request outcome and tags cache hints (`absent_blocked` / `absent_probe`)
so you can alert on spikes in invalid PID traffic even without a dedicated abuse
tracker.

## Monitor Service

`anon_ticket_monitor` polls `monero-wallet-rpc`'s `get_transfers` endpoint,
validates each PID via the domain helpers, and persists eligible payments using
`SeaOrmStorage`. Environment variables required:

- `MONERO_RPC_URL`
- `MONITOR_START_HEIGHT`

The monitor assumes your `monero-wallet-rpc` is exposed without HTTP auth (run
it with `--disable-rpc-login` or only behind a trusted proxy). If you need
Digest/Basic auth, place a proxy in front of the wallet that terminates those
schemes and forwards anonymous requests to the monitor.

Optional telemetry settings mirror the API (`MONITOR_LOG_FILTER`,
`MONITOR_METRICS_ADDRESS`). When `MONITOR_METRICS_ADDRESS` is provided, the
monitor automatically exposes Prometheus metrics (RPC successes/failures, batch
sizes, ingested payments) so ops can visualize sync progress.

The binary tracks the last processed height in the storage layer so it can
resume after restarts. Configure the RPC credentials to point at the wallet you
use for receiving PID-based transfers.

### Watch-Only Wallet Deployment (Recommended)

To keep spend keys inside a hardware wallet while still letting the monitor
tail incoming transfers, run `monero-wallet-rpc` against a watch-only wallet and
point `MONERO_RPC_URL` at that instance. The expected workflow for every
deployment is:

1. **Export address + view key from your hardware wallet.** Load the wallet in
   `monero-wallet-cli` (e.g. `monero-wallet-cli --hw-device ledger`) and run
   `address` plus `viewkey` to capture the primary address and private view key.
   Never export the spend key; the hardware device keeps it offline.
2. **Generate a watch-only wallet file.** Use the captured details to build a
   read-only wallet: `monero-wallet-cli --generate-from-view-key \
   --restore-height <height> watch-only --address <primary-address> \
   --view-key <private-view-key> --password ""`. This wallet cannot sign
   transactions but can decode every incoming output. Pick a restore height that
   matches your network (mainnet default or stagenet/testnet if applicable) to
   avoid a full rescan.
3. **Start `monero-wallet-rpc` in watch-only mode.** Point it at a trusted
   daemon (`--daemon-address <node>` or `--daemon-host 127.0.0.1 --daemon-port
   18081`) and load the watch-only file: `monero-wallet-rpc --wallet-file
   watch-only --password "" --daemon-address <daemon> --rpc-bind-port 18082 \
   --confirm-external-bind --disable-rpc-login`. The `--disable-rpc-login`
   flag ensures the monitor can talk to the RPC without HTTP authentication.
4. **Wire environment variables.** Set `MONERO_RPC_URL` (for example
   `http://127.0.0.1:18082/json_rpc`) and `MONITOR_START_HEIGHT` to the block
   height where you want ingestion to begin. All other `.env` entries stay the
   same regardless of whether you run mainnet, stagenet, or testnet.

Because the monitor only calls `get_transfers`/`get_height`, it never needs the
spend key. Using watch-only wallets should therefore be treated as the default
operational stance—doing so keeps hardware wallets offline, limits blast radius
if the RPC endpoint leaks, and still lets you validate payments in near real
time. The only extra maintenance cost is occasionally running `rescan_bc` on
the watch-only wallet whenever you rotate restore heights or bootstrap from a
new daemon.

## Observability

Both binaries share the domain-level telemetry module:

- `TelemetryConfig::from_env(<prefix>)` picks up log filters and optional
  metrics listeners without forcing additional env vars.
- `init_telemetry` installs a global `tracing-subscriber` configured via the
  supplied filter (default `info`).
- Prometheus metrics are collected via `metrics-exporter-prometheus`; if a
  `<PREFIX>_METRICS_ADDRESS` (e.g. `API_METRICS_ADDRESS=0.0.0.0:9898`) exists, a
  listener is spawned automatically, otherwise the API's `/metrics` endpoint can
  be scraped directly.
- `storage/`: `SeaOrmStorage` now re-exports submodules for migrations, per-trait implementations, and a `StorageBuilder` so future caching/sharding layers can wrap the database connection before it is shared.
