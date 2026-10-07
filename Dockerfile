ARG NODE_VERSION=24
ARG RUST_VERSION=1.95

FROM node:${NODE_VERSION}-bookworm-slim AS web
WORKDIR /source/web
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run build
RUN npx vite build --config vite.share.config.ts

FROM rust:${RUST_VERSION}-bookworm AS rust
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --locked --release --bins

FROM debian:bookworm-slim AS hub
ARG VCS_REF=unknown
ARG VERSION=0.1.0
ARG BUILD_DATE=unknown
LABEL org.opencontainers.image.title="opsd" \
      org.opencontainers.image.description="opsd Hub 管理系统" \
      org.opencontainers.image.source="https://github.com/opsd-labs/opsd" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=rust /source/target/release/opsd-hub /usr/local/bin/opsd-hub
COPY --from=web /source/web/dist /opt/opsd/web
ENV OPSD_DATA_DIR=/var/lib/opsd
EXPOSE 65535 8444 65534
ENTRYPOINT ["/usr/local/bin/opsd-hub"]
CMD ["serve", "--listen", "0.0.0.0:65535", "--agent-listen", "0.0.0.0:8444", "--health-listen", "127.0.0.1:65534", "--web", "/opt/opsd/web"]

FROM debian:bookworm-slim AS agent
ARG VCS_REF=unknown
ARG VERSION=0.1.0
ARG BUILD_DATE=unknown
LABEL org.opencontainers.image.title="opsd-agent" \
      org.opencontainers.image.description="opsd Agent Docker 运行镜像" \
      org.opencontainers.image.source="https://github.com/opsd-labs/opsd" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}"
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates docker.io docker-compose iproute2 iputils-ping nftables iptables ufw firewalld procps \
    && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /usr/local/lib/docker/cli-plugins && printf '#!/bin/sh\nexec /usr/bin/docker-compose "\$@"\n' > /usr/local/lib/docker/cli-plugins/docker-compose && chmod 0755 /usr/local/lib/docker/cli-plugins/docker-compose
COPY --from=rust /source/target/release/opsd-agent /usr/local/bin/opsd-agent
COPY deploy/opsd-agent-container /usr/local/bin/opsd-agent-container
RUN chmod 0755 /usr/local/bin/opsd-agent-container
ENV OPSD_AGENT_MODE=container OPSD_AGENT_DIR=/var/lib/opsd-agent
ENTRYPOINT ["/usr/local/bin/opsd-agent-container"]
CMD ["run"]

FROM hub AS final
