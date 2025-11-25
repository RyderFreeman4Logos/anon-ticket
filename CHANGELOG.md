1a571bf6 chore(repo): add workspace gitignore
Established a baseline `.gitignore` that covers Cargo builds, wasm-pack artifacts, IDE clutter, and local configuration in order to keep future Rust worktree clean despite the current minimal file set. Documented why several project-specific paths (matrix bot configs, proto outputs, drafts) remain untracked so contributors know where to place sensitive data without risking accidental commits. Highlighted the intent to pair every public English log with a private Chinese mirror while keeping the mirror ignored so bi-lingual context is preserved locally without polluting history; future entries should follow the same diary style.
af31d54 docs(todo): seed project roadmap
Outlined a tiered roadmap in `TODO.md` that bridges the high-level blueprint from `drafts/dev.md` with concrete, test-first tasks. Prioritized workspace scaffolding and deterministic environment setup as immediate items so later crates inherit consistent tooling, then layered storage, validation, API, monitor, caching, and observability milestones with explicit testing (sqlx integration, Actix HTTP harnesses, property tests) and documentation requirements (README sections, module docs, architecture notes). The checklist also encodes risk callouts—like trait abstraction churn, RPC availability, and cache DoS potential—to keep future contributors aware while enabling modular backends (SQLite now, Redis/moka later).
2702e1a feat(workspace): bootstrap cargo workspace
Stood up the multi-crate workspace with resolver 2, shared package metadata, and lint/format configs so every crate inherits the same rules. Added placeholder API/monitor binaries plus a domain crate that exposes `workspace_ready_message`, wiring the binaries through it to prove dependencies compile end-to-end. Documented the workspace layout/commands in `README.md`, ticked the Immediate-1 entry in `TODO.md`, and validated the scaffold by running `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` from the root.
dbfc2f0 feat(env): add deterministic config plumbing
Pinned the toolchain via `rust-toolchain.toml`, introduced `.env.example`, and documented the `config/` secrets workflow so every crate consumes the same deterministic environment. Extended the workspace manifest with shared `dotenvy`/`sha3`/`hex`/`thiserror` deps, taught the domain crate to expose `BootstrapConfig`, `ConfigError`, and a SHA3-256 PID fingerprint helper, and updated the API + monitor binaries to call those routines. README now explains the environment setup, TODO reflects Immediate-2 completion, and `.gitignore` shields secrets while `config/README.md` captures expectations. Verified everything with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.rustup-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
1895cbe chore(toolchain): bump rust to 1.91.1
Upgraded the workspace to Rust 1.91.1 via `rust-toolchain.toml` and reflected the change in README so contributors install the latest stable toolchain. Verified compatibility by running `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.rustup-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` on the new release.
6dcb33c feat(storage): add seaorm repository layer
Introduced the trait-based storage contracts in `anon_ticket_domain::storage` (PID/service token newtypes, storage errors, async traits) and wired them into the new `anon_ticket_storage` crate. The SeaORM adapter now runs schema migrations for SQLite/optional Postgres, offers a single `SeaOrmStorage::connect` helper, and ships integration tests that exercise payment claiming, token issuance/revocation, and monitor height persistence. README documents how to toggle between SQLite and Postgres features, and the ShortTerm-3 TODO item is checked off to reflect the completed storage milestone.
a992d88 fix(storage): atomically guard payment claims
Hardened `SeaOrmStorage::claim_payment` so that it performs a single atomic `UPDATE ... WHERE status = 'unclaimed'` before reading back the row, preventing concurrent claimers from double-spending the same PID. Added a regression test that launches two async claimers and asserts only one succeeds, ensuring the guard works under real concurrency. Verified the change with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.rustup-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
da571d8 fix(storage): gate tests per backend feature
Adjusted the `anon_ticket_storage` test suite to respect whichever SeaORM backend feature is enabled: SQLite runs against the in-memory DSN, while Postgres relies on a `TEST_POSTGRES_URL` env var, and unsupported combos skip tests with a clear message. README now explains how to set that env var before using the documented Postgres command. Validated the change with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.rustup-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
02f3e77 feat(domain): add pid validation and token helpers
Introduced `validate_pid`, `PaymentId::parse`, and `derive_service_token` so every crate can enforce the 32-character hex PID contract and derive SHA3-based service tokens consistently. The helpers ship unit tests that check invalid inputs, parsing, and deterministic outputs, plus README guidance encouraging callers to use the new APIs. Also ticked ShortTerm-4 in TODO.md to reflect the completed milestone. Validated via `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.rustup-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
1e76921 chore(domain): format pid validation test
Lightly reformatted the `pid_validation_rejects_invalid_inputs` assertion so the long hex string comparison is multi-line and easier to diff/scan during future edits. No behavior change, but keeps the growing domain helper tests consistent with rustfmt output.
7c44143 fix(storage): make payment insert idempotent
Reworked `SeaOrmStorage::insert_payment` to issue a single `INSERT ... ON CONFLICT DO NOTHING` via `exec_without_returning`, so duplicate PID inserts succeed instead of raising unique constraint errors under concurrent monitor tasks. Added a regression test (`inserting_same_pid_twice_is_ok`) to ensure the helper remains side-effect free when called twice. Validated with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.rustup-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
740fdfd feat(api): add redeem endpoint
Implemented the first Actix-Web surface for `POST /api/v1/redeem`: it validates PID format via the shared domain helpers, atomically claims payments through `SeaOrmStorage`, derives deterministic service tokens, and emits structured JSON success/ error responses (404, 409, etc.). Added an `AppState` wiring to BootstrapConfig + SeaORM in `main`, plus README docs covering request/response contracts. Integration tests now spin up the handler on SQLite to cover invalid PID, missing payment, success, and duplicate claim scenarios. Ran `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.rustup-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
c4a6d8c feat(monitor): implement rpc poller
Built the first version of `anon_ticket_monitor`: it now loads `BootstrapConfig`, connects to `SeaOrmStorage`, polls `monero-wallet-rpc/get_transfers`, validates PID format, inserts payments, and persists the last processed height so restarts resume safely. Added a lightweight JSON-RPC client module, reqwest-based HTTP plumbing, tracing, and README/TODO updates documenting the service and completing MidTerm-6. Verified with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.cargo-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
edaac9a fix(monitor): index incoming transfers
Patched the monitor RPC call to request `in` transfers (and deserialize them) so the loop now processes both inbound and outbound entries, ensuring customer payments are persisted instead of silently ignored. The run loop chains the two vectors before inserting payments, preserving height tracking. Verified with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.cargo-local cargo fmt --all` and `cargo clippy --workspace --all-features -- -D warnings`.
c5ec4e1 fix(monitor): persist max scanned height
Updated the monitor loop to track the maximum block height observed in each `get_transfers` batch and persist it once per batch. This prevents outgoing transfers with lower heights from regressing the stored cursor, ensuring the service continues making forward progress instead of reprocessing old blocks. Verified with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.cargo-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
a27beb9 feat(cache): add pid cache abstraction
Added `PidCache` + `InMemoryPidCache` to `anon_ticket_domain`, integrated it with the Actix API so negative lookups are memoized and successful claims mark PIDs as present. README now documents the cache behavior and TODO MidTerm-7 is complete. Tests cover the cache module and existing API integration continues to pass. Verified via `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.cargo-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
3f24251 fix(cache): never block storage lookups
Removed the negative-cache gate in `redeem_handler` so PIDs are always rechecked against storage, preventing "NotFound" entries from permanently poisoning legitimate redemptions before the monitor imports their payments. The cache now only records observations after a storage lookup (success, claimed, or absent). Verified with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.cargo-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
200c400 feat(api): add token introspection and revocation
Implemented `GET /api/v1/token/{token}` and `POST /api/v1/token/{token}/revoke`, including domain-backed `TokenStatusResponse`, revoke requests, and integration tests that cover active, missing, and revocation scenarios. AppState now wires in the PID cache alongside the storage handle, and README/TODO document the new APIs (closing MidTerm-8). Validated with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.cargo-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
2973e1a fix(monitor): advance height for non pid entries
Adjusted the monitor loop to track the highest block height observed in each RPC batch even when no PID-qualified transfer is processed, so a block full of unrelated transfers no longer stalls progress. Verified with `CARGO_HOME=$PWD/.cargo-local RUSTUP_HOME=$PWD/.cargo-local cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
38dcbc1 fix(config): isolate api bootstrap env vars
Extracted a dedicated `ApiConfig` from the domain crate so the Actix server only validates `DATABASE_URL` and `API_BIND_ADDRESS`, leaving the monitor-specific Monero RPC knobs in `BootstrapConfig`. Updated the API binary to load the new helper, tightened the config test suite with a mutex guard to prevent env races, and refreshed the README guidance so operators know which services require which variables. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
8585236 feat(observability): add shared telemetry and abuse tracking
Introduced a reusable `telemetry` module in the domain crate that wires `tracing-subscriber` + Prometheus metrics with optional HTTP listeners and an `AbuseTracker`, then plumbed it through the API (PID abuse counters, `/metrics` route, request outcome counters) and monitor (RPC success/failure gauges, ingestion metrics). Workspace manifests and crate-level `Cargo.toml` files now pull in `metrics`, `metrics-exporter-prometheus`, and `once_cell`, `.env.example`/README/TODO describe the new knobs, and tests were rerun via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features` to keep CI parity.
b84f523 fix(telemetry): treat empty metrics address as unset
Addressed the regression where `TelemetryConfig::from_env` persisted blank `*_METRICS_ADDRESS` values and caused `init_telemetry` to fail by trimming env input and dropping empty strings before parsing socket addresses. Added a focused unit test to lock in the behavior while keeping the existing mutex guard to avoid env races, then re-ran `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` to ensure the metrics opt-in flow works with the default `.env` template.
fe8cae5 fix(monitor): avoid double tracing initialization
Removed the redundant `tracing_subscriber::fmt::init()` call inside `run()` because `init_telemetry` already installs the global subscriber; keeping both triggered `SetGlobalDefaultError` and crashed the monitor on startup. Verified the fix with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` so telemetry remains the single entry point for logging/metrics.
512f79e fix(api): recover service tokens for claimed payments
Made the redeem endpoint idempotent: when a PID is already claimed the handler now re-derives the deterministic token, ensures the `service_tokens` row exists, and returns the token instead of surfacing `409 Conflict`. The happy path now records tokens via their actual claimed timestamp, metrics distinguish recovered claims, README documents the behavior, and the duplicate-claim test asserts a 200 response with the original token. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
2b8c509 fix(api): make token recovery idempotent
Hardened the `ensure_token_record` helper so that when a claimed PID is retried concurrently it catches unique violations from `insert_token`, re-reads the existing deterministic token, and returns it, keeping the `already_claimed` path consistent with the documented behavior. `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` confirm the API now handles double-submit races without 500s.
65e819b fix(monitor): persist next height and read pid cache
Updated the monitor loop to only advance the stored cursor when at least one transfer is ingested and to persist `max_height + 1`, preventing repeated processing of the most recent block when the chain is idle. On the API side, `redeem_handler` finally checks the PID cache before querying storage and a new integration test proves the negative cache short-circuits to 404 immediately. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` to ensure both binaries exhibit the expected behavior.
4eeb0a1 fix(monitor): only ingest incoming transfers
Updated the monitor RPC request to drop `out` transfers and limit ingestion to `incoming` entries, preventing wallet spends from being misinterpreted as claimable deposits. The response struct no longer carries an unused `out` field, so the JSON parsing matches the new behavior. `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` confirm the binary still builds and operates as expected.
8667ccf fix(monitor): persist cursor even without new transfers
Adjusted the monitor loop so it always writes `last_processed_height` based on the highest block returned—even when no qualifying payments were inserted—ensuring restarts resume from the last scanned block instead of rewinding to the prior payment. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
dcf0eb0 fix(cache): add ttl to pid cache
Implemented TTL-backed negative caching in `InMemoryPidCache` (defaults to 60s with a `new(ttl)` helper) so transient 404s no longer permanently block legitimate redemptions. The monitor now only advances its scan cursor when it actually observes transfer heights or queries `get_height`, preventing idle periods from skipping real deposits. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
9eac460 chore(ci): add cargo deny license check
Added a `deny.toml` configuration that restricts dependencies to permissive/commercial-friendly licenses and clarifies `ring`/`webpki` metadata, then wired a GitHub Actions workflow that runs `cargo deny check licenses` on every push/PR to `main` plus a daily cron + manual trigger. This keeps the dependency tree continuously vetted for license regressions so the project remains safe for closed-source commercial use.
f1b97e9 chore(licenses): relax cargo deny config
Cleaned up `deny.toml` to match the latest cargo-deny schema and explicitly allow the OpenSSL/Unicode-3.0 family used by transitive TLS + ICU crates, making the license gate practical for the current dependency graph. `cargo deny check licenses` now passes locally while still blocking copyleft additions.
a37231a feat(api): add unix and internal listeners
Extended `ApiConfig`/.env so the HTTP API can bind to Unix sockets (`API_UNIX_SOCKET`) and optional internal-only listeners (`API_INTERNAL_BIND_ADDRESS` or `API_INTERNAL_UNIX_SOCKET`). The server now prefers UDS when configured, auto-cleans stale sockets, and spins up a second listener for internal routes like `/metrics` while keeping a TCP fallback. README documents the new env vars, the cache gained TTL semantics, and the test suite covers config loading plus socket cleanup. Verified via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features`, and `cargo deny check licenses`.
59ebb17 fix(api): recheck cached-negative pids
Addressed the review finding that negative PID cache entries blocked redemption for the full TTL by letting `redeem_handler` treat them as hints instead of an authoritative gate and emitting a dedicated cache metric rather than short-circuiting to 404. Added an Actix test that marks a PID absent, inserts the payment afterward, and proves redemption still succeeds so clients who probe early can claim as soon as storage records the transfer. Verified the patch with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features`, and `cargo deny check licenses`.
e866883 fix(monitor): support unauthenticated rpc endpoints
Documented and implemented optional Monero wallet credentials so deployments that front a trusted RPC endpoint can omit HTTP Basic Auth entirely. `BootstrapConfig` now keeps `MONERO_RPC_USER/PASS` as `Option` and exposes tests covering unset as well as blank values to lock down the contract, while the monitor routes every request builder through a helper that only injects `Authorization` when credentials exist. README’s monitor section spells out the new env expectations, and the full fmt/clippy/test loop guards against regressions while leaving room to add token-protected RPCs later if abuse surfaces.
843e7fc docs(monitor): document watch-only wallet workflow
Extended the monitor section of README with a step-by-step guide for running monero-wallet-rpc against a hardware-backed watch-only wallet so every deployment keeps spend keys offline by default. The doc covers exporting address/view key, generating the watch-only wallet with a restore height, launching rpc with optional HTTP Basic Auth, and wiring MONERO_RPC_URL/MONITOR_START_HEIGHT, then explains why this topology should be the baseline operating model. Validated formatting-only change with the usual fmt/clippy/test loop to ensure no drift in the workspace.
557c740 fix(domain): canonicalize payment ids
Normalized every PaymentId constructor/parse path so the string is lowercased once and stored in a canonical form, closing the regression where monitor ingested lowercase RPC data but the API accepted uppercase input that never matched. Added explicit tests that uppercase values are normalized both via `PaymentId::parse` and `PaymentId::new`, ensuring storage, monitor, API, and token derivation stay case-insensitive without losing entropy. Verified via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
abed757 fix(api): enforce pid cache short-circuit
Operationalized the negative PID cache by teaching `redeem_handler` to bail out before hitting storage whenever `PidCache::might_contain` returns false, logging cache hints and escalating abuse metrics at the edge. Added a TTL-aware helper plus two Actix tests that prove requests are blocked while an entry is cached and retried successfully once the TTL expires, updated README to explain the 60s default, and introduced an env override so unit tests can skip `.env` hydration. Suite validated with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` despite the known `sqlx-postgres` future-incompat warning.
7127ada fix(api): add grace window for pid cache
Backed the PID cache short-circuit with a 500ms grace window by extending the cache trait to expose negative entry age and only blocking requests while that window is fresh; afterwards the handler re-validates against storage even though the TTL hasn’t expired. README documents the behavior, metrics labels distinguish blocked vs probed misses, and a new Actix test proves that requests succeed after the grace period while existing TTL-based expiry coverage remains. Verified via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` (future-incompat warning still emanates from `sqlx-postgres 0.7.4`).
REVIEW fix/monitor-dotenv-guard
Reviewer reported that `anon_ticket_monitor` bypasses the new `ANON_TICKET_SKIP_DOTENV` guard by invoking `dotenvy::dotenv()` before `BootstrapConfig::load_from_env`, so production deployments cannot disable `.env` hydration; this branch will remove the redundant call and rely fully on the shared config loader. Also tasked with ensuring future commits keep config interactions consistent across binaries.
ad3d722 fix(monitor): respect dotenv skip flag
The monitor binary no longer calls `dotenvy::dotenv()` on startup, instead relying entirely on `BootstrapConfig::load_from_env()` so the shared `hydrate_env_file` logic (including `ANON_TICKET_SKIP_DOTENV`) governs dotenv behavior uniformly across binaries. This removes the risk of production deployments accidentally loading a local `.env` and overwriting real credentials. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
1356512 fix(monitor): drop rpc auth support
Removed the unused `MONERO_RPC_USER/PASS` configuration, simplified `BootstrapConfig`, and stopped the monitor from adding Basic Auth headers so deployments no longer trip over digest-only RPC endpoints. README/.env.example now tell operators to run `monero-wallet-rpc --disable-rpc-login` (or front it with a proxy) and the workspace tests/clippy suite were re-run to confirm the streamlined contract.
REVIEW fix/bootstrap-monitor-vars
Reviewer flagged that `BootstrapConfig::load_from_env` still requires `API_BIND_ADDRESS`, causing monitor-only deployments to fail with MissingVar even though the monitor never uses API settings. Need to decouple monitor bootstrap from API-only vars so `DATABASE_URL`, `MONERO_RPC_URL`, and `MONITOR_START_HEIGHT` are sufficient.
678b06b fix(config): stop requiring api vars for monitor
Trimmed `BootstrapConfig` so it only validates `DATABASE_URL`, `MONERO_RPC_URL`, and `MONITOR_START_HEIGHT`, which lets the monitor start without the API-only `API_BIND_ADDRESS`. The API continues to load its own `ApiConfig`, so deployments can configure each binary independently. Ran `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` to confirm the leaner contract.
772c710 docs(todo): add structure refactor roadmap
Captured the upcoming structural work—domain module split, API handler/state/application layers, monitor pipeline traits, and storage crate decomposition—inside `TODO.md` so contributors share a single roadmap with explicit test, documentation, and risk expectations before touching code. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` to ensure the workspace stays green after documenting the plan.
fa66673 refactor(domain): split crate into modules
Reorganized `anon_ticket_domain` into `config`, `model`, `services`, and `storage::traits`, moved the PID cache/telemetry helpers into a `services` namespace, updated API/monitor/storage crates to import the scoped modules, refreshed the README with an internals section, and checked off the ShortTerm-12 roadmap entry. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features`, and `rg "\\p{Script=Han}" -n` to keep the workspace lint/quality gates green after the large move.
2287d36 refactor(api): separate application state and handlers
Split the Actix API crate into `application.rs` (bootstrap + server wiring), `state.rs` (shared handles), and `handlers/` (redeem/token/metrics modules) with tests moved into `tests.rs`; updated README + TODO to describe the new layout and keep ShortTerm-13 tracked. All existing Actix integration tests were preserved, and the suite now imports the scoped modules. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features`, and `rg "\\p{Script=Han}" -n`.
1a175fa refactor(monitor): split worker, pipeline, and rpc
Recomposed the monitor crate around `rpc::TransferSource`, a reusable ingestion pipeline, and a dedicated worker loop so alternative data sources can be injected for tests. `main.rs` now just boots config/telemetry, builds the SeaORM storage + RPC source, and hands both to `run_monitor`. README documents the new structure, TODO MidTerm-14 is checked off, and `async-trait` was added for the transfer-source abstraction. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features`, and `rg "\\p{Script=Han}" -n`.
eb30d03 chore(monitor): drop legacy client module
Removed the unused `client.rs` shim now that the RPC code lives under `rpc/`, keeping the crate tree aligned with the new module map. Workspace still passes `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features`, and `rg "\\p{Script=Han}" -n`.
9c23379 refactor(storage): split traits and add builder
Decomposed `anon_ticket_storage` into focused modules (`migration.rs`, `payment_store.rs`, `token_store.rs`, `monitor_state_store.rs`) and introduced a `StorageBuilder` so alternate backends/caching layers can wrap the connection before use. README documents the internals and TODO MidTerm-15 is checked off. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features`, and `rg "\\p{Script=Han}" -n`.
ed6f227 docs(todo): add domain hardening milestone
Captured ShortTerm-16 work in `TODO.md`, outlining removal of `AbuseTracker`, swapping the PID cache to moka with TTL, enforcing 64-hex (32-byte) PIDs, and trimming required env vars; refreshed the plan summary to stress domain hardening and the elimination of redundant abuse tracking. Verified the doc-only change with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`; next steps are to implement the moka migration and stricter PID length validation across crates.
f0868d4 chore(deps): add moka cache
Added moka 0.12.11 as a workspace-managed dependency (sync-only feature set) to replace the handwritten PID cache with lock-free TTL semantics in the upcoming refactor; keeping it at the workspace level ensures API/monitor/domain can share a consistent version. Verified no behavior change via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
f7b364f refactor(telemetry): remove abuse tracker
Removed the `AbuseTracker` and its env-driven thresholds from the shared telemetry module, simplifying `TelemetryConfig`/`TelemetryGuard` to metrics-only responsibilities. The API state now omits the tracker, redeem handler no longer records/escalates probes, and docs reflect that abuse signals come from PID cache hint counters instead of a dedicated tracker. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
f69e43c refactor(cache): back pid cache with moka
Replaced the lock-based PID cache with a moka `Cache` for both positive and negative entries, enabling lock-free lookups plus TTL-based eviction and bounded capacity (100k keys). Negative entries still expose age for the grace window logic, positives now expire with the same TTL to prevent unbounded growth. Validated via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
0075b23 feat(domain): enforce 64-char payment ids
Bumped `PID_LENGTH` to 64 hex characters (32 bytes) across validation, parsing, and tests, updated README examples, and refreshed cache/unit tests to use 64-char fixtures so all surfaces enforce the higher-entropy requirement. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
107cc49 fix(config): trim required env vars
Taught `get_required_var` to trim whitespace and treat empty strings as missing so `DATABASE_URL`, `API_BIND_ADDRESS`, and other required knobs fail fast instead of accepting blank values; added regressions asserting trimmed values and whitespace-only inputs. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
7fd7501 docs(todo): close domain hardening milestone
Checked off ShortTerm-16 in `TODO.md` and refreshed the plan summary to reflect the completed domain hardening (moka cache, 64-char PIDs, trimmed env parsing) and removal of abuse tracking. Change is documentation-only; code remained untouched and prior test runs already cover the new baseline.
e9e87b1 feat(domain): harden payment id construction
Restricted `PaymentId` creation to validated paths by making `new` crate-scoped, dropping `From<&str>`, adding `TryFrom<String>`, and introducing `PaymentId::generate()` backed by workspace `getrandom`; updated storage/token mapping and monitor ingestion to parse DB/RPC PIDs and refreshed API tests to use the validated 64-hex fixture. Ensured required env vars fail fast on blanks via trimmed parsing tests. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
373e2a9 fix(domain): add token hash separator and wasm rng feature
Hardened `derive_service_token` by hashing `pid|txid` (with an explicit `|` delimiter) to prevent future-length collisions and locked the behavior with a deterministic digest test. Enabled the `js` feature on the workspace `getrandom` so `PaymentId::generate()` builds cleanly on `wasm32` targets, and refreshed README/TODO to capture the finished roadmap items. This is a breaking change for existing service tokens because the derived values now differ; clients should recompute tokens with the new scheme. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` (the known `sqlx-postgres` future-incompat warning remains).
031981f docs(changelog): note token separator and wasm rng support
Documented the token hash separator and `getrandom` wasm feature toggle in the changelog alongside prior PID/token hardening changes. Documentation-only update validated by rerunning the fmt/clippy/test suite to keep the log accurate with recent breaking changes.
4898748 chore(config): remove dotenv auto-loading
Dropped the workspace `dotenvy` dependency and deleted `hydrate_env_file`, making all binaries rely solely on explicit `std::env::var` lookups; telemetry no longer attempts implicit `.env` hydration. README now instructs developers to export vars via `direnv` or `source .env`, and TODO marks ShortTerm-19 complete. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` (future-incompat warning from `sqlx-postgres` persists).
2054305a chore(todo): reprioritize bloom roadmap
Resequenced the Bloom-focused TODOs so in-process monitor co-location (prewarming Bloom/cache from storage) happens before enforcing Bloom-required startup, then clarified the Bloom-only redeem strategy (no negative-cache writes) and telemetry/sizing knobs. Added realistic defaults (1 GB, k≈6) with FPR examples to show small-memory deployments are acceptable while still allowing large-memory operators to push FP rates toward zero; documentation tasks will capture this posture for Tor/no-IP-limit environments.
2fb421d perf(storage): harden sqlite wal and atomic claims
Forced SQLite connections into WAL journaling with `synchronous=NORMAL` during storage initialization, reused the prepare path for the builder, tightened schema definitions to 64-character PIDs/txids/tokens, and rewrote `claim_payment` to a single `UPDATE ... RETURNING` so claims return the updated row without a second query. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings` (known `sqlx-postgres` future-incompat warning), and `cargo test --all --all-features`; note existing SQLite deployments keep their original column widths and may need a follow-up migration if already provisioned.
dc2e667 refactor(core): store pid and token as bytes
Shifted `PaymentId` and `ServiceToken` internals to `[u8;32]`, keeping hex at API boundaries while persisting raw bytes. Storage migrations now declare BLOB/BYTEA columns for PIDs/tokens, SeaORM entities map to `Vec<u8>`, and caches operate on byte keys. Hash derivation remains the same (still hashes hex) so service tokens are stable; API/token handlers validate hex input and continue returning hex strings. Verified via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features` (the known `sqlx-postgres` future-incompat warning persists).
f2a598afe6d5a375b28ce0a3038c6de7352969db feat(storage): optimize for single-node performance (WAL, binary types, tinyint status)

