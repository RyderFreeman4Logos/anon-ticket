//! Library entrypoint for embedding the monitor inside other binaries (e.g.,
//! the API process). The binary in `main.rs` remains available for
//! development/CI use but production should prefer in-process co-location so
//! the Bloom/cache can be updated immediately after ingestion.

pub mod pipeline;
pub mod rpc;
pub mod worker;

pub use rpc::{RpcTransferSource, TransferEntry, TransferSource, TransfersResponse};
pub use worker::{build_rpc_source, run_monitor, MonitorError, MonitorHooks};
