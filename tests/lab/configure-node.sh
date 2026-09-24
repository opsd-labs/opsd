#!/usr/bin/env bash
# 在专用 WSL 发行版配置独立 Docker Engine 和本次运行的资源前缀。
set -euo pipefail

NODE_ID="${1:?用法：configure-node.sh opsd_节点名 hub|agent [opsd-test-运行号-]}"
ROLE="${2:?必须指定 hub 或 agent}"
TEST_PREFIX="${3:-opsd-test-local-}"

[[ "$NODE_ID" =~ ^opsd_[a-z0-9_]+$ ]] || { echo "拒绝非 opsd_ 节点标识：$NODE_ID" >&2; exit 2; }
[[ "$ROLE" == hub || "$ROLE" == agent ]] || { echo "角色只能是 hub 或 agent" >&2; exit 2; }
[[ "$TEST_PREFIX" =~ ^opsd-test-[a-zA-Z0-9-]+-$ ]] || { echo "测试资源前缀格式无效：$TEST_PREFIX" >&2; exit 2; }
[[ "$(id -u)" -eq 0 ]] || { echo '必须以 root 运行。' >&2; exit 2; }

if ! command -v dockerd >/dev/null 2>&1; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq docker.io procps >/dev/null
fi

mkdir -p /var/run
if command -v systemctl >/dev/null 2>&1 && systemctl start docker.service 2>/dev/null; then
  :
elif command -v service >/dev/null 2>&1 && service docker start 2>/dev/null; then
  :
elif ! pgrep -x dockerd >/dev/null 2>&1; then
  nohup dockerd --host=unix:///var/run/docker.sock >/var/log/opsd-lab-dockerd.log 2>&1 &
fi

ready=0
for _ in $(seq 1 30); do
  if docker info >/dev/null 2>&1; then ready=1; break; fi
  sleep 1
done
[[ "$ready" -eq 1 ]] || { tail -n 60 /var/log/opsd-lab-dockerd.log 2>/dev/null || true; echo 'Docker Engine 未就绪。' >&2; exit 1; }

install -d -m 0755 /var/lib/opsd-lab
printf '%s\n' "$NODE_ID" > /var/lib/opsd-lab/node_id
printf '%s\n' "$TEST_PREFIX" > /var/lib/opsd-lab/test_prefix
printf '%s\n' "$ROLE" > /var/lib/opsd-lab/role
docker version --format 'Docker Engine {{.Server.Version}}'
echo "节点已配置：${NODE_ID}（${ROLE}），测试资源前缀 ${TEST_PREFIX}"
