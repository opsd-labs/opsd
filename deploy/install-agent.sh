#!/usr/bin/env bash
# 专属接入参数由 Hub 填入，原始模板不能直接执行。
set +x
set +a
set -Eeuo pipefail
umask 077
unset token ca_pem
__OPSD_INSTALL_CONFIG__

stage='检查安装环境'
data_dir=/var/lib/opsd-agent
work_dir=
diagnostics() {
  if [[ "$mode" == host ]]; then
    printf '%s\n' '诊断：systemctl status opsd-agent.service' '日志：journalctl -u opsd-agent.service -n 50 --no-pager' >&2
  else
    printf '%s\n' '诊断：docker inspect opsd-agent' '日志：docker logs --tail 50 opsd-agent' >&2
  fi
}
fail() { printf '安装失败：%s\n' "$1" >&2; diagnostics; exit 1; }
trap 'code=$?; printf "安装失败，所在步骤：%s\n" "$stage" >&2; diagnostics; exit "$code"' ERR
trap 'if [[ -n "$work_dir" ]]; then rm -rf -- "$work_dir"; fi' EXIT

[[ $EUID == 0 ]] || fail '请在 root 的 Bash 终端中执行安装命令。'
[[ $(uname -s) == Linux ]] || fail '当前仅支持 Linux。'
case "$(uname -m)" in
  x86_64|amd64) arch=amd64 ;;
  aarch64|arm64) arch=arm64 ;;
  *) fail '当前架构不受支持，仅支持 x64 和 ARM64。' ;;
esac
[[ -r /etc/os-release ]] || fail '无法识别发行版。'
. /etc/os-release
case " ${ID:-} ${ID_LIKE:-} " in
  *' debian '*|*' ubuntu '*) package_manager=apt ;;
  *' rhel '*|*' fedora '*|*' centos '*|*' rocky '*|*' almalinux '*)
    if command -v dnf >/dev/null 2>&1; then package_manager=dnf; else package_manager=yum; fi ;;
  *) fail '当前仅支持 Debian、Ubuntu 与 RHEL 系发行版。' ;;
esac
if [[ "$mode" == host ]]; then
  [[ -d /run/systemd/system ]] && command -v systemctl >/dev/null 2>&1 || fail '宿主机安装需要正在运行的 systemd。'
else
  command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1 || fail 'Docker 不可用，请先安装并启动 Docker。'
fi

stage='准备基础依赖'
packages=()
command -v curl >/dev/null 2>&1 || packages+=(curl)
command -v jq >/dev/null 2>&1 || packages+=(jq)
command -v tar >/dev/null 2>&1 || packages+=(tar)
if [[ "$mode" == host ]]; then
  command -v openssl >/dev/null 2>&1 || packages+=(openssl)
  command -v lsblk >/dev/null 2>&1 || packages+=(util-linux)
  if ! command -v ip >/dev/null 2>&1; then
    if [[ "$package_manager" == apt ]]; then packages+=(iproute2); else packages+=(iproute); fi
  fi
  command -v nft >/dev/null 2>&1 || packages+=(nftables)
  command -v iptables-restore >/dev/null 2>&1 || packages+=(iptables)
