# anon_ticket_monitor

The ingestion engine for the `anon-ticket` workspace. This service polls a `monero-wallet-rpc` instance, filters for incoming transactions containing valid Payment IDs (PIDs), and persists them to the SQLite database.

## 🚀 Features

- **Robust Polling**: Implements a stateful, "stop-and-wait" ingestion loop that resumes exactly where it left off after restarts.
- **Single-Node Fortress**: Optimized for local, high-throughput SQLite access using WAL mode and batch transactions.
- **Atomic Units**: Handles Monero amounts as `i64` (pico-nero) to maintain strict compatibility with SQLite's type system.
- **Idempotent**: Uses `INSERT ... ON CONFLICT DO NOTHING` to safely replay block ranges without duplicating payments.

## 🛠️ Configuration

The monitor is configured strictly via environment variables (or `.env` file loading via `direnv`).

| Variable | Description | Required |
| :--- | :--- | :--- |
| `DATABASE_URL` | Path to the SQLite database (e.g., `sqlite://ticket.db?mode=rwc`). | Yes |
| `MONERO_RPC_URL` | URL of the `monero-wallet-rpc` (e.g., `http://127.0.0.1:18083/json_rpc`). | Yes |
| `MONITOR_START_HEIGHT` | Block height to start scanning from if no state exists in DB. | Yes |
| `MONITOR_MIN_PAYMENT_AMOUNT` | Minimum atomic units required to persist a payment (defaults to `1_000_000`). | No |
| `RUST_LOG` | Tracing filter (e.g., `info,anon_ticket_monitor=debug`). | No |

## 🏗️ Architecture

The monitor is composed of three distinct layers:

1.  **RPC Layer (`rpc/`)**: A lightweight, `async-trait` backed client for `get_transfers` and `get_height`.
2.  **Pipeline (`pipeline.rs`)**: Validates PIDs (hex format, checksums) and transforms raw RPC entries into domain `NewPayment` models.
3.  **Worker (`worker.rs`)**: The main event loop that orchestrates fetching, processing, and height persistence.

For a deep dive into the design decisions (Polling vs Push, Error Handling, Type Constraints), see [DESIGN.md](./DESIGN.md).

## 📦 Usage

The monitor is designed to run as a standalone background service (systemd unit or Docker container).

```bash
# Run locally
export DATABASE_URL=sqlite://ticket.db
export MONERO_RPC_URL=http://127.0.0.1:18083/json_rpc
export MONITOR_START_HEIGHT=3000000

cargo run -p anon_ticket_monitor
```
