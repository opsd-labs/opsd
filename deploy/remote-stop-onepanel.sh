#!/usr/bin/env bash
# 只停止预检阶段明确识别且当时处于 active 的 1Panel 单元。
set -Eeuo pipefail
run_id=${1:?缺少运行编号}
node_id=${2:?缺少节点编号}
[[ "$run_id" =~ ^[A-Za-z0-9][A-Za-z0-9_.-]{2,63}$ ]] || exit 2
[[ "$node_id" =~ ^C(001|091|101)$ ]] || exit 2
state_dir="/var/tmp/opsd-preflight-${run_id}-${node_id,,}"
source_file="$state_dir/onepanel.active"
[[ -f "$source_file" ]] || { echo "缺少预检活动清单：$source_file" >&2; exit 1; }
state="/var/tmp/opsd-onepanel-${run_id}-${node_id,,}.active"
: > "$state"
while IFS= read -r unit; do
  [[ -z "$unit" ]] && continue
  [[ "$unit" =~ ^1panel([-.].*)?\.service$ ]] || { echo "拒绝异常单元名：$unit" >&2; exit 2; }
  if systemctl is-active --quiet "$unit"; then
    echo "$unit" >> "$state"
    systemctl stop "$unit"
    echo "已停止：$unit"
  else
    echo "预检时活动但当前已停止：$unit"
  fi
done < "$source_file"
chmod 600 "$state"
echo "恢复清单：$state"
