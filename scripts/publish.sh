#!/usr/bin/env bash
# 发布 IAI Chain：跨平台构建 + tar.gz 打包 + SHA256 生成，输出到 dist/ 供 gh release 上传。
#
# 用法：
#   scripts/publish.sh                            # 默认：host + linux-x86_64 + linux-aarch64
#   scripts/publish.sh --targets host,linux-x86_64
#   scripts/publish.sh --tag v0.5.0               # 显式 tag（默认从 Cargo.toml 读）
#   scripts/publish.sh --upload                   # 自动 gh release create + upload（需 gh 已认证）
#   scripts/publish.sh --docker-image rust:1.88-bookworm
#
# 资产命名（与 iai upgrade 命令对齐）：
#   dist/iai-v<TAG>-<TARGET>.tar.gz
#   dist/iai-v<TAG>-<TARGET>.tar.gz.sha256
#
# 跨平台说明：
#   - host            —— 本机 cargo build（最快，需要本机 Rust 工具链）
#   - linux-x86_64    —— Docker rust:1.86-bookworm（已验证可用）
#   - linux-aarch64   —— Docker rust:1.86-bookworm --platform linux/arm64
#   - macos-x86_64    —— 需要 osxcross，本脚本不自动处理；建议在 GitHub Actions 矩阵中跑
#   - windows-x86_64  —— 需要 mingw cross，本脚本不自动处理；同上

set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)

# ── 参数 ───────────────────────────────────────────────────────
TAG=""
TARGETS=""
DOCKER_IMAGE="rust:1.86-bookworm"
DO_UPLOAD=0

while [ $# -gt 0 ]; do
  case "$1" in
    --tag)            TAG="$2"; shift 2 ;;
    --targets)        TARGETS="$2"; shift 2 ;;
    --docker-image)   DOCKER_IMAGE="$2"; shift 2 ;;
    --upload)         DO_UPLOAD=1; shift ;;
    -h|--help)
      sed -n '2,28p' "$0"; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

# ── 版本 ───────────────────────────────────────────────────────
if [ -z "$TAG" ]; then
  VERSION=$(grep '^version' Cargo.toml | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
  TAG="v$VERSION"
fi
echo "Tag:    $TAG"
echo

# ── 目标列表 ───────────────────────────────────────────────────
host_triple=$(rustc -vV 2>/dev/null | awk '/^host:/ {print $2}')
if [ -z "$host_triple" ]; then
  echo "未检测到 rustc，请先安装 Rust 工具链" >&2
  exit 1
fi
host_target=""
case "$host_triple" in
  x86_64-apple-darwin)  host_target="macos-x86_64"   ;;
  aarch64-apple-darwin) host_target="macos-aarch64"  ;;
  x86_64-unknown-linux-gnu)   host_target="linux-x86_64"   ;;
  aarch64-unknown-linux-gnu)  host_target="linux-aarch64"  ;;
  *) echo "不支持的 host: $host_triple" >&2; exit 1 ;;
esac

if [ -z "$TARGETS" ]; then
  TARGETS="host,linux-x86_64,linux-aarch64"
fi

declare -a want_targets=()
IFS=',' read -ra want_targets <<< "$TARGETS"
for i in "${!want_targets[@]}"; do
  want_targets[$i]=$(echo "${want_targets[$i]}" | xargs)
done

echo "Targets:"
for t in "${want_targets[@]}"; do
  if [ "$t" = "host" ]; then
    echo "  - host ($host_target)"
  else
    echo "  - $t"
  fi
done
echo

# ── 准备 dist/ ─────────────────────────────────────────────────
rm -rf dist
mkdir -p dist

# ── 构建函数 ───────────────────────────────────────────────────

strip_binary() {
  local bin="$1"
  local target="$2"
  case "$target" in
    linux-*)
      # Linux ELF：用 GNU strip（macOS 系统 strip 也能识别 ELF，但 GNU strip 更稳）
      if command -v strip >/dev/null 2>&1; then
        strip "$bin"
      fi
      chmod 755 "$bin"
      ;;
    macos-*)
      # macOS Mach-O：不调用 strip（macOS 系统 strip 与 GNU strip 行为差异大；
      # release profile 已剥离大部分符号，二进制已足够小）
      chmod 755 "$bin"
      ;;
    windows-*) ;;
  esac
}

# 输出 tar.gz 顶层目录名（与 upgrade.rs asset_name() 约定对齐）
asset_basename() {
  local ver="$1"  # 已去掉 v 前缀
  local target="$2"
  echo "iai-v${ver}-${target}.tar.gz"
}

