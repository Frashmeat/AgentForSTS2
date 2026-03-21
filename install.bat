@echo off
:: 第一次进入时开启日志（同时输出到屏幕和 install.log）
if not defined INSTALL_LOGGING (
    set INSTALL_LOGGING=1
    call "%~f0" %* 2>&1 | powershell -NoProfile -Command "$input | Tee-Object -FilePath '%~dp0install.log'"
    exit /b %errorlevel%
)
setlocal enabledelayedexpansion
chcp 65001 >nul
title AgentTheSpire Installer

echo.
echo  ==============================
echo    AgentTheSpire Installer
echo  ==============================
echo.

:: ── 检查 Python ──────────────────────────────────────────────────────────────
python --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] 未找到 Python，请先安装 Python 3.11+
    pause & exit /b 1
)
for /f "tokens=2" %%v in ('python --version 2^>^&1') do set PY_VER=%%v
echo [OK] Python !PY_VER!

:: ── 检查 Node.js ─────────────────────────────────────────────────────────────
node --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] 未找到 Node.js，请先安装 Node.js 18+
    pause & exit /b 1
)
for /f %%v in ('node --version 2^>nul') do set NODE_VER=%%v
echo [OK] Node.js !NODE_VER!

:: ── 检查 .NET SDK ─────────────────────────────────────────────────────────────
dotnet --version >nul 2>&1
if errorlevel 1 (
    echo [WARN] 未找到 .NET SDK，Code Agent 将无法编译 mod
    echo        run setup_mod_deps.bat to install automatically
    echo.
) else (
    for /f %%v in ('dotnet --version 2^>nul') do set DOTNET_VER=%%v
    echo [OK] .NET SDK !DOTNET_VER!
)

:: ── 检查 Godot 路径 ───────────────────────────────────────────────────────────
set "CONFIG_FILE=%~dp0config.json"
set "GODOT_OK=0"
if exist "!CONFIG_FILE!" (
    for /f "usebackq delims=" %%g in (`python -c "import json,pathlib; p=pathlib.Path(r'!CONFIG_FILE!'); cfg=json.loads(p.read_text(encoding='utf-8')) if p.exists() else {}; print(cfg.get('godot_exe_path',''))" 2^>nul`) do set "GODOT_PATH=%%g"
    if defined GODOT_PATH if not "!GODOT_PATH!"=="" (
        if exist "!GODOT_PATH!" (
            echo [OK] Godot 路径已配置
            set "GODOT_OK=1"
        )
    )
)
if "!GODOT_OK!"=="0" (
    echo [WARN] 未配置 Godot 路径，.pck 打包功能不可用
    echo        run setup_mod_deps.bat to install Godot 4.5.1 automatically
)

:: ── 检查 claude CLI ───────────────────────────────────────────────────────────
claude --version >nul 2>&1
if errorlevel 1 (
    echo [WARN] 未找到 claude CLI
    echo        订阅账号模式需运行: npm install -g @anthropic-ai/claude-code
    echo        使用 Kimi/DeepSeek 等 API 可跳过
    echo.
) else (
    echo [OK] claude CLI 已安装
)

:: ── 1/3 Python 依赖（venv 隔离，不影响 conda/mamba/系统 Python）────────────
echo.
echo [1/3] 创建 Python 虚拟环境...
cd /d "%~dp0backend"

if not exist ".venv" (
    python -m venv .venv
    if errorlevel 1 (
        echo [ERROR] 创建 venv 失败
        pause & exit /b 1
    )
    echo [OK] venv 创建成功
) else (
    echo [OK] venv 已存在，跳过创建
)

echo [1/3] 安装后端依赖...
call .venv\Scripts\activate.bat
python -m pip install --upgrade pip --quiet
pip install --quiet -r requirements.txt
if errorlevel 1 (
    echo [ERROR] 后端依赖安装失败
    pause & exit /b 1
)
echo [OK] 后端依赖安装完成
call .venv\Scripts\deactivate.bat

:: ── 2/3 前端依赖 ─────────────────────────────────────────────────────────────
echo.
echo [2/3] 安装前端依赖...
cd /d "%~dp0frontend"
npm install --silent
if errorlevel 1 (
    echo [ERROR] 前端依赖安装失败
    pause & exit /b 1
)
echo [OK] 前端依赖安装完成

:: ── 3/3 前端构建 ─────────────────────────────────────────────────────────────
echo.
echo [3/3] 构建前端...
npm run build
if errorlevel 1 (
    echo [ERROR] 前端构建失败
    pause & exit /b 1
)
echo [OK] 前端构建完成

:: ── 可选：本地图生 ────────────────────────────────────────────────────────────
echo.
set /p LOCAL_IMG="是否安装本地图像生成（ComfyUI + FLUX.2，需约 12GB 磁盘）？[y/N] "
if /i "!LOCAL_IMG!"=="y" (
    echo.
    echo 正在安装 ComfyUI...
    cd /d "%~dp0"
    git clone https://github.com/comfyanonymous/ComfyUI.git comfyui
    cd comfyui
    python -m pip install -r requirements.txt
    echo.
    echo [提示] FLUX.2 模型文件需手动下载放入 comfyui\models\checkpoints\
    echo        下载地址：https://huggingface.co/black-forest-labs/FLUX.2-dev
    python -c "import json,pathlib; p=pathlib.Path('../config.json'); cfg=json.loads(p.read_text(encoding='utf-8')) if p.exists() else {}; cfg.setdefault('image_gen',{})['local']={'comfyui_url':'http://127.0.0.1:8188','installed':True,'model_path':''}; p.write_text(json.dumps(cfg,indent=2,ensure_ascii=False),encoding='utf-8')"
)

echo.
echo  ==============================
echo    安装完成！
echo    运行 start.bat 启动 AgentTheSpire
echo  ==============================
echo.
pause
