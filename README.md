<img src="assets/icons/project-header.png" alt="Votablit — Vote + Abliterate" width="100%">

[![CI](https://github.com/coder3101/votablit/actions/workflows/ci.yml/badge.svg)](https://github.com/coder3101/votablit/actions/workflows/ci.yml)
[![Deploy](https://img.shields.io/badge/deployed-live-brightgreen)](https://votablit.fly.dev)

The community decides which AI model gets abliterated next. Vote for your pick, the top model gets processed weekly, and the result is published on [HuggingFace](https://huggingface.co).

## Quick Start

```bash
# Prerequisites: Rust 1.85+, cargo-nextest, sqlx-cli
cargo install cargo-nextest sqlx-cli

# Run locally
ADMIN_TOKEN=dev-secret make run

# Open http://localhost:8080
```

The database is created automatically on first launch at `./leaderboard.db` (configurable via `DATABASE_PATH`).

## Stack

- **Backend:** Rust, Axum 0.8, SQLite via sqlx 0.9
- **Frontend:** Askama templates, HTMX, vanilla CSS
- **Deploy:** Fly.io single instance with persistent volume
- **CI:** GitHub Actions (clippy + nextest on push)

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

### Admin

Requires `Authorization: Bearer <ADMIN_TOKEN>`.

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/admin/models` | Create model with optional HF link |
| `PUT` | `/api/admin/models/{id}` | Update a model's HF link |
| `DELETE` | `/api/admin/models/{id}` | Delete a model and its votes |
| `DELETE` | `/api/admin/deliveries/{id}` | Delete an abliterated model delivery |
| `DELETE` | `/api/admin/prune` | Delete all zero-vote models |
| `POST` | `/api/admin/deliver` | Record an abliterated model delivery |

### Admin UI

Available at `/admin/login`. Enter the `ADMIN_TOKEN` to access the dashboard.

| Path | Description |
|------|-------------|
| `/admin/login` | Login form (public) |
| `/admin` | Dashboard — manage models and deliveries (requires auth) |

The admin dashboard provides:
- **Stats overview** — model count, total votes, deliveries
- **Model management** — view leaderboard, delete models, prune zero-vote models
- **Delivery management** — record new deliveries, view/delete existing ones
- **Auto-refresh** — model list refreshes every 10s, deliveries every 30s

Auth uses `localStorage`: the token is stored client-side after login and sent as
a `Bearer` header on every subsequent request.

### Rate Limits

- **Voting:** 3 votes per IP per hour, 1 vote per model per UUID
- **Model submission:** 1 per IP per day
- **Board cap:** 50 models max

## Makefile

```bash
make help            # list all targets
make run             # run locally (ADMIN_TOKEN=dev-secret)
make test            # run all tests
make lint            # clippy
make ci              # lint + test
make sqlx-prepare    # regenerate offline query cache
make docker-run      # build + run in Docker
make deploy          # fly deploy
make clean           # remove artifacts
```

## Tests

```bash
make test
```

88 tests covering queries, extractors, HF validation, DB migrations, all API endpoints (including admin delivery deletion), rate limiting, auth, pagination, and edge cases.

See [DEVELOPMENT.md](DEVELOPMENT.md) for test architecture, security hardening details, and development workflow.