- **Performance**: Enforced SQLite WAL mode and synchronous=NORMAL in `SeaOrmStorage::connect`.
- **Atomicity**: Reimplemented `claim_payment` using raw SQL `UPDATE ... RETURNING *` to eliminate race conditions.
- **Efficiency**: 
  - Refactored `PaymentId` and `ServiceToken` to use `[u8; 32]` (Binary) instead of Hex Strings.
  - Updated migrations to use `BLOB` (SQLite) / `BYTEA` (Postgres) for 2x index density.
  - Optimized `status` column to `TINYINT` (1 byte) instead of `VARCHAR(16)`.

9db62e55026e43f208d577ecd8708b7dd5d521c2 chore(storage): update changelog and todo for storage optimization

- Updated `crates/storage/DESIGN.md` to reflect the "Single-Node Fortress" strategy, detailing WAL mode, atomic `RETURNING` queries, and binary storage optimizations.
- Updated `crates/storage/README.md` to highlight optimized SQLite configuration and binary storage features.
- Updated `TODO.md` and `TODO.zh.md` to track ShortTerm-20, 21, and 22, focusing on storage hardening and type optimizations.

3f9dab196a09806443c2999e8293796d559e223b docs(storage): update design and readme with tinyint and wal details

- Updated `DESIGN.md` and `README.md` to explicitly mention `TINYINT` status optimization and expand on "Compact Schema" benefits.

