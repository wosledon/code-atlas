#!/usr/bin/env bash
# 把单个二进制打成发布包：二进制 + 两个 README + 许可证 + 配置样例。
#
#   scripts/package-release.sh <target triple> <二进制路径> [输出目录=dist]
#
# Windows 出 .zip（7z 在 GitHub Windows runner 上自带），其余出 .tar.gz，
# 包内统一是一层 `atlas-<triple>/` 目录，解压即用。
set -euo pipefail

TRIPLE=${1:?用法: package-release.sh <triple> <二进制> [输出目录]}
BINARY=${2:?用法: package-release.sh <triple> <二进制> [输出目录]}
OUT=${3:-dist}

test -f "$BINARY" || { echo "找不到二进制：$BINARY" >&2; exit 1; }
for extra in LICENSE README.md README.en.md atlas.toml.example; do
  test -f "$extra" || { echo "缺少随包文件：$extra" >&2; exit 1; }
done

STAGE="stage/atlas-$TRIPLE"
rm -rf "$STAGE" "$OUT/atlas-$TRIPLE."*
mkdir -p "$STAGE" "$OUT"
cp "$BINARY" "$STAGE/"
cp LICENSE README.md README.en.md atlas.toml.example "$STAGE/"

# 压缩工具在各平台不一：`7z` 只在部分镜像里有，Git Bash 也不带 `zip`，
# 所以按可用性逐级回退，最后用 Python 标准库（CI 与开发机都有）。
make_zip() {
  local archive=$1 dir=$2
  if command -v 7z >/dev/null 2>&1; then
    7z a -bso0 -bsp0 "$archive" "$dir"
  elif command -v zip >/dev/null 2>&1; then
    zip -qr "$archive" "$dir"
  elif command -v python3 >/dev/null 2>&1 || command -v python >/dev/null 2>&1; then
    local py; py=$(command -v python3 || command -v python)
    "$py" - "$archive" "$dir" <<'PY'
import os, sys, zipfile

archive, stage = sys.argv[1], sys.argv[2].rstrip("/")
base = os.path.dirname(stage) or "."
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for dirpath, _, files in os.walk(stage):
        for name in files:
            path = os.path.join(dirpath, name)
            z.write(path, os.path.relpath(path, base))
PY
  else
    echo "没有可用的 zip 工具（试过 7z / zip / python3）" >&2
    return 1
  fi
}

case "$TRIPLE" in
  *windows*) ARCHIVE="$OUT/atlas-$TRIPLE.zip"; make_zip "$ARCHIVE" "$STAGE" ;;
  *)         ARCHIVE="$OUT/atlas-$TRIPLE.tar.gz"; tar -czf "$ARCHIVE" -C stage "atlas-$TRIPLE" ;;
esac

rm -rf "$STAGE"
echo "已生成 $ARCHIVE（$(( $(wc -c < "$ARCHIVE") / 1024 )) KB）"
