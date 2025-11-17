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
   These variables mirror the requirements enforced by `BootstrapConfig` in the
   domain crate, so binaries will refuse to start if any field is missing or
   malformed.
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
- `400 Bad Request` if the PID is not a 32-char hex string.
- `404 Not Found` if the PID has never been observed.
- `409 Conflict` if the PID was already claimed.

The server uses `BootstrapConfig` to load `DATABASE_URL` / `API_BIND_ADDRESS`
before constructing `SeaOrmStorage`, so it inherits the same environment
variables documented earlier.

### Token Introspection & Revocation

- `GET /api/v1/token/{token}` – returns the token status (`active`/`revoked`),
  amount, `issued_at`, optional `revoked_at`, and `abuse_score`.
- `POST /api/v1/token/{token}/revoke` – accepts `{ "reason": "...", "abuse_score": 5 }`
  to mark a service token as revoked; subsequent lookups report `revoked`.

### PID Cache

The API keeps an in-memory PID cache (`InMemoryPidCache`) that records PIDs
observed via storage lookups. Negative entries allow the handler to short-circuit
obvious 404 responses without hitting the database, while positive entries are
recorded after successful claims. The abstraction lives in `anon_ticket_domain`
so it can later be backed by Redis or a real Bloom filter.

## Monitor Service

`anon_ticket_monitor` polls `monero-wallet-rpc`'s `get_transfers` endpoint,
validates each PID via the domain helpers, and persists eligible payments using
`SeaOrmStorage`. Environment variables required:

- `MONERO_RPC_URL`
- `MONERO_RPC_USER`
- `MONERO_RPC_PASS`
- `MONITOR_START_HEIGHT`

The binary tracks the last processed height in the storage layer so it can
resume after restarts. Configure the RPC credentials to point at the wallet you
use for receiving PID-based transfers.