28584c7e9563cad6c78b75343e56790cf7556e88 feat(monitor): harden worker loop and add tests

- **Robustness**: Updated `run_monitor` to catch and log transient storage errors instead of panicking, ensuring the service survives DB glitches.
- **Refactor**: Genericized `run_monitor` and `process_entry` to accept any implementation of `PaymentStore` + `MonitorStateStore`, enabling easier testing.
- **Cleanup**: Removed redundant `validate_pid` checks in pipeline, relying on the hardened `PaymentId::parse` logic.
- **Testing**: Added unit tests for `handle_batch` error propagation using Mock storage.

f67e6a8e32906859336124705001389803874020 chore(monitor): update changelog for robustness fixes

- Updated CHANGELOGs to record monitor hardening work.
8a343f8... chore(monitor): fix rpc tests and add technical blog

- **Testing**: Added unit tests for `RpcTransferSource` JSON deserialization to verify field mappings.
- **Documentation**: Added `crates/monitor/BLOG.md` detailing the "Single-Node Fortress" design philosophy for payment monitoring.
- **Refinement**: Added code comments explaining the `i64` amount type constraint.
53d553d... docs(monitor): rename blog to design and add readme

- **Refactor**: Renamed `BLOG.md` to `DESIGN.md` to align with workspace conventions.
- **Docs**: Added `README.md` for `anon_ticket_monitor` covering configuration and architecture overview.
c7fc8f0... chore(todo): plan short-term dust filtering

