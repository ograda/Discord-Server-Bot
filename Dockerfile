FROM rust:1.98-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 bot && useradd --uid 10001 --gid bot --no-create-home bot
WORKDIR /app
COPY --from=builder /app/target/release/discord-server-bot /usr/local/bin/discord-server-bot
USER 10001:10001
ENV CONFIG_DIR=/app/config
STOPSIGNAL SIGTERM
ENTRYPOINT ["discord-server-bot"]
