# Anon-Ticket

Anon-Ticket is organized as a Cargo workspace so that the HTTP API, monitor
service, and domain primitives can evolve independently while sharing the same
lint/test configuration.

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
