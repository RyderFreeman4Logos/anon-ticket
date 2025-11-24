//! Domain-level building blocks shared across API and monitor crates.
//!
//! The crate now exposes cohesive modules for configuration (`config`),
//! data models (`model`), reusable services such as telemetry (`services`),
//! and storage contracts (`storage`). Downstream crates can import individual
//! modules directly or rely on the curated re-exports below.

pub mod config;
pub mod integrated_address;
pub mod model;
pub mod services;
pub mod storage;

pub use config::{ApiConfig, BootstrapConfig, ConfigError};
pub use integrated_address::*;
pub use model::*;
pub use services::cache::*;
pub use services::telemetry::*;
pub use storage::traits::*;
