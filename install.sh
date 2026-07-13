#!/usr/bin/env bash
# IAI Chain 一键安装：从 GitHub Releases 下载预编译 `iai` 并装到 PATH。
#
# 用法：
#   curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash
#   curl -fsSL …/install.sh | bash -s -- --dir ~/.local/bin
#   IAI_VERSION=v0.4.2 bash install.sh
#
# 环境变量：
#   IAI_VERSION      指定 tag（默认 latest）
#   IAI_INSTALL_DIR  安装目录（默认：可写则 /usr/local/bin，否则 ~/.local/bin）
#   IAI_REPO         默认 i3xai/iaichain
#
# 资产命名（与 scripts/publish.sh / iai upgrade 对齐）：
#   iai-v<VER>-<TARGET>.tar.gz
#   TARGET ∈ macos-aarch64 | macos-x86_64 | linux-x86_64 | linux-aarch64

set -euo pipefail

REPO="${IAI_REPO:-i3xai/iaichain}"
VERSION="${IAI_VERSION:-}"
INSTALL_DIR="${IAI_INSTALL_DIR:-}"
GITHUB_API="${GITHUB_API:-https://api.github.com}"
GITHUB_DL="${GITHUB_DL:-https://github.com}"
# 官网静态镜像（publish 同步 dist/）；也可用 IAI_DOWNLOAD_MIRROR 覆盖加速前缀
OFFICIAL_MIRROR="${IAI_OFFICIAL_MIRROR:-https://iaiaiai.ai/releases}"

usage() {
  sed -n '2,18p' "$0"
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version|-v) VERSION="$2"; shift 2 ;;
    --dir)        INSTALL_DIR="$2"; shift 2 ;;
    --repo)       REPO="$2"; shift 2 ;;
    -h|--help)    usage ;;
    *) echo "未知参数: $1（用 --help）" >&2; exit 2 ;;
  esac
done

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "缺少命令: $1" >&2
    exit 1
  }
}

need_cmd curl
need_cmd tar
need_cmd uname

detect_target() {
  local os arch
  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)
  case "$os" in
    darwin)
      case "$arch" in
        arm64|aarch64) echo "macos-aarch64" ;;
        x86_64)        echo "macos-x86_64" ;;
        *) echo "不支持的 macOS 架构: $arch" >&2; exit 1 ;;
      esac
      ;;
    linux)
      case "$arch" in
        x86_64|amd64)  echo "linux-x86_64" ;;
        aarch64|arm64) echo "linux-aarch64" ;;
        *) echo "不支持的 Linux 架构: $arch" >&2; exit 1 ;;
      esac
      ;;
    *)
      echo "暂不支持的系统: $os（请从源码 cargo build --release）" >&2
      exit 1
      ;;
  esac
}

resolve_version() {
  if [ -n "$VERSION" ]; then
    # 允许传 0.4.2 或 v0.4.2
    case "$VERSION" in
      v*) echo "$VERSION" ;;
      *)  echo "v$VERSION" ;;
    esac
    return
  fi
  local tag
  tag=$(curl -fsSL "$GITHUB_API/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -1)
  if [ -z "$tag" ]; then
    echo "无法解析最新 Release。请确认 https://github.com/$REPO/releases 已有资产，或设置 IAI_VERSION=vX.Y.Z" >&2
    exit 1
  fi
  echo "$tag"
}

default_install_dir() {
  if [ -n "${IAI_INSTALL_DIR:-}" ]; then
    echo "$IAI_INSTALL_DIR"
    return
  fi
  if [ -w /usr/local/bin ] 2>/dev/null || [ "$(id -u)" -eq 0 ]; then
    echo "/usr/local/bin"
  else
    echo "${HOME}/.local/bin"
  fi
}

verify_sha256() {
  local file="$1" sumfile="$2"
  if [ ! -f "$sumfile" ]; then
    echo "⚠ 未找到 .sha256，跳过校验"
    return 0
  fi
  local expect actual
  expect=$(awk '{print $1}' "$sumfile" | head -1)
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
  else
    echo "⚠ 无 sha256sum/shasum，跳过校验"
    return 0
  fi
  if [ "$expect" != "$actual" ]; then
    echo "SHA256 不匹配: expect=$expect actual=$actual" >&2
    exit 1
  fi
  echo "✓ SHA256 校验通过"
}