- Planned `ShortTerm-24` to implement a minimum payment amount filter in the monitor, mitigating resource exhaustion attacks from dust transactions.

e7be3447d3c09f5a8d57ba732a2d56e928f2f3b0 feat(monitor): drop dust payments below threshold

- **Config**: Added optional `MONITOR_MIN_PAYMENT_AMOUNT` to `BootstrapConfig` (default `1_000_000` atomic units) with parsing tests to enforce trimmed, numeric input.
- **Ingestion**: `process_entry` now rejects transfers below the minimum, logs a warning, and increments a `dust` counter instead of persisting low-value records.
- **Worker Wiring**: Propagated the threshold through `run_monitor`/`handle_batch` and expanded unit tests to cover dust filtering paths.
- **Docs & Tracking**: Documented the new env var in `crates/monitor/README.md` and checked off ShortTerm-24 in `TODO.md`.
- **Verification**: Ran `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.

ffaba9d0b59c8d23d932a4d418f5e8ef5e3cd6e0 feat(domain): migrate payment ids to 64-bit blobs

- **PID Contract**: Reframed `PaymentId` as an encrypted `[u8; 8]` (16-hex) payload, updating validation, parsing, canonicalization, and deterministic token derivation to the new width while keeping `ServiceToken` at 32 bytes.
- **Storage**: Shrunk `payments.pid` and `service_tokens.pid` columns to `binary(8)` in migrations/entities and aligned SeaORM stores with the new byte-length conversions.
- **Runtime Surfaces**: Refreshed API and monitor fixtures/tests to use 16-char PIDs, kept the existing lightweight RPC types (no monero crate migration yet), and ensured cache keys follow the compact PID length.
- **Docs & Tracking**: Updated README PID guidance, domain design notes, and checked off ShortTerm-25 in TODO.
- **Verification**: Ran `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
4c74338... docs(monitor): add watch-only wallet deployment guide

