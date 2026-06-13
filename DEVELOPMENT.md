# Development Guide

## Prerequisites

- **Rust 1.85+** (edition 2024) -- check with `rustc --version`
- **cargo-nextest** -- `cargo install cargo-nextest`
- **sqlx-cli** -- `cargo install sqlx-cli --features sqlite`
- **SQLite 3** -- for manual DB inspection and cache generation
- **GNU Make** -- for running common tasks

## Makefile

Run `make help` to see all targets:

```
build              Build the project
check              Type-check without building
ci                 Run lint + test (CI pipeline)
clean              Remove build artifacts and scratch DB
deploy             Deploy to Fly.io
docker-build       Build the Docker image locally
docker-run         Build and run in Docker
help               Show this help
lint               Run clippy on all targets
run                Run the server locally
sqlx-prepare       Regenerate the sqlx offline query cache
test               Run all tests
```

## Building

```bash
make build
```

The project uses `SQLX_OFFLINE=true` (set in `.cargo/config.toml`) so all `sqlx::query!()` macros resolve against the cached `.sqlx/` directory. No running database is needed to compile.

## Running Locally

```bash
make run                           # uses ADMIN_TOKEN=dev-secret
ADMIN_TOKEN=my-token make run      # custom token
```

`ADMIN_TOKEN` is required -- the server refuses to start without it. This creates `leaderboard.db` in the current directory and runs migrations automatically. Open `http://localhost:8080`.

## Database

### Schema

Three tables defined in `migrations/20260613111122_create_schema.sql`:

- **`models`** -- `model_id` (PK), `vote_count`, `hf_link`, `created_by_ip`, `created_at`
- **`votes`** -- `(client_uuid, model_id)` composite PK, `ip_address`, `voted_at`, FK to models with `ON DELETE CASCADE`
- **`deliveries`** -- auto-increment `id`, `model_id`, `vote_count`, `hf_link`, `notes`, `delivered_at`

The second migration (`20260613120000_add_votes_fk.sql`) adds the foreign key constraint to `votes`.

### Migrations

Migrations are embedded at compile time via `sqlx::migrate!("./migrations")` and run on startup in `src/db.rs`. The `_sqlx_migrations` tracking table ensures each migration only runs once.

To add a new migration:

```bash
# 1. Create the migration file
sqlx migrate add <description>

# 2. Write your SQL in the new file under migrations/

# 3. Regenerate the offline cache (picks up all migrations automatically)
make sqlx-prepare
```

### Offline Query Cache

The `.sqlx/` directory contains JSON files with pre-computed query metadata. This allows `cargo build` to verify SQL queries at compile time without a database connection.

**Important distinction:**

- **`.sqlx/build.db`** -- a throwaway scratch database used *only* for cache generation. Contains no real data. Deleted and rebuilt every time because it's simpler than tracking which migrations have been applied to it.
- **`leaderboard.db`** (or `/data/leaderboard.db` in production) -- your real database. **Never deleted.** Migrations are incremental: `sqlx::migrate!()` checks the `_sqlx_migrations` table on startup and only runs new ones.

**When to regenerate the cache:**

- After adding, modifying, or removing any `sqlx::query!()` / `query_as!()` / `query_scalar!()` call
- After changing the database schema (new migration)

**How to regenerate:**

```bash
make sqlx-prepare
```

This does four things automatically:
1. Deletes the scratch `.sqlx/build.db`
2. Applies every migration in `migrations/` to a fresh scratch DB
3. Runs `cargo sqlx prepare` to generate the JSON cache files
4. Runs `cargo check` to verify compilation

Commit the updated `.sqlx/*.json` files afterward.

### Query Patterns

All queries are in `src/queries.rs`. Key patterns:

- **`query_as!(RowType, "SQL")`** -- for SELECT queries returning rows. Use `"col!"` nullability overrides in raw strings for NOT NULL columns (SQLite quirk: all TEXT columns resolve as `Option<String>`).
- **`query_scalar!("SQL")`** -- for COUNT or existence checks returning a single value.
- **`query!("SQL")`** -- for INSERT/UPDATE/DELETE.
- All functions take `impl sqlx::Executor<'_, Database = Sqlite>` so they work with both `&SqlitePool` and `&mut SqliteConnection` (transactions).
- Exception: `delete_model` and `prune_zero_vote_models` take `&SqlitePool` directly since they rely on `ON DELETE CASCADE` (single query, no transaction needed).

## Testing

```bash
make test                                        # all tests
cargo nextest run -E "test(admin_delete)"        # specific test
cargo nextest run -E "binary(votablit)"            # unit tests only
cargo nextest run -E "binary(integration)"       # integration tests only
```

### Test Architecture

- **Unit tests** (`#[sqlx::test]`) -- in `src/queries.rs`, `src/db.rs`, `src/hf.rs`, `src/extractors.rs`. Each test receives a fresh in-memory SQLite pool with migrations already applied.
- **Integration tests** (`tests/integration.rs`) -- full route tests using `#[sqlx::test]` + `MockConnectInfo`. Each test builds a complete `Router` from a fresh pool.

