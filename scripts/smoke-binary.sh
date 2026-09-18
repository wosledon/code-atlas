#!/usr/bin/env bash
# 验证「单个二进制」这个交付形态：把它复制到没有 web/dist 的隔离目录再跑起来，
# 确认内嵌 UI 真的在包里、并且能提供页面与 API。
#
#   scripts/smoke-binary.sh <atlas 可执行文件路径>
#
# 退出码非 0 表示这个二进制不能独立工作（最常见原因：打包前忘了构建前端，
# build.rs 因此没有内嵌资源，只能回落到内置兜底页）。
set -euo pipefail

BIN=${1:?用法: smoke-binary.sh <atlas 可执行文件>}
BIN=$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")
PORT=${SMOKE_PORT:-47321}

# 隔离目录：这里没有 web/dist，二进制只能靠内嵌资源。
ISO=$(mktemp -d)
trap 'rm -rf "$ISO"; [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null || true' EXIT
cp "$BIN" "$ISO/atlas"
mkdir -p "$ISO/repo"

# Git Bash 会把 POSIX 路径原样交给原生 exe，Windows 上不认；能转就转。
if command -v cygpath >/dev/null 2>&1; then
  REPO_ARG=$(cygpath -w "$ISO/repo")
else
  REPO_ARG="$ISO/repo"
fi

"$ISO/atlas" web --port "$PORT" --path "$REPO_ARG" >"$ISO/log" 2>&1 &
PID=$!

# 内嵌资源的存在与否，启动日志里会直说。
for _ in $(seq 1 40); do
  grep -q "UI: embedded in binary" "$ISO/log" && break
  kill -0 "$PID" 2>/dev/null || { echo "二进制启动即退出："; cat "$ISO/log"; exit 1; }
  sleep 0.25
done
if ! grep -q "UI: embedded in binary" "$ISO/log"; then
  echo "未走内嵌 UI 分支（多半是打包时没有 web/dist）："
  cat "$ISO/log"
  exit 1
fi

# 能返回首页与 API 才算真的可用。
for _ in $(seq 1 40); do
  if curl -sf "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
curl -sf "http://127.0.0.1:$PORT/api/health" >/dev/null || { echo "API 无响应"; cat "$ISO/log"; exit 1; }
curl -sf "http://127.0.0.1:$PORT/" | grep -q 'id="root"' || { echo "首页不是内嵌的 SPA"; exit 1; }

echo "冒烟通过：内嵌 UI 可用，首页与 /api/health 均正常"
