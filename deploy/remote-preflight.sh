#!/usr/bin/env bash
# 仅做只读预检；不会停止服务、修改 Docker 或写入防火墙。
set -Eeuo pipefail

node_id=${1,,}
case "$node_id" in c001|c091|c101) ;; *) echo "拒绝未知节点：$node_id" >&2; exit 2 ;; esac
run_id=${2:?缺少运行编号}
[[ "$run_id" =~ ^[A-Za-z0-9][A-Za-z0-9_.-]{2,63}$ ]] || { echo '运行编号格式错误' >&2; exit 2; }
out_dir="/var/tmp/opsd-preflight-${run_id}-${node_id}"
mkdir -p "$out_dir"
exec > >(tee "$out_dir/preflight.log") 2>&1

fail=0
record() {
  local name=$1; shift
  echo "--- ${name} ---"
  if "$@"; then echo "结果：PASS"; else echo "结果：FAIL（退出码=$?）"; fail=1; fi
}

echo "节点：$node_id"
echo "主机名：$(hostname)"
echo "时间：$(date -Is)"
uname -a || true
cat /etc/os-release || true
printf '架构：'; uname -m || true

record docker-version docker version
record docker-info docker info
if docker compose version >/dev/null 2>&1; then docker compose version; else record docker-compose-version docker-compose version; fi
record listening-sockets ss -lntup
record disk-space df -h / /opt 2>/dev/null || df -h /
record docker-containers docker ps --no-trunc --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}'
echo '--- docker-mounts ---'
if ids=$(docker ps -q); then
  if [[ -n "$ids" ]]; then docker inspect $ids --format '{{.Id}} {{range .Mounts}}{{.Type}}={{.Source}}:{{.Destination}} {{end}}'; fi
  echo '结果：PASS'
else
  echo '结果：FAIL（无法读取容器挂载）'; fail=1
fi

systemctl list-units --all --type=service --no-legend 2>/dev/null | awk '$1 ~ /^1panel([-.].*)?\.service$/ {print $1}' | sort -u > "$out_dir/onepanel.units" || :
: > "$out_dir/onepanel.active"
while IFS= read -r unit; do
  [[ -n "$unit" ]] || continue
  if systemctl is-active --quiet "$unit"; then echo "$unit" | tee -a "$out_dir/onepanel.active"; fi
done < "$out_dir/onepanel.units"
echo "已识别的 1Panel 单元：$(paste -sd, "$out_dir/onepanel.units" || true)"
echo "当前活动的 1Panel 单元：$(paste -sd, "$out_dir/onepanel.active" || true)"

if [[ -e /opt/opsd-test/data/control.db || -e /opt/opsd-test/compose.yml || -e /var/lib/opsd-agent/config.json ]]; then
  echo '发现已有 opsd 测试数据或部署文件，拒绝自动继续。' >&2
  fail=1
fi
if [[ "$node_id" == c091 ]]; then
  for port in 65535 8444; do
    listeners=$(ss -ltnp "sport = :$port" | tail -n +2 || true)
    if [[ -n "$listeners" ]]; then
      if grep -q '"1panel"' <<<"$listeners"; then
        echo "端口 $port 当前由已识别的 1Panel 进程占用；停止该服务后重新确认"
      else
        echo "端口 $port 被非 1Panel 进程占用：$listeners" >&2; fail=1
      fi
    else
      echo "端口 $port 可用"
    fi
  done
fi

if systemctl list-unit-files opsd-agent.service >/dev/null 2>&1 && systemctl is-active --quiet opsd-agent.service; then
  echo '已有活动的 opsd-agent.service，拒绝覆盖。' >&2
  fail=1
fi

echo "预检日志：$out_dir/preflight.log"
exit "$fail"
