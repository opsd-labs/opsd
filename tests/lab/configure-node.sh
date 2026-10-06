#!/usr/bin/env bash
# 在专用 WSL 发行版内安装实验依赖、配置独立 Docker Engine，并记录能力探测结果。
#
# 用法：
#   configure-node.sh -o lab [--install-deps] [--node-version 22.20.0]
#   configure-node.sh -o lab [--enable-systemd]
#   configure-node.sh opsd_节点名 hub|agent [opsd-test-运行号-] [--start-docker]
#
# 设计约束：
# - 每个发行版使用自己的 /var/lib/docker 与 /var/run/docker.sock，不共享宿主 Docker。
# - Docker 启动失败必须输出 daemon 日志尾部并返回非零，不得伪造成功。
# - 只操作匹配 ^opsd_ 的节点标识。
set -euo pipefail

LOG=/var/log/opsd-lab-bootstrap.log
log() { printf '%s %s\n' "$(date -Iseconds)" "$*" | tee -a "$LOG"; }
die() { log "错误：$*" >&2; exit 1; }

NODE_ID=''
ROLE=''
TEST_PREFIX='opsd-test-local-'
MODE='node'
NODE_VERSION='22.20.0'
INSTALL_DEPS=0
ENABLE_SYSTEMD=0
MIRROR=0
START_DOCKER=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -o|--origin) MODE='bootstrap'; shift ;;
    --install-deps) INSTALL_DEPS=1; shift ;;
    --enable-systemd) ENABLE_SYSTEMD=1; shift ;;
    --mirror) MIRROR=1; shift ;;
    --start-docker) START_DOCKER=1; shift ;;
    --node-version) NODE_VERSION="${2:?--node-version 需要值}"; shift 2 ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    -*) die "未知参数：$1" ;;
    *) if [[ -z "$NODE_ID" ]]; then NODE_ID="$1"; elif [[ -z "$ROLE" ]]; then ROLE="$1"; else TEST_PREFIX="$1"; fi; shift ;;
  esac
done

[[ "$(id -u)" -eq 0 ]] || die '必须以 root 运行。'

if [[ "$MODE" == 'bootstrap' ]]; then
  [[ "$INSTALL_DEPS" -eq 1 || "$ENABLE_SYSTEMD" -eq 1 || "$MIRROR" -eq 1 ]] || die 'bootstrap 模式需要 --install-deps、--enable-systemd 或 --mirror。'
else
  [[ "$NODE_ID" =~ ^opsd_[a-z0-9_]+$ ]] || die "拒绝非 opsd_ 节点标识：$NODE_ID"
  [[ "$ROLE" == hub || "$ROLE" == agent ]] || die '角色只能是 hub 或 agent。'
  [[ "$TEST_PREFIX" =~ ^opsd-test-[a-zA-Z0-9-]+-$ ]] || die "测试资源前缀格式无效：$TEST_PREFIX"
fi

