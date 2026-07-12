#!/usr/bin/env bash
# 部署官网静态手册到阿里云 iaiaiai.ai（/docs/）
#
# 用法：
#   scripts/deploy-docs.sh
#   DOCS_HOST=root@139.224.28.252 scripts/deploy-docs.sh
#
# 目标：
#   /var/www/iai/docs/     ← 手册静态文件
#   nginx location /docs/  ← 优先于反代到 iai serve

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
HOST="${DOCS_HOST:-root@139.224.28.252}"
REMOTE_DOCS="/var/www/iai/docs"
REMOTE_INSTALL="/var/www/iai/install.sh"

echo "→ rsync web/docs → $HOST:$REMOTE_DOCS"
ssh -o BatchMode=yes "$HOST" "mkdir -p '$REMOTE_DOCS' /var/www/iai/site/landing /var/www/iai/site/shared"
rsync -az --delete "$ROOT/web/docs/" "$HOST:$REMOTE_DOCS/"

echo "→ rsync 落地页（首页「文档」→ /docs/）"
rsync -az "$ROOT/web/landing/" "$HOST:/var/www/iai/site/landing/"
rsync -az "$ROOT/web/shared/" "$HOST:/var/www/iai/site/shared/"

echo "→ 同步 install.sh"
scp -q "$ROOT/install.sh" "$HOST:$REMOTE_INSTALL"
ssh -o BatchMode=yes "$HOST" "chmod 755 '$REMOTE_INSTALL'"

echo "→ 确保 nginx 有 /docs/ location"
ssh -o BatchMode=yes "$HOST" 'bash -s' <<'REMOTE'
set -euo pipefail
CONF=/etc/nginx/sites-available/iai
if grep -q 'location /docs/' "$CONF"; then
  echo "  nginx /docs/ 已存在"
else
  # 在 HTTPS server 的 location / 之前插入 /docs/
  python3 - <<'PY'
from pathlib import Path
p = Path("/etc/nginx/sites-available/iai")
text = p.read_text()
snippet = """
    location /docs/ {
        alias /var/www/iai/docs/;
        try_files $uri $uri/ /docs/index.html;
        add_header Cache-Control "public, max-age=300";
    }
"""
# 仅在 HTTPS server（含 ssl_certificate）块中、第一个「location / {」反代前插入
marker = "    # API 限流 + 反代\n    location /api/ {"
# 找 HTTPS 段：含 live/iaiaiai.ai
parts = text.split("ssl_certificate     /etc/letsencrypt/live/iaiaiai.ai/fullchain.pem;")
if len(parts) < 2:
    raise SystemExit("未找到 iaiaiai.ai HTTPS 证书段")
head, rest = parts[0], parts[1]
# 在 rest 里找 API 反代前插入 docs（HTTPS 块）
if "location /docs/" in rest:
    print("already present in https block")
else:
    if marker not in rest:
        # 退而求其次：在「location / {」proxy 前插入
        needle = "    location / {\n        proxy_pass http://127.0.0.1:8787;"
        if needle not in rest:
            raise SystemExit("未找到插入点")
        rest = rest.replace(needle, snippet + "\n" + needle, 1)
    else:
        rest = rest.replace(marker, snippet + "\n" + marker, 1)
    p.write_text(head + "ssl_certificate     /etc/letsencrypt/live/iaiaiai.ai/fullchain.pem;" + rest)
    print("  已写入 /docs/ location")
PY
fi
nginx -t
systemctl reload nginx
echo "  nginx reloaded"
REMOTE

echo
echo "✓ 完成：https://iaiaiai.ai/docs/"
echo "  自检：curl -fsSIL https://iaiaiai.ai/docs/ | head"
