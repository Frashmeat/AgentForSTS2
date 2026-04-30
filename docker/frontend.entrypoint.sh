#!/bin/sh
# 容器启动时根据环境变量生成 runtime-config.js，让前端 SPA 知道后端地址。
# 由 nginx 镜像的 /docker-entrypoint.d/ 机制自动执行。
set -eu

target="/usr/share/nginx/html/runtime-config.js"

web_base="${WEB_BASE_URL:-}"
ws_base="${WORKSTATION_BASE_URL:-}"

# 同时给 http(s) base 与 ws(s) base 派生一个默认值
derive_ws_base() {
    case "$1" in
        https://*) printf 'wss://%s' "${1#https://}" ;;
        http://*)  printf 'ws://%s'  "${1#http://}" ;;
        *)         printf '' ;;
    esac
}

web_ws_base="${WEB_WS_BASE_URL:-$(derive_ws_base "$web_base")}"
ws_ws_base="${WORKSTATION_WS_BASE_URL:-$(derive_ws_base "$ws_base")}"

cat > "$target" <<EOF
window.__AGENT_THE_SPIRE_API_BASES__ = window.__AGENT_THE_SPIRE_API_BASES__ ?? {};
window.__AGENT_THE_SPIRE_WS_BASES__ = window.__AGENT_THE_SPIRE_WS_BASES__ ?? {};
EOF

if [ -n "$web_base" ]; then
    printf 'window.__AGENT_THE_SPIRE_API_BASES__.web = "%s";\n' "$web_base" >> "$target"
fi
if [ -n "$ws_base" ]; then
    printf 'window.__AGENT_THE_SPIRE_API_BASES__.workstation = "%s";\n' "$ws_base" >> "$target"
fi
if [ -n "$web_ws_base" ]; then
    printf 'window.__AGENT_THE_SPIRE_WS_BASES__.web = "%s";\n' "$web_ws_base" >> "$target"
fi
if [ -n "$ws_ws_base" ]; then
    printf 'window.__AGENT_THE_SPIRE_WS_BASES__.workstation = "%s";\n' "$ws_ws_base" >> "$target"
fi

echo "[runtime-config] wrote $target (web=$web_base, workstation=$ws_base)"
