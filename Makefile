.PHONY: build check run test lint sqlx-prepare docker-build deploy clean help

# Default admin token for local development
ADMIN_TOKEN ?= dev-secret

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

build: ## Build the project
	cargo build

check: ## Type-check without building
	cargo check

run: ## Run the server locally
	ADMIN_TOKEN=$(ADMIN_TOKEN) cargo run

test: ## Run all tests
	cargo nextest run

lint: ## Run clippy on all targets
	cargo clippy --all-targets

ci: lint test ## Run lint + test (CI pipeline)

sqlx-prepare: ## Regenerate the sqlx offline query cache
	@echo "Rebuilding scratch database from migrations..."
	@rm -f .sqlx/build.db
	@for f in migrations/*.sql; do \
		echo "  applying $$f"; \
		sqlite3 .sqlx/build.db < "$$f"; \
	done
	@echo "Generating query cache..."
	SQLX_OFFLINE=false DATABASE_URL="sqlite:$$(pwd)/.sqlx/build.db" cargo sqlx prepare
	@echo "Verifying compilation..."
	cargo check
	@echo "Done. Commit the updated .sqlx/*.json files."

docker-build: ## Build the Docker image locally
	docker build -t votablit .

docker-run: docker-build ## Build and run in Docker
	docker run --rm -p 8080:8080 \
		-e ADMIN_TOKEN=$(ADMIN_TOKEN) \
		-v votablit_data:/data \
		votablit

deploy: ## Deploy to Fly.io
	fly deploy

clean: ## Remove build artifacts and scratch DB
	cargo clean
	rm -f .sqlx/build.db
	rm -f leaderboard.db leaderboard.db-shm leaderboard.db-wal
