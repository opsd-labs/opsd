FROM node:24-bookworm-slim AS web
WORKDIR /source/web
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.95-bookworm AS rust
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --locked --release --bin opsd-hub

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=rust /source/target/release/opsd-hub /usr/local/bin/opsd-hub
COPY --from=web /source/web/dist /opt/opsd/web
ENV OPSD_DATA_DIR=/var/lib/opsd
# 65535 为控制台端口，8444 为 Agent 端到端 mTLS 通道，65534 为仅回环的健康检查端口。
EXPOSE 65535 8444
ENTRYPOINT ["/usr/local/bin/opsd-hub"]
CMD ["serve", "--listen", "0.0.0.0:65535", "--agent-listen", "0.0.0.0:8444", "--health-listen", "127.0.0.1:65534", "--web", "/opt/opsd/web"]
