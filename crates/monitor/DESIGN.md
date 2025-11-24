# Building a "Single-Node Fortress" Monitor for Monero

In the `anon-ticket` architecture, the **Monitor** service acts as the bridge between the immutable ledger of Monero and our mutable, SQL-backed application state. Its job is deceptively simple: watch the blockchain for payments and issue tickets. But in a system designed for "Single-Node Fortress" reliability, "watching" implies a set of rigorous guarantees.

This post dives into the design of `anon_ticket_monitor`, explaining how we process cryptocurrency transactions with high fidelity and minimal complexity.

## Architecture: The Polling Loop

Unlike some architectures that rely on ZeroMQ or websocket pushes, we opted for a robust **polling model** against the `monero-wallet-rpc`.

```rust
pub async fn run_monitor<S, D>(...) {
    // 1. Recover state
    let mut height = storage.last_processed_height().await?.unwrap_or(config.start_height);

    loop {
        // 2. Fetch & Process
        match source.fetch_transfers(height).await {
            Ok(transfers) => {
                if let Err(e) = handle_batch(..., transfers, &mut height).await {
                    warn!("batch failed, retrying: {}", e);
                }
            }
            Err(e) => warn!("rpc failed: {}", e),
        }
        // 3. Pace
        sleep(Duration::from_secs(5)).await;
    }
}
```

### Why Polling?
1.  **Resilience**: If the monitor crashes, it simply restarts, reads the `last_processed_height` from SQLite, and resumes exactly where it left off. There are no "missed events" to replay.
2.  **Flow Control**: The monitor dictates the pace. It won't be overwhelmed by a flood of socket messages; it processes one batch at a time.
3.  **Simplicity**: No complex async stream management or reconnection logic for sockets. Just `loop` and `sleep`.

## Data Pipeline: From RPC to Storage

The pipeline is designed to be **idempotent** and **type-safe**.

1.  **Ingestion**: We fetch transfers using `get_transfers` with a `min_height`.
2.  **Validation**:
    *   We strictly validate the `payment_id` (PID). In our system, the PID is a 32-byte hex string derived from a cryptographic hash.
    *   Invalid PIDs are logged and discarded immediately, preventing database pollution.
3.  **Persistence**:
    *   Valid payments are inserted into SQLite.
    *   Crucially, we use `INSERT ... ON CONFLICT DO NOTHING`. This allows us to re-process the same block range without fear of duplicate records or errors.

### The `i64` Constraint
One specific design choice involves the transaction `amount`. Monero atomic units (pico-nero) are large integers. While Rust's `u64` fits the total supply (~18.4 million XMR), standard SQLite `INTEGER` types are signed 64-bit (`i64`).

To maintain compatibility with our embedded database strategy, we store amounts as `i64`. This imposes a theoretical limit of ~9.22 million XMR per single transaction. Since `anon-ticket` is designed for micro-payments (tickets), this trade-off is acceptable and documented in our types.

## Security & Robustness

### Input Sanitization
The monitor treats the RPC response as untrusted input in one specific regard: **PIDs**. While we trust the node to report valid amounts and heights, the `payment_id` field is user-controlled data on the blockchain. We parse it into a hardened `[u8; 8]` type (Compact Payment ID) before it ever touches our domain logic.

### Transient Failures
Network glitches and database locks are inevitable. The worker loop wraps the batch processor in a `Result` block. If the database is temporarily locked (even with WAL mode) or the RPC times out, the monitor logs a warning and retries the *same* height in the next cycle. It never advances the cursor until persistence succeeds.

## Conclusion

`anon_ticket_monitor` demonstrates that reliability doesn't require complex distributed systems. By leveraging a strong polling contract, idempotent storage operations, and strict type validation, we build a payment listener that is easy to reason about and hard to break—a true component of a fortress.