Key details:

- `#[sqlx::test]` auto-creates an isolated in-memory DB per test and runs all migrations. No `DATABASE_URL` needed.
- `MockConnectInfo(SocketAddr)` provides `ConnectInfo<SocketAddr>` to handlers without a real TCP connection.
- `X-Forwarded-For` headers in tests simulate different client IPs for rate-limit testing.
- `ADMIN_TOKEN` is injected via `AppState::new(pool, "test-secret-token".into())`, not environment variables.
- `nextest` runs each test in its own process, ensuring full isolation.

### Adding a New Endpoint

1. Add the query function to `src/queries.rs` with `#[sqlx::test]` unit tests
2. Add the handler to `src/routes/api.rs` (or `pages.rs` for HTML)
3. Register the route in `src/lib.rs`
4. Add integration tests in `tests/integration.rs`
5. Regenerate the sqlx cache: `make sqlx-prepare`
6. Run `make ci` (lint + test)

## Security

### Hardening Measures

- **Constant-time token comparison** -- `subtle::ConstantTimeEq` prevents timing attacks on admin token (`src/extractors.rs`)
- **No CORS** -- same-origin only; wildcard CORS removed entirely
- **Trusted IP extraction** -- prefers `Fly-Client-IP` (set by Fly.io edge, not forgeable); falls back to *last* entry in `X-Forwarded-For` (appended by the trusted proxy, not the client-controlled first entry)
- **HTMX SRI** -- subresource integrity hash on the CDN script tag
- **Input validation** -- model IDs restricted to `[a-zA-Z0-9\-_./]`; client UUIDs capped at 64 chars; HF links validated as HTTPS URLs
- **Structured logging** -- `tracing` structured fields prevent log injection via user-controlled input
- **Fail-fast startup** -- server panics if `ADMIN_TOKEN` is unset or empty
- **Non-root Docker** -- runtime container uses a dedicated `votablit` user
- **Foreign keys** -- `ON DELETE CASCADE` enforced via `PRAGMA foreign_keys = ON`

### Known Limitations

- **No CSRF protection** -- the app uses no cookies/sessions, so CSRF is limited to consuming the victim's IP rate limit quota. The `client_uuid` is in `localStorage` (origin-scoped), not accessible cross-origin.
- **`client_uuid` is client-controlled** -- a user can change it in DevTools to bypass per-UUID dedup. IP rate limiting (3/hour) is the real defense.
- **Rate limits are not atomic** -- concurrent requests within a single SQLite transaction can read stale state. SQLite's write serialization limits practical exploitation, but a burst of simultaneous requests could exceed the 3/hour limit by 1-2 votes.

## Architecture

### Request Flow

```
Client -> Axum Router -> Handler -> queries.rs -> SQLite
                           |
                      extractors.rs (AdminAuth, IP extraction, validation)
                           |
                      error.rs (AppError -> HTTP response)
```

### Key Design Decisions

- **`AppState`** holds `SqlitePool`, `reqwest::Client`, and `admin_token`. Cheap to clone (internally Arc-wrapped).
- **`AdminAuth`** extractor reads the token from `AppState`, not from env vars. Uses constant-time comparison.
- **`AppError`** enum maps to HTTP status codes via `IntoResponse`. Handlers return `Result<T, AppError>`.
- **Foreign keys** with `ON DELETE CASCADE` on `votes.model_id`. Enabled via `SqliteConnectOptions::foreign_keys(true)`.
- **No user auth** -- rate limiting via client UUID (localStorage) + IP address.
- **Paginated deliveries** -- `GET /api/deliveries?page=1&per_page=10` with total count. HTMX partial has prev/next buttons.

### HuggingFace Validation

`src/hf.rs` validates HF links by:

1. Parsing the URL to extract `org/model` (supports `huggingface.co` and `hf.co`)
2. Hitting the HuggingFace `/api/models/{id}/tree/main` endpoint
3. Summing file sizes and rejecting models over 70 GB

The `reqwest::Client` is shared via `AppState` (connection pooling, 15s timeout).

## Deployment

### Fly.io

```bash
fly apps create votablit
fly volumes create leaderboard_data --region sjc --size 1
fly secrets set ADMIN_TOKEN="production-secret"
make deploy
```

The `Dockerfile` is a multi-stage build:
1. **Builder** (`rust:1.86-slim-bookworm`) -- compiles the binary with `SQLX_OFFLINE=true`
2. **Runtime** (`debian:bookworm-slim`) -- copies the binary + `static/` directory

The database is stored on a persistent Fly volume at `/data/leaderboard.db`. Migrations run automatically on startup.

### Docker (local)

```bash
ADMIN_TOKEN=my-token make docker-run
```

## CI

GitHub Actions runs on every push and PR to `main`:

1. Clippy with `-D warnings` (zero warnings policy)
2. `cargo nextest run` (all 81 tests)

See `.github/workflows/ci.yml`.

## Linting

```bash
make lint
```

Always run before committing. Zero warnings policy. Use `make ci` to run both lint and test in one command.
