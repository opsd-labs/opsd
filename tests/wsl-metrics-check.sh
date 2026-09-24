#!/usr/bin/env bash
# 在 WSL 中真实构建并运行 Agent 的测试：指标采样、文件路径闸门、受控终端。
# 这三块的判定都依赖 Unix 路径语义与 /proc，只能在 Linux 上验证。
set -uo pipefail

echo "== 环境 =="
whoami
uname -r

# rustup 可能装在 root 或当前用户下，两处都找一下
for candidate in "$HOME/.cargo/bin/cargo" /root/.cargo/bin/cargo; do
  if [ -x "$candidate" ]; then
    CARGO="$candidate"
    break
  fi
done
if [ -z "${CARGO:-}" ]; then
  echo "未找到 cargo，尝试安装"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh
  sh /tmp/rustup.sh -y --profile minimal --default-toolchain stable >/dev/null 2>&1
  CARGO="$HOME/.cargo/bin/cargo"
fi
echo "cargo: $CARGO"
"$CARGO" --version

SRC=/mnt/f/Code/1server/opsd
BUILD="$HOME/opsd-metrics-build"
echo "== 同步源码到 WSL 原生目录（/mnt 上构建很慢） =="
rm -rf "$BUILD"
mkdir -p "$BUILD"
cp -r "$SRC/src" "$BUILD/src"
cp "$SRC/Cargo.toml" "$SRC/Cargo.lock" "$BUILD/"
cd "$BUILD"

echo "== 构建 agent =="
"$CARGO" build --bin opsd-agent 2>&1 | tail -5

echo "== 运行全部 Agent 测试 =="
"$CARGO" test --bin opsd-agent 2>&1 | tail -40

echo "== 路径闸门与受控终端（逐条列出） =="
"$CARGO" test --bin opsd-agent agent::files 2>&1 | tail -30
"$CARGO" test --bin opsd-agent agent::shell 2>&1 | tail -15

echo "== 真实连续采样（打印实际数值） =="
"$CARGO" test --bin opsd-agent 真实连续两次采样的数值自洽 -- --nocapture 2>&1 | tail -12

echo "== 真实主机盘点（打印实际结果） =="
"$CARGO" test --bin opsd-agent 真实主机盘点的字段与风险自洽 -- --nocapture 2>&1 | tail -14
