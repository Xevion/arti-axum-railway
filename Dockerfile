# syntax=docker/dockerfile:1
FROM rust:1.86-bookworm AS base
WORKDIR /app

RUN apt-get update && apt-get install -y protobuf-compiler libssl-dev pkg-config
RUN cargo install cargo-chef

FROM base AS planner
COPY . .
ENV CARGO_TARGET_DIR=/app/target-railway
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
RUN cargo install arti --features=full
COPY --from=planner /app/recipe.json recipe.json
ENV CARGO_TARGET_DIR=/app/target-railway
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS final

RUN apt-get update && apt-get install -y libsqlite3-0 openssl && rm -rf /var/lib/apt/lists/*

RUN addgroup --system app && adduser --system --ingroup app app
RUN mkdir -p /etc/arti /home/app/.local/share/arti
COPY onionservice.toml /etc/arti/
RUN chown -R app:app /etc/arti /home/app/.local/share/arti

USER app
WORKDIR /app

COPY --from=builder /app/target-railway/release/arti-axum-railway .
COPY --from=builder /usr/local/cargo/bin/arti .

ARG PORT=
ENV PORT=${PORT}
EXPOSE ${PORT}

CMD ["./arti-axum-railway", "--config", "/etc/arti/onionservice.toml"]