# ---------- systemd ----------
if [[ "$ENABLE_SYSTEMD" -eq 1 ]]; then
  if ! grep -q '^\[boot\]' /etc/wsl.conf 2>/dev/null; then
    printf '\n[boot]\nsystemd=true\n' >> /etc/wsl.conf
  elif ! grep -q '^systemd=true' /etc/wsl.conf 2>/dev/null; then
    # 已有 [boot] 段：在段内补齐 systemd=true，不覆盖其他键。
    awk '
      /^\[boot\]/ { inboot = 1; print; next }
      /^\[/ { inboot = 0 }
      { print }
      inboot && !done && /^$/ { print "systemd=true"; done = 1 }
      END { if (inboot && !done) print "systemd=true" }
    ' /etc/wsl.conf > /etc/wsl.conf.new && mv /etc/wsl.conf.new /etc/wsl.conf
  fi
  log '已写入 /etc/wsl.conf（systemd=true），重启发行版后生效。'
  # 多动作可叠加：只有单独执行 --enable-systemd 时才在此结束，避免跳过后续安装。
  if [[ "$INSTALL_DEPS" -eq 0 && "$START_DOCKER" -eq 0 ]]; then exit 0; fi
fi

# ---------- apt 镜像源 ----------
# 默认的 deb.debian.org 在本机经 HTTPS 拉大包极慢（实测 docker.io 超过 10 分钟未完成）。
# 换成国内镜像并走 HTTP；原始配置备份为 *.orig，便于核查与回退。
if [[ "$INSTALL_DEPS" -eq 1 || "$MIRROR" -eq 1 ]]; then
  if [[ "${OPSD_SKIP_MIRROR:-0}" != '1' ]]; then
    mirror_file=''
    for candidate in /etc/apt/sources.list.d/*.sources; do
      [[ -f "$candidate" ]] && mirror_file="$candidate" && break
    done
    if [[ -n "$mirror_file" ]] && ! grep -q 'tuna.tsinghua.edu.cn' "$mirror_file" 2>/dev/null; then
      cp -n "$mirror_file" "${mirror_file}.orig" 2>/dev/null || true
      cat > "$mirror_file" <<'SOURCES'
Types: deb
URIs: http://mirrors.tuna.tsinghua.edu.cn/debian
Suites: trixie trixie-updates trixie-backports
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp

Types: deb
URIs: http://mirrors.tuna.tsinghua.edu.cn/debian-security
Suites: trixie-security
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp
SOURCES
      log "已将 apt 源切换为清华镜像（原配置备份在 ${mirror_file}.orig）"
    fi
  fi
  # 只做换源时到此为止。
  if [[ "$INSTALL_DEPS" -eq 0 && "$ENABLE_SYSTEMD" -eq 0 && "$START_DOCKER" -eq 0 ]]; then exit 0; fi
fi

# ---------- 安装依赖 ----------
if [[ "$INSTALL_DEPS" -eq 1 ]]; then
  export DEBIAN_FRONTEND=noninteractive
  log '安装系统依赖…'
  apt-get update -qq >> "$LOG" 2>&1

  # 基础工具与防火墙后端：缺失即失败，这些没有替代品。
  apt-get install -y -qq \
    ca-certificates curl gnupg xz-utils \
    iproute2 iptables nftables ufw firewalld \
    iputils-ping procps gawk \
    build-essential pkg-config libssl-dev git file python3 >> "$LOG" 2>&1 \
    || die '安装基础依赖失败'

  # Docker 与 Compose：包名随发行版变动（Debian 12 是 docker-compose-v2，
  # Debian 13 是 docker-compose），逐个尝试而不是硬编码单一名字。
  docker_installed=0
  for pkg in docker.io docker-cli docker-compose docker-buildx; do
    if apt-get install -y -qq "$pkg" >> "$LOG" 2>&1; then
      docker_installed=1
    else
      log "包 $pkg 不可用或安装失败，跳过（可能并非该发行版所需）"
    fi
  done
  command -v dockerd >/dev/null 2>&1 || die '未能安装 Docker Engine（dockerd 缺失）'
  command -v docker  >/dev/null 2>&1 || die '未能安装 Docker CLI（docker 缺失）'
  : "$docker_installed"

  # Compose：优先用发行版包，没有则补官方插件。两种路径都必须真的能用。
  install -d -m 0755 /usr/local/lib/docker/cli-plugins
  if ! docker compose version >/dev/null 2>&1; then
    compose_url='https://github.com/docker/compose/releases/download/v2.29.7/docker-compose-linux-x86_64'
    curl -fsSL "$compose_url" -o /usr/local/lib/docker/cli-plugins/docker-compose >> "$LOG" 2>&1 \
      || die "下载 Docker Compose 失败：$compose_url"
    chmod +x /usr/local/lib/docker/cli-plugins/docker-compose
  fi
  docker compose version >/dev/null 2>&1 || die 'Docker Compose 插件不可用'

  # Node.js 20 不提供 zlib.crc32，集成测试需要 Node 22+。用官方固定版本安装包。
  if ! command -v node >/dev/null 2>&1 || [[ "$(node -e 'process.stdout.write(process.versions.node.split(".")[0])' 2>/dev/null || echo 0)" -lt 22 ]]; then
    node_dir="/usr/local/lib/nodejs/node-v${NODE_VERSION}-linux-x64"
    if [[ ! -x "$node_dir/bin/node" ]]; then
      tarball="/tmp/node-v${NODE_VERSION}-linux-x64.tar.xz"
      node_url="https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz"
      curl -fsSL "$node_url" -o "$tarball" >> "$LOG" 2>&1 || die "下载 Node.js ${NODE_VERSION} 失败：$node_url"
      install -d -m 0755 /usr/local/lib/nodejs
      tar -xJf "$tarball" -C /usr/local/lib/nodejs || die "解压 Node.js 失败"
      rm -f "$tarball"
      [[ -x "$node_dir/bin/node" ]] || die "解压后未找到 $node_dir/bin/node"
    fi
    for b in node npm npx; do
      ln -sf "$node_dir/bin/$b" "/usr/local/bin/$b"
    done
  fi

  # 逐项验证，任何一项不达标即失败——不做「装了就算好」的假设。
  for tool in dockerd docker iptables nft ufw firewall-cmd ip node; do
    command -v "$tool" >/dev/null 2>&1 || die "缺少必需工具：$tool"
  done
  node_major="$(node -e 'process.stdout.write(process.versions.node.split(".")[0])')"
  [[ "$node_major" -ge 22 ]] || die "Node.js 版本为 $node_major，需要 22 或更高"
  node -e 'if(typeof require("node:zlib").crc32!=="function")process.exit(1)' \
    || die 'Node.js 缺少 zlib.crc32，集成测试无法运行'
  log "Node.js $(node --version)（zlib.crc32 可用）"
fi

# ---------- 启动并验证独立 Docker Engine ----------
if [[ "$START_DOCKER" -eq 1 ]]; then
  install -d -m 0755 /var/run /var/lib/docker
  docker_start=''
  if command -v systemctl >/dev/null 2>&1 && systemctl start docker.service 2>/dev/null; then
    docker_start='systemctl'
  elif command -v service >/dev/null 2>&1 && service docker start 2>/dev/null; then
    docker_start='service'
  elif ! pgrep -x dockerd >/dev/null 2>&1; then
    nohup dockerd --host=unix:///var/run/docker.sock >/var/log/opsd-lab-dockerd.log 2>&1 &
    docker_start='dockerd'
  fi

  ready=0
  for _ in $(seq 1 30); do
    if docker info >/dev/null 2>&1; then ready=1; break; fi
    sleep 1
  done
  if [[ "$ready" -ne 1 || -z "$docker_start" ]]; then
    tail -n 60 /var/log/opsd-lab-dockerd.log 2>/dev/null || true
    die 'Docker Engine 未就绪。'
  fi

  install -d -m 0755 /var/lib/opsd-lab
  printf '%s\n' "$NODE_ID" > /var/lib/opsd-lab/node_id
  printf '%s\n' "$ROLE" > /var/lib/opsd-lab/role
  printf '%s\n' "$TEST_PREFIX" > /var/lib/opsd-lab/test_prefix
  printf '%s\n' "$docker_start" > /var/lib/opsd-lab/docker_start_mode

  # 独立 Engine 的硬证据：socket 归属与数据根目录必须在本发行版内。
  socket_path="$(docker context inspect --format '{{.Endpoints.docker.Host}}' 2>/dev/null || echo unix:///var/run/docker.sock)"
  docker_root="$(docker info --format '{{.DockerRootDir}}')"
  server_version="$(docker version --format '{{.Server.Version}}')"
  api_version="$(docker version --format '{{.Server.APIVersion}}')"
  compose_version="$(docker compose version --short 2>/dev/null || echo '不可用')"
  kernel="$(uname -r)"
  [[ "$docker_root" == '/var/lib/docker' ]] || die "Docker 数据根目录异常：$docker_root"

  # 能力探测：记录结果而不是假设能力存在。
  probe() { if "$@" >/dev/null 2>&1; then echo '可用'; else echo '不可用'; fi; }
  cat > /var/lib/opsd-lab/capabilities.txt <<EOF
node_id=$NODE_ID
role=$ROLE
test_prefix=$TEST_PREFIX
docker_start=$docker_start
docker_host=$socket_path
docker_root=$docker_root
docker_engine=$server_version
docker_api=$api_version
docker_compose=$compose_version
kernel=$kernel
kernel_is_shared_wsl2=yes
netns_add=$(probe ip netns add opsd-probe-ns)
iptables_save=$(probe iptables-save -c)
nft_list=$(probe nft -nn list ruleset)
ufw_status=$(probe ufw status verbose)
firewalld_state=$(probe firewall-cmd --state)
EOF
  ip netns del opsd-probe-ns 2>/dev/null || true

  log "节点已配置：${NODE_ID}（${ROLE}），前缀 ${TEST_PREFIX}"
  log "Docker Engine ${server_version}（API ${api_version}，Compose ${compose_version}，启动方式 ${docker_start}）"
  log "数据根目录 ${docker_root}，socket ${socket_path}"
  cat /var/lib/opsd-lab/capabilities.txt | tee -a "$LOG"
fi
