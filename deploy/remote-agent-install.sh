#!/usr/bin/env bash
# 安装宿主机 Agent；不执行防火墙写入。
set -Eeuo pipefail
binary=${1:?缺少 Agent 二进制}
hub=${2:?缺少 Hub URL}
agent_url=${3:?缺少 Agent URL}
ca=${4:?缺少 CA 文件}
fingerprint=${5:?缺少 CA 指纹}
token=${6:?缺少一次性令牌文件}
node_id=${7:?缺少节点编号}
[[ "$node_id" =~ ^[A-Za-z0-9._:-]{1,128}$ ]] || { echo '节点编号格式无效' >&2; exit 2; }
[[ "$hub" =~ ^https:// ]] && [[ "$agent_url" =~ ^wss://.+/agent$ ]] || { echo 'Hub 和 Agent 地址必须使用 TLS，Agent 地址必须以 /agent 结尾' >&2; exit 2; }
[[ -x "$binary" ]] || { echo 'Agent 二进制不可执行' >&2; exit 1; }
file "$binary" | grep -Eiq 'ELF.*executable' || { echo 'Agent 不是 Linux ELF' >&2; exit 1; }
if systemctl is-active --quiet opsd-agent.service; then echo 'opsd-agent 已运行，拒绝覆盖' >&2; exit 1; fi
install -d -m 700 /var/lib/opsd-agent
install -m 700 "$binary" /usr/local/bin/opsd-agent
install -m 600 "$ca" /var/lib/opsd-agent/ca.pem
install -m 600 "$token" /var/lib/opsd-agent/enrollment.token
/usr/local/bin/opsd-agent --data-dir /var/lib/opsd-agent enroll --hub "$hub" --agent-url "$agent_url" --ca /var/lib/opsd-agent/ca.pem --fingerprint "$fingerprint" --token-file /var/lib/opsd-agent/enrollment.token
rm -f /var/lib/opsd-agent/enrollment.token
install -m 644 /tmp/opsd-agent.service /etc/systemd/system/opsd-agent.service
systemctl daemon-reload
systemctl enable --now opsd-agent.service
sleep 2
systemctl is-active --quiet opsd-agent.service
printf '节点 %s Agent 已启动\n' "$node_id"
