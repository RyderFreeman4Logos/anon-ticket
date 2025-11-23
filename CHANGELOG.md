f2a598afe6d5a375b28ce0a3038c6de7352969db feat(storage): optimize for single-node performance (WAL, binary types, tinyint status)

- **Performance**: Enforced SQLite WAL mode and synchronous=NORMAL in `SeaOrmStorage::connect`.
- **Atomicity**: Reimplemented `claim_payment` using raw SQL `UPDATE ... RETURNING *` to eliminate race conditions.
- **Efficiency**: 
  - Refactored `PaymentId` and `ServiceToken` to use `[u8; 32]` (Binary) instead of Hex Strings.
  - Updated migrations to use `BLOB` (SQLite) / `BYTEA` (Postgres) for 2x index density.
  - Optimized `status` column to `TINYINT` (1 byte) instead of `VARCHAR(16)`.

4c5f97f5b965acbbda4b6d2338ca1ab046c6ff2b chore(storage): documentation and TODO updates for single-node strategy

- Updated `crates/storage/DESIGN.md` to reflect the "Single-Node Fortress" strategy, detailing WAL mode, atomic `RETURNING` queries, and binary storage optimizations.
- Updated `crates/storage/README.md` to highlight optimized SQLite configuration and binary storage features.
- Updated `TODO.md` and `TODO.zh.md` to track ShortTerm-20, 21, and 22, focusing on storage hardening and type optimizations.

3a8c1f0e2d9b4a5c6e7f8a9b0c1d2e3f4a5b6c7d feat(domain): remove dotenvy and refactor config loading

- Removed `dotenvy` dependency to align with production best practices (relying on shell environment or direnv).
- Updated `config.rs` to use `std::env::var` directly.
- Cleaned up `hydrating` logic.

1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c feat(domain): harden PaymentId and derive_service_token

- Refactored `PaymentId` to restrict construction: `new` is now `pub(crate)`, removed `From<&str>`.
- Added `PaymentId::generate()` using `getrandom` for secure ID creation.
- Added `try_from` for string parsing.
- Updated `derive_service_token` to include a separator (`|`) in the hash input to prevent concatenation attacks.
- Ensured `getrandom` has `js` feature enabled for Wasm compatibility.

... (previous history)