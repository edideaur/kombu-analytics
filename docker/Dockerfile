FROM node:22-slim AS webui
WORKDIR /app
COPY umami/package.json umami/pnpm-lock.yaml ./umami/
COPY webui/package.json ./webui/
RUN corepack enable && pnpm --filter webui install --frozen-lockfile || npm --prefix webui install
COPY umami ./umami
COPY webui ./webui
RUN pnpm --filter webui build || npm --prefix webui run build

FROM rust:1.85-slim AS builder
WORKDIR /app
COPY Cargo.toml rust-toolchain.toml deny.toml kani.toml ./
COPY crates ./crates
COPY --from=webui /app/webui/dist ./webui/dist
RUN cargo build --release --bin kombu

FROM debian:stable-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/kombu /usr/local/bin/kombu
COPY --from=webui /app/webui/dist /app/webui/dist
COPY --from=webui /app/umami/public /app/umami/public
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
EXPOSE 3000
HEALTHCHECK --interval=5s --timeout=5s --retries=5 CMD curl -f http://localhost:3000/api/health || exit 1
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
