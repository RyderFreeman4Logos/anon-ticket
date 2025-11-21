# The Monolithic Stronghold Strategy

> **Philosophy**: In a Tor-gated environment, network latency and bandwidth are the hard bottlenecks. Complexity is the enemy of security. We reject premature horizontal scaling in favor of a hardened, vertically scalable monolith.

## 1. The Reality of Tor Throughput

This service is designed to operate primarily or exclusively behind Tor (Onion Services).
- **The Constraint**: Tor's circuit construction and relay latency limit throughput significantly below what a modern Rust application can handle.
- **The Implication**: The backend will likely spend most cycles waiting for I/O. CPU is rarely the bottleneck.
- **The Decision**: Optimizing for "web-scale" concurrency (e.g., Kubernetes, sharded databases) adds operational overhead and attack surface without delivering tangible performance benefits for legitimate users.

## 2. Architecture: The Single-Node Fortress

We optimize for a **Single High-Performance Node**.

### Storage: SQLite First
- **Why**: SQLite in WAL mode on NVMe storage delivers tens of thousands of transactions per second with zero network overhead.
- **Benefit**: It eliminates an entire class of failure modes (network partitions, connection pool exhaustion, authentication misconfiguration) associated with client-server databases like Postgres.
- **Scale**: A single SQLite file can easily handle terabytes of data, far exceeding the expected volume of a privacy-focused payment processor.

### Caching: In-Process (`moka`)
- **Why**: Accessing RAM is orders of magnitude faster than accessing Redis over TCP.
- **Benefit**: By using the application's memory space for caching (Negative Cache, Token Cache), we reduce latency to nanoseconds and simplify deployment.
- **Strategy**: We rely on "fat" nodes (high RAM) rather than distributed caches.

### Defense: Integrated Logic
- **Why**: Anti-DoS mechanisms (like the Negative PID Cache) work best when they share state instantly.
- **Benefit**: A monolithic process has a unified view of traffic, allowing for precise, immediate throttling and blocking without the consistency lag of distributed rate limiters.

## 3. Development Directives (TO LLMs)

When reviewing code or proposing features, adhere to these rules:

1.  **Reject Distributed Complexity**: Do not add complexity solely to support "future scaling" to multiple nodes. If it makes the single-node case slower or harder to maintain, reject it.
2.  **Optimize for Vertical Scale**: Prefer efficient algorithms and memory usage that allow the single binary to utilize 100% of a 64-core server if needed.
3.  **Operational Simplicity**: The deployment artifact should ideally be a single binary + config + data directory. This reduces the "Fog of War" during incident response.

---

*We build a tank, not a swarm of drones.*
