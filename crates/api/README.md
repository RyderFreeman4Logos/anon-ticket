# anon_ticket_api

The HTTP REST gateway for the anon-ticket system. It provides endpoints for users to redeem payments for tokens and for admins to manage the system.

## 🚀 Features

- **Dual-Listener**: Separates public traffic from internal admin/metrics traffic for "Defense in Depth".
- **Unix Socket Support**: Native support for binding to Unix Domain Sockets, ideal for secure IPC with Nginx or Tor.
- **Atomic Redemption**: Guarantees no double-spending of Payment IDs.
- **DoS Protection**: Built-in negative caching to protect the database from invalid PID spam.

## 🛠️ Configuration

Configured via environment variables.

### Public Interface
| Variable | Description | Default |
| :--- | :--- | :--- |
| `API_BIND_ADDRESS` | TCP address for public traffic (e.g. `0.0.0.0:8080`). | `127.0.0.1:8080` |
| `API_UNIX_SOCKET` | Path to Unix socket (overrides TCP if set). | `None` |

### Internal Interface
| Variable | Description | Default |
| :--- | :--- | :--- |
| `INTERNAL_BIND_ADDRESS` | TCP address for admin/metrics (e.g. `127.0.0.1:9090`). | `None` (Disabled) |
| `INTERNAL_UNIX_SOCKET` | Path to internal Unix socket. | `None` |

### Dependencies
| Variable | Description | Required |
| :--- | :--- | :--- |
| `DATABASE_URL` | Connection string for SQLite/Postgres. | Yes |

## 📚 API Reference

### Public Endpoints

#### `POST /api/v1/redeem`
Exchanges a Payment ID for a Service Token.
- **Body**: `{ "pid": "16_char_hex_string" }`
- **Response**: `{ "status": "success", "service_token": "...", "balance": 1000 }`

#### `GET /api/v1/token/{token}`
Checks the status of a Service Token.
- **Response**: `{ "status": "active|revoked", "amount": 1000, ... }`

### Internal Endpoints

#### `GET /metrics`
Prometheus metrics exposition.

#### `POST /api/v1/token/{token}/revoke`
**Admin Only**. Revokes a token immediately.
- **Body**: `{ "reason": "abuse", "abuse_score": 100 }`
- **Response**: `{ "status": "revoked", ... }`

## 📦 Usage

```bash
# Run locally with both ports open
export DATABASE_URL=sqlite://ticket.db
export API_BIND_ADDRESS=127.0.0.1:8080
export INTERNAL_BIND_ADDRESS=127.0.0.1:9090

cargo run -p anon_ticket_api
```