- Added `crates/monitor/secure-monero-rpc-deployment.md` detailing how to export view keys and run a watch-only `monero-wallet-rpc`.
- Updated `README.md` to reference the secure deployment guide.
7cd8db6... chore(security): plan integrated address migration

- **Security**: Identified cleartext legacy PID vulnerability and planned migration to encrypted 64-bit Integrated Addresses (ShortTerm-25).
- **Documentation**: Added `docs/security/payment-id-security.md` proving the safety of 64-bit PIDs against brute-force attacks.

844fdbbc0a706d1bd864b33defc879ae2105fbe6 chore(todo): mark monero adoption task done

- Checked off ShortTerm-26 after landing monero 0.21/monero-rpc 0.5 adoption, integrated-address helper, and monitor RPC refactor; TODO now focuses on finishing WASM guard (ShortTerm-27).

d5a7746d5e21d16849655a176ebf37dc8f8386bb chore(changelog): log todo completion

- Documented the ShortTerm-26 completion in the changelog after updating TODO.

e6c663e76c33880ad4cd7e510a50da0f19038359 feat(domain): add wasm feature flag and docs

- Added a domain-only `wasm` feature enabling `getrandom/js` for wasm32 builds and documented the build command `cargo build -p anon_ticket_domain --target wasm32-unknown-unknown --features wasm`.
- Updated `crates/domain/README.md` with WASM usage notes; mirrored in `README.zh.md` (ignored by git).
- Marked ShortTerm-27 as completed in TODOs; domain remains the only crate with wasm support.

