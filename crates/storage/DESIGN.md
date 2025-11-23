# The Single-Node Fortress: Storage Design in `anon-ticket`

In the era of microservices and distributed databases, `anon_ticket_storage` takes a contrarian approach: **The Single-Node Fortress**. By betting on vertical scalability and the raw speed of in-process databases (SQLite), we eliminate network latency, connection pooling overhead, and distributed consensus complexity.

This document outlines how we turned SQLite into a high-throughput production engine and how we balance ORM convenience with raw SQL performance.

## 1. Unleashing SQLite: WAL & Synchronous Modes

Out of the box, SQLite is tuned for safety and low resource usage, not high concurrency. To support the throughput required by a payment processor, we strictly enforce a production configuration in `SeaOrmStorage::connect`.

### Write-Ahead Logging (WAL)
We enable `PRAGMA journal_mode=WAL;`.
- **Concurrency**: In default `DELETE` mode, a write locks the entire database, blocking reads. In `WAL` mode, readers do not block writers, and writers do not block readers. This allows our API to serve token introspection requests even while the Monitor is actively ingesting blockchain transfers.

### Synchronous = NORMAL
We set `PRAGMA synchronous=NORMAL;`.
- **Trade-off**: The default `FULL` sync waits for the disk to physically finish writing content *and* metadata for every transaction. `NORMAL` ensures data is handed off to the OS filesystem cache safely.
- **Why**: While `WAL` + `NORMAL` theoretically risks database corruption on *OS crashes* (not process crashes), modern filesystems make this rare. The performance gain is massive (often 10x-100x more write transactions per second), which is critical for ingesting transaction bursts.

## 2. Bypassing the ORM for Atomicity

We use **SeaORM** for type-safe schema definitions and migrations, but we don't let it constrain our critical paths.

### The `claim_payment` Challenge
Redeeming a payment requires an atomic check-and-update: "Find a payment with `status='unclaimed'`, set it to `'claimed'`, and return it."
A naive ORM approach might do:
1. `SELECT * FROM payments WHERE pid = ?`
2. Check status in Rust.
3. `UPDATE payments ...`

This introduces a race condition (Double Spend).

### The `RETURNING` Solution
We utilize the `UPDATE ... RETURNING` clause supported by both SQLite (3.35+) and Postgres. Since SeaORM's high-level API abstracts this away, we drop down to `db.execute` using the `Statement` builder:

```sql
UPDATE payments
SET status = 1, claimed_at = NOW() -- 1 = Claimed
WHERE pid = $PID AND status = 0    -- 0 = Unclaimed
RETURNING *
```

This single SQL statement guarantees atomicity at the database engine level. It eliminates the need for application-level locks or "Select-for-Update" transactions, keeping the database contention window as small as possible.

## 3. Extreme Compactness: Binary IDs & TinyInts

We rigorously minimize the storage footprint to maximize cache locality and IO throughput.

### Binary Identifiers
We reject `TEXT` affinity for cryptographic identifiers.
- **PIDs & Tokens**: Defined as `BLOB` (SQLite) or `BYTEA` (Postgres) with a length of 32 bytes.
- **Rationale**:
    - **Space Efficiency**: Storing raw bytes instead of Hex strings (64 bytes) cuts the index size in half. Smaller indexes mean more pages fit in RAM.
    - **Performance**: Avoids redundant Hex encoding/decoding cycles at the storage layer.

### TinyInt Status
We map low-cardinality enums to single-byte integers.
- **PaymentStatus**: Stored as `TINYINT` (0 = Unclaimed, 1 = Claimed) instead of `VARCHAR(16)`.
- **Rationale**: Reduces row size by ~15 bytes per record. In a table with millions of payments, this saves tens of megabytes of RAM/disk and significantly reduces write amplification.

---

*By treating SQLite not as a file format, but as a high-performance engine, `anon_ticket_storage` delivers the throughput of Redis with the ACID guarantees of a relational database.*