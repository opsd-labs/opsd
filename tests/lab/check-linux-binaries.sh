#!/usr/bin/env bash
# 校验 WSL 内构建产物的真实类型：必须是 Linux ELF，不能是 Windows PE。
set -uo pipefail
repo_dir="${1:-${OPSD_REPO_LINUX:-/mnt/f/Code/1server/opsd}}"
cd "$repo_dir" || { echo "repository not found: $repo_dir" >&2; exit 1; }

fail=0
for b in opsd-hub opsd-agent; do
  path="target/debug/$b"
  if [[ ! -e "$path" ]]; then
    echo "缺失: $path"; fail=1; continue
  fi
  magic=$(head -c 4 "$path" | od -An -tx1 | sed 's/ //g')
  size=$(stat -c %s "$path")
  case "$magic" in
    7f454c46) kind='ELF(Linux)' ;;
    4d5a*)    kind='PE(Windows)'; fail=1 ;;
    *)        kind="未知($magic)"; fail=1 ;;
  esac
  [[ -x "$path" ]] || { kind="$kind 但不可执行"; fail=1; }
  echo "$b: $kind 魔数=$magic 大小=$size 可执行=$([[ -x "$path" ]] && echo 是 || echo 否)"
done

# 残留的 .exe 说明有人在 Windows 侧构建过，必须单独指出。
for b in opsd-hub opsd-agent; do
  [[ -e "target/debug/$b.exe" ]] && echo "注意：存在 Windows 产物 target/debug/$b.exe（本回实验室不依赖它）"
done

exit $fail
