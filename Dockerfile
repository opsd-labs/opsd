ARG NODE_VERSION=24
FROM node:${NODE_VERSION}-bookworm-slim AS web
WORKDIR /source/web
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

ARG RUST_VERSION=1.95
FROM rust:${RUST_VERSION}-bookworm AS rust
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --locked --release --bin opsd-hub

FROM debian:bookworm-slim
ARG VCS_REF=unknown
ARG VERSION=0.1.0
ARG BUILD_DATE=unknown
LABEL org.opencontainers.image.title="opsd" \
      org.opencontainers.image.description="opsd Hub 与 Agent 管理系统" \
      org.opencontainers.image.source="https://github.com/opsd-labs/opsd" \
      org.opencontainers.image.revision="$VCS_REF" \
      org.opencontainers.image.version="$VERSION" \
      org.opencontainers.image.created="$BUILD_DATE"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=rust /source/target/release/opsd-hub /usr/local/bin/opsd-hub
COPY --from=web /source/web/dist /opt/opsd/web
ENV OPSD_DATA_DIR=/var/lib/opsd
# 65535 为控制台端口，8444 为 Agent 端到端 mTLS 通道，65534 为仅回环的健康检查端口。
EXPOSE 65535 8444
ENTRYPOINT ["/usr/local/bin/opsd-hub"]
CMD ["serve", "--listen", "0.0.0.0:65535", "--agent-listen", "0.0.0.0:8444", "--health-listen", "127.0.0.1:65534", "--web", "/opt/opsd/web"]