do_native_build() {
  local target="$1"
  echo "→ 本机构建 $target"
  cargo build --release --bin iai 2>&1 | tail -8
  local bin="target/release/iai"
  strip_binary "$bin" "$target"
  echo "  ✓ built $(ls -la "$bin" | awk '{print $5}') bytes"
}

do_docker_build() {
  local target="$1"
  local platform
  case "$target" in
    linux-x86_64)   platform="linux/amd64" ;;
    linux-aarch64)  platform="linux/arm64" ;;
    *) echo "Docker 不支持的目标: $target" >&2; return 1 ;;
  esac
  echo "→ Docker 构建 $target ($platform)"

  local image_dir
  image_dir=$(mktemp -d -t iai-build-XXXXXX)
  cp target/release/iai "$image_dir/iai.host" 2>/dev/null || true

  docker run --rm \
    --platform "$platform" \
    -v "$ROOT":/src \
    -w /src \
    "$DOCKER_IMAGE" \
    sh -c 'apt-get update -qq >/dev/null 2>&1 && \
           apt-get install -y -qq pkg-config libssl-dev >/dev/null 2>&1 && \
           cargo build --release --bin iai 2>&1 | tail -5 && \
           strip /src/target/release/iai -o /src/target/release/iai.stripped && \
           chmod 755 /src/target/release/iai.stripped && \
           ls -la /src/target/release/iai.stripped'

  local bin="target/release/iai.stripped"
  if [ ! -f "$bin" ]; then
    echo "Docker 构建产物缺失: $bin" >&2
    return 1
  fi
  echo "  ✓ built $(ls -la "$bin" | awk '{print $5}') bytes"
}

pack_one() {
  local target="$1"
  local ver="${TAG#v}"
  local tar_name
  tar_name=$(asset_basename "$ver" "$target")
  local tmp_dir
  tmp_dir=$(mktemp -d -t iai-pack-XXXXXX)

  # tar 包内容：单个 iai 二进制 + 顶层目录
  local tar_top="iai-v${ver}-${target}"
  mkdir -p "$tmp_dir/$tar_top"

  # 选择输入：Docker 流程会产生 iai.stripped；host/macOS 流程只有 iai
  if [ -f target/release/iai.stripped ]; then
    cp target/release/iai.stripped "$tmp_dir/$tar_top/iai"
  else
    cp target/release/iai          "$tmp_dir/$tar_top/iai"
  fi
  chmod 755 "$tmp_dir/$tar_top/iai"

  # 用绝对路径 dist，避免 cd 后相对路径错乱
  local dist_abs="$ROOT/dist"
  (cd "$tmp_dir" && tar czf "$dist_abs/$tar_name" "$tar_top")

  # SHA256（sha256sum 兼容格式：<hash>  <file>）
  (cd "$dist_abs" && sha256sum "$tar_name" > "$tar_name.sha256")

  rm -rf "$tmp_dir"
  echo "  📦 dist/$tar_name  +  .sha256"
}

# ── 主流程 ─────────────────────────────────────────────────────

for t in "${want_targets[@]}"; do
  actual="$t"
  if [ "$t" = "host" ]; then
    actual="$host_target"
  fi

  echo "── $actual ──"

  if [ "$actual" = "$host_target" ]; then
    do_native_build "$actual"
  elif [[ "$actual" == linux-* ]]; then
    do_docker_build "$actual"
  else
    echo "  ⚠ 跳过 $actual（本脚本不支持；建议 GitHub Actions 矩阵构建）" >&2
    continue
  fi

  pack_one "$actual"
  echo
done

# ── 上传 ───────────────────────────────────────────────────────

echo "────────────────────"
echo "dist/ 内容："
ls -la dist/
echo

if [ "$DO_UPLOAD" = 1 ]; then
  if ! command -v gh >/dev/null 2>&1; then
    echo "未安装 gh CLI，跳过上传。请手动：gh release create $TAG ./dist/*" >&2
    exit 1
  fi
  echo "🚀 创建 release 并上传 $TAG ..."
  if gh release view "$TAG" >/dev/null 2>&1; then
    gh release upload "$TAG" dist/* --clobber
  else
    notes="Auto-built release $TAG.

Assets (per target):
$(ls dist/ | sed 's/^/- /')
"
    gh release create "$TAG" dist/* --title "$TAG" --notes "$notes"
  fi
  echo "✓ 完成：https://github.com/i3xai/iaichain/releases/tag/$TAG"
else
  echo "📋 下一步：去 https://github.com/i3xai/iaichain/releases 创建 release $TAG"
  echo "   把以下资产拖上去（或用 gh CLI）："
  echo ""
  echo "     gh release create $TAG ./dist/* \\"
  echo "       --title \"$TAG\" \\"
  echo "       --notes  \"<填发布说明>\""
fi