TARGET=$(detect_target)
TAG=$(resolve_version)
VER="${TAG#v}"
ASSET="iai-v${VER}-${TARGET}.tar.gz"
URL="$GITHUB_DL/$REPO/releases/download/${TAG}/${ASSET}"
SHA_URL="${URL}.sha256"

if [ -z "$INSTALL_DIR" ]; then
  INSTALL_DIR=$(default_install_dir)
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "Repo:    $REPO"
echo "Version: $TAG"
echo "Target:  $TARGET"
echo "Asset:   $ASSET"
echo "Install: $INSTALL_DIR/iai"
echo

echo "↓ 下载 $ASSET"
download_ok=0
CANDIDATES=()
if [ -n "${IAI_DOWNLOAD_MIRROR:-}" ]; then
  CANDIDATES+=("${IAI_DOWNLOAD_MIRROR%/}/$REPO/releases/download/${TAG}/${ASSET}")
fi
CANDIDATES+=("$OFFICIAL_MIRROR/$ASSET")
CANDIDATES+=("https://ghfast.top/https://github.com/$REPO/releases/download/${TAG}/${ASSET}")
CANDIDATES+=("https://ghproxy.net/https://github.com/$REPO/releases/download/${TAG}/${ASSET}")
CANDIDATES+=("$URL")

for u in "${CANDIDATES[@]}"; do
  echo "  → $u"
  if curl -fsSL --connect-timeout 15 --max-time 180 "$u" -o "$TMP/$ASSET"; then
    download_ok=1
    URL="$u"  # 同前缀拉 sha256
    break
  fi
done
if [ "$download_ok" != 1 ]; then
  echo "下载失败。请检查 Release 是否包含 $ASSET，或设置 IAI_DOWNLOAD_MIRROR：" >&2
  echo "  https://github.com/$REPO/releases/tag/$TAG" >&2
  echo "  IAI_DOWNLOAD_MIRROR=https://ghfast.top/https://github.com bash install.sh" >&2
  exit 1
fi

# sha：优先同目录镜像，再试 GitHub
SHA_OK=0
for su in "$OFFICIAL_MIRROR/$ASSET.sha256" "${URL}.sha256" "$SHA_URL"; do
  if curl -fsSL --connect-timeout 10 --max-time 60 "$su" -o "$TMP/$ASSET.sha256" 2>/dev/null; then
    verify_sha256 "$TMP/$ASSET" "$TMP/$ASSET.sha256"
    SHA_OK=1
    break
  fi
done
if [ "$SHA_OK" != 1 ]; then
  echo "⚠ 未下载到 .sha256，跳过校验"
fi

echo "↓ 解压"
tar -xzf "$TMP/$ASSET" -C "$TMP"
BIN=$(find "$TMP" -type f -name iai | head -1)
if [ -z "$BIN" ] || [ ! -f "$BIN" ]; then
  echo "包内未找到 iai 二进制" >&2
  exit 1
fi
chmod 755 "$BIN"

mkdir -p "$INSTALL_DIR"
# 若目标不可写，尝试 sudo
if [ -w "$INSTALL_DIR" ]; then
  cp "$BIN" "$INSTALL_DIR/iai"
else
  need_cmd sudo
  sudo cp "$BIN" "$INSTALL_DIR/iai"
  sudo chmod 755 "$INSTALL_DIR/iai"
fi

echo
echo "✓ 已安装: $INSTALL_DIR/iai"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo
    echo "该目录不在 PATH 中，请加入当前 shell："
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    if [ "$INSTALL_DIR" = "${HOME}/.local/bin" ]; then
      echo "或写入 ~/.zshrc / ~/.bashrc 后重新打开终端。"
    fi
    ;;
esac

echo
"$INSTALL_DIR/iai" version || true
echo
echo "下一步见 START.md：iai relay / iai serve 双节点演示"
echo "  https://github.com/$REPO/blob/main/START.md"
