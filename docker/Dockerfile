FROM node:22-slim AS webui
WORKDIR /app/webui
COPY webui/package.json webui/package-lock.json ./
RUN npm ci || npm install
COPY webui ./
RUN npm run build

FROM rust:1.85-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml deny.toml kani.toml ./
COPY crates ./crates
COPY --from=webui /app/webui/dist ./webui/dist
COPY --from=webui /app/webui/public ./webui/public
RUN cargo build --release --bin kombu

FROM debian:stable-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/kombu /usr/local/bin/kombu
COPY --from=webui /app/webui/dist /app/webui/dist
COPY --from=webui /app/webui/public /app/webui/public
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
EXPOSE 3000
HEALTHCHECK --interval=5s --timeout=5s --retries=5 CMD curl -f http://localhost:3000/api/health || exit 1
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