4202ce8fe06614c3ea7dfbe97f45c3918f48b5a1 feat(monero): add wasm-safe integrated addresses and monero-rpc

- **Deps & WASM**: Upgraded workspace to `monero 0.21`, `monero-rpc 0.5`, `getrandom 0.3`, and added `cfg-if`, with target-specific `wasm32` gating (`wasm_js`) to keep PaymentId generation browser-safe.
- **Domain API**: Introduced `integrated_address` module exposing `build_integrated_address`/`decode_integrated_address` helpers on top of monero types for FFI/wasm usage; added cfg_if-backed randomness shim and unit tests.
- **Monitor RPC**: Replaced hand-rolled JSON structs with `monero_rpc::WalletClient` + `GotTransfer` mapping, preserving `TransferEntry` shape, guarding amount overflow, and decoding 64-bit payment IDs; updated tests accordingly.
- **Tracking**: Added ShortTerm-26/27 tasks to TODO for monero adoption and wasm gating; ran `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
0040f85... docs(storage): reflect 8-byte PID in design docs

- Updated `DESIGN.md` and `README.md` to accurately describe the 8-byte Compact PID schema.

4c84cab5c410c624f4e714833f2ba0009488dd40 feat(monitor): make poll interval configurable

- **Config**: Added `MONITOR_POLL_INTERVAL_SECS` to `BootstrapConfig` with a 5s default and tests covering default and override cases.
- **Worker**: `run_monitor` now sleeps using the configurable interval instead of a hardcoded 5s.
- **Docs**: Updated monitor README with the new env var plus a Metrics & Observability section; root READMEs list the optional knobs; TODOs marked ShortTerm-28 done.
- **Verification**: Ran `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.

