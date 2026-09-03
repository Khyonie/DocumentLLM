FROM rust:1.97-trixie AS chef
RUN cargo install cargo-chef --locked
RUN apt-get update \
    && apt-get install --yes --no-install-recommends libprotobuf-dev libssl-dev pkg-config protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app

FROM node:24-trixie-slim AS frontend
WORKDIR /app/docllm-frontend
COPY docllm-frontend/package.json docllm-frontend/package-lock.json ./
RUN npm ci
COPY docllm-frontend ./
RUN npm run build

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY docllm-backend ./docllm-backend
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --workspace --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY docllm-backend ./docllm-backend
RUN cargo build --release --package documentllm-server

FROM debian:trixie-slim AS runtime
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/documentllm-server /usr/local/bin/
COPY --from=frontend /app/docllm-frontend/dist ./docllm-frontend/dist
ENV DOCUMENTLLM_BIND_ADDRESS=0.0.0.0:3001 \
    DOCUMENTLLM_DATABASE_PATH=/data/database.lancedb \
    DOCUMENTLLM_EMBEDDING_CACHE_PATH=/cache/fastembed
EXPOSE 3001
VOLUME ["/data", "/app/upload", "/cache/fastembed"]
CMD ["documentllm-server"]
