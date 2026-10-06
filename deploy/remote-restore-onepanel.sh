#!/usr/bin/env bash
# 恢复指定运行编号保存的 1Panel 活动单元，不执行 disable。
set -Eeuo pipefail
run_id=${1:?缺少运行编号}
node_id=${2:?缺少节点编号}
[[ "$run_id" =~ ^[A-Za-z0-9][A-Za-z0-9_.-]{2,63}$ ]] || exit 2
[[ "$node_id" =~ ^C(001|091|101)$ ]] || exit 2
state="/var/tmp/opsd-onepanel-${run_id}-${node_id,,}.active"
[[ -f "$state" ]] || { echo "没有恢复清单：$state" >&2; exit 1; }
while IFS= read -r unit; do
  [[ -z "$unit" ]] && continue
  [[ "$unit" =~ ^1panel([-.].*)?\.service$ ]] || { echo "拒绝异常单元名：$unit" >&2; exit 2; }
  systemctl start "$unit"
  echo "已恢复：$unit"
done < "$state"