8ac8819c4b330256035e500832f14a845d4e66ce fix(config): trim monitor env parsing

- **Stability**: `BootstrapConfig` now trims whitespace before parsing `MONITOR_MIN_PAYMENT_AMOUNT` and `MONITOR_POLL_INTERVAL_SECS`, preventing env whitespace from breaking number parsing.
- **Tests**: `set_env()` clears `MONITOR_POLL_INTERVAL_SECS` so config tests stay deterministic when developers export the var locally.
- **Verification**: Ran `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.

4bc4676f9e0b0cc1b21f1c20fd3f88c92f2a7cce fix(api): restrict revoke endpoint to internal listener

- **Access Control**: Moved `POST /api/v1/token/{token}/revoke` off the public listener and onto the internal listener in `application.rs`; public requests now 404 unless an internal socket/port is configured.
- **Tests**: Added integration coverage to assert public 404 vs internal 200 and to confirm the token status reflects revocation.
- **Docs**: Updated README (and local zh mirror) to document the internal-only revoke route; checked off ShortTerm-29 in TODOs.
- **Verification**: Ran `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
5ef6236... chore(todo): plan api security hardening

- Planned **ShortTerm-29** to move the privileged `revoke` endpoint to the internal listener, preventing unauthorized access from the public internet.
f09beac... chore(todo): plan monitor confirmation safety

