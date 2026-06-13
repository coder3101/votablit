FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release -p votablit

FROM debian:trixie-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash votablit
USER votablit
WORKDIR /home/votablit

COPY --from=builder /app/target/release/votablit .
COPY static ./static

ENV DATABASE_PATH=/data/leaderboard.db
ENV BIND_ADDR=0.0.0.0:8080

EXPOSE 8080
CMD ["./votablit"]
