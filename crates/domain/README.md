# anon_ticket_domain

The **Shared Kernel** of the anon-ticket workspace. This crate contains the core business logic, data models, and interfaces that drive the entire system. It is designed to be the "source of truth" for validation, security primitives, and configuration.

## 🧱 Modules

### `model`
**The Data Contract.**
- Defines strong types like `PaymentId` (64-char hex) and `ServiceToken`.
- Centralizes validation logic (`validate_pid`) to prevent "string typing."
- Implements deterministic crypto derivations (SHA3-256) for token generation.

### `services`
**Reusable Infrastructure.**
- **`cache`**: A high-performance `moka`-based caching layer implementing the "Negative Cache" pattern to protect the database from DoS attacks.
- **`telemetry`**: A unified observability stack that wires up `tracing` and `prometheus` metrics with consistent configuration across all binaries.

### `storage`
**Persistence Interfaces.**
- Defines `async_trait` contracts (`PaymentStore`, `TokenStore`, `MonitorStateStore`) that decouple business logic from the underlying database implementation (SeaORM).

### `config`
**Deterministic Environment.**
- `BootstrapConfig` & `ApiConfig`: Centralized parsing of `.env` and process variables.
- Ensures all binaries (API, Monitor) fail fast on missing configuration and share the same connecting parameters.

## 📦 Usage

This crate is consumed by:
- **`anon_ticket_api`**: Uses models for request validation, storage traits for DB access, and services for telemetry/caching.
- **`anon_ticket_monitor`**: Uses storage traits to persist incoming transfers and config for bootstrapping.
- **`anon_ticket_storage`**: Implements the traits defined here.

## 🛡️ Security Philosophy

This crate enforces a **"Parse, Don't Validate"** approach. By wrapping primitives in newtypes (e.g., `PaymentId`), we ensure that it is impossible for downstream code to represent or pass around invalid data. If you hold a `PaymentId`, it is guaranteed to be valid.

For a deeper dive into the design decisions, see [DESIGN.md](./DESIGN.md).
