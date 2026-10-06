#!/usr/bin/env bash
# 在 C091 的独立目录拉取并启动 Hub，不触碰 Docker socket 和业务项目。
set -Eeuo pipefail
root=${1:-/opt/opsd-test}
image=${2:?缺少镜像地址}
[[ "$root" == /opt/opsd-test ]] || { echo '只允许使用 /opt/opsd-test' >&2; exit 2; }
[[ "$image" =~ ^ghcr\.io/opsd-labs/opsd:[A-Za-z0-9_.-]+$ ]] || { echo '镜像地址不在允许范围' >&2; exit 2; }
[[ -f "$root/compose.yml" && -f "$root/deployment.env" ]] || { echo '缺少 Compose 或部署环境文件' >&2; exit 1; }
[[ -f "$root/secrets/admin-password" ]] || { echo '缺少管理员密码文件' >&2; exit 1; }
chmod 700 "$root" "$root/data" "$root/backups" "$root/secrets"
chmod 600 "$root/secrets/admin-password"
cd "$root"
for port in "${OPSD_CONSOLE_PORT:-65535}" "${OPSD_AGENT_PORT:-8444}"; do
  if ss -ltn "sport = :$port" | tail -n +2 | grep -q .; then echo "端口 $port 在停止 1Panel 后仍被占用" >&2; exit 1; fi
done
export OPSD_IMAGE="$image"
docker pull "$image"
digest=$(docker image inspect "$image" --format '{{index .RepoDigests 0}}')
printf '%s\n' "$digest" | tee "$root/image-digest.txt"
if [[ ! -f "$root/data/control.db" ]]; then
  docker compose --env-file "$root/deployment.env" -f "$root/compose.yml" run --rm -T -v "$root/secrets:/run/secrets:ro" hub init --password-file /run/secrets/admin-password --names 'C091,C001,C101' | tee "$root/hub-init.txt"
fi
docker compose --env-file "$root/deployment.env" -f "$root/compose.yml" up -d
docker compose --env-file "$root/deployment.env" -f "$root/compose.yml" ps
container=$(docker compose --env-file "$root/deployment.env" -f "$root/compose.yml" ps -q hub)
[[ -n "$container" ]] || { echo 'Hub 容器未创建' >&2; exit 1; }
for _ in {1..30}; do
  status=$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}no-healthcheck{{end}}' "$container")
  [[ "$status" == healthy ]] && break
  sleep 2
done
[[ "$status" == healthy ]] || { docker logs --tail 80 "$container" >&2; exit 1; }
if docker inspect "$container" --format '{{range .Mounts}}{{println .Source .Destination}}{{end}}' | grep -Fq '/var/run/docker.sock'; then
  echo 'Hub 容器不应挂载 Docker socket' >&2; exit 1
fi
echo "Hub 已启动，容器=$container，摘要=$digest"
