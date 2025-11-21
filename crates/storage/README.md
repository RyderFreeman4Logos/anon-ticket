# anon_ticket_storage

The persistence layer for the anon-ticket workspace. This crate implements the storage traits defined in `anon_ticket_domain` using **SeaORM**, providing a robust and swappable backend.

## 🚀 Features

- **SQLite First (Optimized)**: By default, this crate configures SQLite in **WAL Mode** with `synchronous=NORMAL`, transforming it from a embedded library into a high-concurrency transaction engine suitable for the "Single-Node Fortress" architecture.
- **Binary Storage**: Stores Payment IDs and Service Tokens as compact `BLOB`s (32 bytes) rather than Hex strings, halving index size and maximizing cache efficiency.
- **Postgres Compatible**: Can be compiled with the `postgres` feature for deployments requiring remote storage.
- **Atomic Operations**: Implements critical business logic (like `claim_payment`) using atomic `UPDATE ... RETURNING` queries to prevent race conditions without application-level locking.
- **Migrations**: Built-in, idempotent schema migrations ensure the database is always in the correct state upon startup.

## 🛠️ Usage

```rust
use anon_ticket_storage::SeaOrmStorage;

// Connects and automatically runs migrations
// Detects SQLite and applies performance PRAGMAs
let storage = SeaOrmStorage::connect("sqlite://ticket.db?mode=rwc").await?;

// Use via domain traits
// use anon_ticket_domain::storage::PaymentStore;
// storage.claim_payment(&pid).await?;
```

## 🏗️ Architecture

This crate bridges the gap between the abstract `domain` traits and the concrete SQL database. It handles:
1.  **Mapping**: Converting SeaORM `Model` structs to Domain `Record` structs (with validation).
2.  **Optimization**: Injecting backend-specific SQL (e.g., `RETURNING` clauses) where the ORM abstraction is too slow or limiting.

For a detailed look at the performance optimizations, see [DESIGN.md](./DESIGN.md).