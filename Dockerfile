# openpost — multi-stage build for a small production image.

# --- Stage 1: build the release binary ---
FROM rust:1-slim-bookworm AS builder

WORKDIR /build

# Cache dependencies: build a dummy crate first so `cargo build` only
# re-runs when Cargo.toml/Cargo.lock change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src target/release/deps/openpost* target/release/openpost

# Real sources (migrations are embedded at compile time by sqlx::migrate!).
COPY src ./src
COPY migrations ./migrations

# `touch` busts cargo's mtime-based freshness check so the real sources are
# recompiled (the dummy build's fingerprint would otherwise look fresh).
RUN touch src/main.rs && cargo build --release

# --- Stage 2: minimal runtime image ---
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --shell /usr/sbin/nologin openpost

COPY --from=builder /build/target/release/openpost /usr/local/bin/openpost
COPY src/web/templates /app/templates
COPY static /app/static

ENV TEMPLATE_DIR=/app/templates \
    STATIC_DIR=/app/static \
    BIND_ADDR=0.0.0.0:3000 \
    DATABASE_URL=postgres://openpost:openpost@db:5432/openpost?sslmode=disable \
    BASE_URL=http://localhost:3000

USER openpost
EXPOSE 3000

ENTRYPOINT ["openpost"]