- Planned **ShortTerm-30** to enforce confirmation depth in the monitor loop, preventing reorg attacks by ignoring immature transactions.

958e406 feat(monitor): enforce confirmation window

- Added `MONITOR_MIN_CONFIRMATIONS` (default `10`) to `BootstrapConfig`, trimming and testing overrides so deployments can bound reorg risk without recompiling.
- Refactored the monitor loop to compute `safe_height = wallet_height - min_confirmations`, skip polling when the cursor is ahead of the safety window, clamp transfer fetches to an inclusive `[start, safe_height]` range, and persist the cursor no further than `safe_height + 1`.
- Updated `TransferSource` to accept a max height, refreshed monitor docs/README and design notes, and checked off ShortTerm-30 in TODO.
- Added unit tests covering confirmation gating and height advancement, plus config coverage; verified via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
e33dd7c... docs(api): add design and readme

- Added `crates/api/DESIGN.md` detailing the Dual-Listener architecture and security model.
- Added `crates/api/README.md` with configuration reference and API specs.
d33171d... docs(api): clarify dos protection strategy

- Clarified in `crates/api/DESIGN.md` that DoS protection relies on internal Negative Caching rather than IP rate-limiting (which is ineffective for Tor services).

2252f95 docs(todo): plan pid cache tuning

- Planned PID negative-cache tuning: add `API_PID_CACHE_NEGATIVE_GRACE_MS` (bounded >0) surfaced via `ApiConfig`, and keep handlers enforcing `grace <= ttl` so cold-start floods are throttled without blocking fresh payments too long.
- Planned cache configurability: introduce `API_PID_CACHE_TTL_SECS` and `API_PID_CACHE_CAPACITY` to tune positive/negative cache retention and memory footprint, with validation that TTL stays above the grace window and docs with sizing guidance.
- Planned Bloom filter layer: design a PID Bloom filter with zero false negatives (false positives acceptable), fed by cache/storage and tunable for FP rate/refresh cadence, to cut DB load under spray traffic while preserving correctness.

6c6818a feat(api): make pid cache configurable

- Added env-driven tuning for PID cache: `API_PID_CACHE_TTL_SECS`, `API_PID_CACHE_CAPACITY`, and `API_PID_CACHE_NEGATIVE_GRACE_MS` (ms) now feed `ApiConfig`, defaulting to 60s/100k/500ms.
- Validated configuration to ensure cache TTL is never shorter than the negative-grace window; surfaced a bootstrap error when misconfigured.
- Wired application bootstrap to honor the configured TTL/capacity and pass the grace window into handler state; made cache defaults public and adjustable via `InMemoryPidCache::with_capacity`.
- Updated `.env.example` and API/Root READMEs with the new knobs; refreshed integration/config tests to cover parsing, invalid numbers, and dynamic grace handling. Verified via `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.

816101f feat(api): add pid bloom filter

- Added domain-level `PidBloom` wrapper (Atomic Bloom) with config validation and no-false-negative semantics; exposed env knobs `API_PID_BLOOM_ENTRIES` and `API_PID_BLOOM_FP_RATE` (defaults 100k / 0.01).
- Redeem handler now consults Bloom to avoid blocking real payments during negative-cache grace while still allowing false positives; Bloom is populated on successful or known-present lookups.
- `ApiConfig` and bootstrap wire the Bloom filter and PID cache together, maintaining TTL ≥ grace invariant and disabling Bloom when entries set to 0.
- Updated `.env.example`, README、API README, TODOs (MidTerm-33 done), plus config/unit/integration tests to cover new parsing paths and Bloom behavior. Verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --all --all-features`.

f492bac fix(api): prefill bloom to bypass negative cache

- Populate Bloom immediately when marking a PID absent/present so the negative-cache grace window no longer blocks fresh payments; Bloom positives now skip the short-circuit.
- Added integration test proving a payment that lands during the grace window redeems successfully because Bloom hints the handler to recheck storage.
- Kept Bloom optional and false-positive-tolerant; verified with `cargo fmt --all`, `cargo clippy --workspace --all-features -- -D warnings`, and `cargo test --all --all-features`.
