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
