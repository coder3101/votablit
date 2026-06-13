# Votablit

**Vote + Abliterate** — The community decides which AI model gets abliterated next. Vote for your pick, the top model gets processed weekly, and the result is published on HuggingFace.

## Stack

- **Backend:** Rust, Axum 0.8, SQLite via sqlx 0.9
- **Frontend:** Askama templates, HTMX, vanilla CSS
- **Deploy:** Fly.io single instance with persistent volume
- **CI:** GitHub Actions (clippy + nextest on push)

## Quick Start

```bash
# Prerequisites: Rust 1.85+, cargo-nextest, sqlx-cli
cargo install cargo-nextest sqlx-cli

# Run locally
ADMIN_TOKEN=dev-secret make run

# Open http://localhost:8080
```

The database is created automatically on first launch at `./leaderboard.db` (configurable via `DATABASE_PATH`).

## Makefile

```bash
make help            # list all targets
make run             # run locally (ADMIN_TOKEN=dev-secret)
make test            # run all 81 tests
make lint            # clippy
make ci              # lint + test
make sqlx-prepare    # regenerate offline query cache
make docker-run      # build + run in Docker
make deploy          # fly deploy
make clean           # remove artifacts
```

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ADMIN_TOKEN` | Yes | -- | Bearer token for admin endpoints (server refuses to start without it) |
| `DATABASE_PATH` | No | `leaderboard.db` | Path to SQLite database file |
| `BIND_ADDR` | No | `0.0.0.0:8080` | Server bind address |
| `RUST_LOG` | No | `votablit=info,tower_http=info` | Log level filter |

## API

### Public

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Main page (HTML) |
| `GET` | `/api/leaderboard` | All models by vote count (JSON) |
| `GET` | `/api/deliveries?page=1&per_page=10` | Abliterated models, paginated (JSON) |
| `POST` | `/api/vote` | Cast a vote |
| `POST` | `/api/models` | Submit a model for abliteration |

### Admin (requires `Authorization: Bearer <ADMIN_TOKEN>`)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/admin/models` | Create model with optional HF link |
| `PUT` | `/api/admin/models/{id}` | Update a model's HF link |
| `DELETE` | `/api/admin/models/{id}` | Delete a model and its votes |
| `DELETE` | `/api/admin/prune` | Delete all zero-vote models |
| `POST` | `/api/admin/deliver` | Record an abliterated model delivery |

### Rate Limits

- **Voting:** 3 votes per IP per hour, 1 vote per model per UUID
- **Model submission:** 1 per IP per day
- **Board cap:** 50 models max

## Tests

```bash
make test
```

81 tests covering queries, extractors, HF validation, DB migrations, all API endpoints, rate limiting, auth, pagination, and edge cases.

See [DEVELOPMENT.md](DEVELOPMENT.md) for test architecture, security hardening details, and development workflow.
