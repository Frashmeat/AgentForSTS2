#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
FRONTEND_DIR="$ROOT_DIR/frontend"
FRONTEND_DIST="$FRONTEND_DIR/dist"
FRONTEND_NODE_MODULES="$FRONTEND_DIR/node_modules"
BACKEND_DIR="$ROOT_DIR/backend"
VENV_PYTHON="$BACKEND_DIR/.venv/bin/python"

if [[ ! -d "$FRONTEND_DIST" ]]; then
    echo "检测到前端构建产物不存在，正在构建 frontend/dist..."
    if [[ ! -d "$FRONTEND_NODE_MODULES" ]]; then
        echo "[ERROR] 缺少前端依赖目录 \"$FRONTEND_NODE_MODULES\"。"
        echo "请先运行 ./tools/install/install.sh 安装依赖后再重试。"
        exit 1
    fi

    (
        cd "$FRONTEND_DIR"
        npm run build
    )
fi

if [[ -x "$VENV_PYTHON" ]]; then
    PYTHON_BIN="$VENV_PYTHON"
elif command -v python3 &>/dev/null; then
    PYTHON_BIN="$(command -v python3)"
else
    echo "[ERROR] 未找到可用的 Python 解释器。"
    echo "请先运行 ./tools/install/install.sh 创建 backend/.venv。"
    exit 1
fi

echo "启动 AgentTheSpire..."
echo "打开浏览器访问 http://localhost:7860"
"$PYTHON_BIN" "$BACKEND_DIR/main.py" &
sleep 2 && open "http://localhost:7860" 2>/dev/null || xdg-open "http://localhost:7860" 2>/dev/null || true
wait