fi
if [[ ! -s /etc/ssl/certs/ca-certificates.crt && ! -s /etc/pki/tls/certs/ca-bundle.crt ]]; then packages+=(ca-certificates); fi
if (( ${#packages[@]} )); then
  printf '%s\n' '正在安装必要的基础依赖……'
  if [[ "$package_manager" == apt ]]; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${packages[@]}"
  else
    "$package_manager" install -y "${packages[@]}"
  fi
fi

identity_exists=false
check_identity() {
  jq -e --arg node "$node_id" --arg hub "$hub" --arg url "$agent_url" \
    '.node_id == $node and .hub == $hub and .agent_url == $url' "$data_dir/config.json" >/dev/null \
    || fail '已有 Agent 身份与本次节点或 Hub 不一致，已停止，未覆盖身份。'
  [[ -s "$data_dir/client.pem" && -s "$data_dir/client-key.pem" && -s "$data_dir/ca.pem" ]] \
    || fail '已有身份文件不完整，请回控制台核实注册状态，不能自动重新登记。'
}
if [[ -e "$data_dir/config.json" ]]; then
  check_identity
  identity_exists=true
else
  [[ ! -e "$data_dir/client.pem" && ! -e "$data_dir/client-key.pem" && ! -e "$data_dir/ca.pem" ]] \
    || fail '发现未完成保存的节点身份，请回控制台核实注册状态。'
  (( $(date +%s) < expires_at )) || fail '一次性令牌已过期，请回控制台重新生成安装信息。'
fi

stage='核对现有服务'
if [[ "$mode" == host ]]; then
  if command -v docker >/dev/null 2>&1 && docker container inspect opsd-agent >/dev/null 2>&1; then
    fail '已有同名 Docker Agent，请使用原安装方式恢复，不能切换模式覆盖。'
  fi
  if [[ "$identity_exists" == false ]] && systemctl cat opsd-agent.service >/dev/null 2>&1; then
    fail '已有 Agent 服务但没有匹配身份，已停止，未覆盖服务。'
  fi
else
  if command -v systemctl >/dev/null 2>&1 && systemctl cat opsd-agent.service >/dev/null 2>&1; then
    fail '已有宿主机 Agent 服务，不能切换为 Docker 模式覆盖。'
  fi
  if docker container inspect opsd-agent >/dev/null 2>&1; then
    [[ "$identity_exists" == true ]] || fail '同名容器已存在且没有匹配身份，已停止。'
    [[ $(docker inspect opsd-agent --format '{{index .Config.Labels "io.opsd.node-id"}}') == "$node_id" \
      && $(docker inspect opsd-agent --format '{{index .Config.Labels "io.opsd.hub"}}') == "$hub" ]] \
      || fail '同名容器不属于本次安装，已停止，未修改容器。'
  fi
fi

work_dir=$(mktemp -d)
if [[ "$mode" == host ]]; then
  if [[ "$identity_exists" == false || ! -x /usr/local/bin/opsd-agent ]]; then
    stage='下载固定版本 Agent'
    printf '正在下载 %s 的 %s 安装包……\n' "$release" "$arch"
    curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
      --connect-timeout 10 --max-time 300 \
      "https://github.com/opsd-labs/opsd/releases/download/$release/opsd-agent-linux-$arch.tar.gz" \
      --output "$work_dir/agent.tar.gz"
    tar -xzf "$work_dir/agent.tar.gz" -C "$work_dir"
    install -m 755 "$work_dir/opsd-agent" /usr/local/bin/opsd-agent
  fi
elif ! docker container inspect opsd-agent >/dev/null 2>&1; then
  stage='拉取固定版本 Docker 镜像'
  docker pull "$agent_image"
fi

if [[ "$identity_exists" == false ]]; then
  stage='注册节点'
  (( $(date +%s) < expires_at )) || fail '下载完成时令牌已过期，请回控制台重新生成安装信息。'
  install -d -m 700 "$data_dir"
  printf '%s\n' "$ca_pem" > "$data_dir/enrollment-ca.pem"
  printf '%s\n' "$token" > "$data_dir/enrollment.token"
  chmod 600 "$data_dir/enrollment-ca.pem" "$data_dir/enrollment.token"
  unset token ca_pem
  printf '%s\n' '正在使用一次性令牌注册节点……'
  if [[ "$mode" == host ]]; then
    /usr/local/bin/opsd-agent --data-dir "$data_dir" enroll \
      --hub "$hub" --agent-url "$agent_url" --ca "$data_dir/enrollment-ca.pem" \
      --fingerprint "$fingerprint" --token-file "$data_dir/enrollment.token"
  else
    docker run --rm --network host --read-only --tmpfs /tmp:size=32m,mode=1777 \
      -v "$data_dir:$data_dir" --entrypoint /usr/local/bin/opsd-agent "$agent_image" \
      --data-dir "$data_dir" enroll --hub "$hub" --agent-url "$agent_url" \
      --ca "$data_dir/enrollment-ca.pem" --fingerprint "$fingerprint" --token-file "$data_dir/enrollment.token"
  fi
  check_identity
fi
rm -f -- "$data_dir/enrollment.token" "$data_dir/enrollment-ca.pem"
unset token ca_pem

stage='启动常驻服务'
if [[ "$mode" == host ]]; then
  printf '%s\n' "$service_unit" > "$work_dir/opsd-agent.service"
  install -m 644 "$work_dir/opsd-agent.service" /etc/systemd/system/opsd-agent.service
  systemctl daemon-reload
  systemctl enable --now opsd-agent.service
else
  if docker container inspect opsd-agent >/dev/null 2>&1; then
    docker start opsd-agent >/dev/null
  else
    docker run -d --name opsd-agent --restart unless-stopped \
      --label "io.opsd.node-id=$node_id" --label "io.opsd.hub=$hub" \
      --network host --read-only --tmpfs /tmp:size=32m,mode=1777 \
      --cap-add NET_RAW --cap-add NET_ADMIN \
      -v /var/run/docker.sock:/var/run/docker.sock \
      -v "$data_dir:$data_dir" -e OPSD_AGENT_MODE=container "$agent_image" run >/dev/null
  fi
fi

stage='等待 Hub 确认在线'
status_url="https://${agent_url#wss://}"
status_url="${status_url%/agent}/status"
deadline=$((SECONDS + 60))
for ((attempt=0; attempt<30 && SECONDS<deadline; attempt++)); do
  timeout=$((deadline - SECONDS))
  (( timeout <= 5 )) || timeout=5
  if curl --fail --silent --show-error --proto '=https' --connect-timeout "$timeout" --max-time "$timeout" \
    --cacert "$data_dir/ca.pem" --cert "$data_dir/client.pem" --key "$data_dir/client-key.pem" \
    "$status_url" --output "$work_dir/status.json" 2>/dev/null \
    && jq -e --arg node "$node_id" '.node_id == $node and .connected == true' "$work_dir/status.json" >/dev/null; then
    printf '%s\n' '安装完成：Hub 已确认节点在线，服务已配置自动启动。'
    exit 0
  fi
  remaining=$((deadline - SECONDS))
  (( remaining > 0 )) || break
  if (( remaining > 2 )); then sleep 2; else sleep "$remaining"; fi
done
fail '服务已启动，但 Hub 尚未确认节点在线。身份和服务已保留，请检查 Hub 地址、8444 端口及服务日志后重试。'
