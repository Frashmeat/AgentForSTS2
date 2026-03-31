"""
STS2 Mod 项目初始化工具
-----------------------
用 S01_IronStrike 作为模板，纯 Python 复制 + 重命名，不需要 Claude。
每个新项目节省 3-7 分钟初始化时间。
"""
from __future__ import annotations

import shutil
from pathlib import Path

from app.shared.prompting import PromptLoader

_TEXT_LOADER = PromptLoader()


# ── local.props 自动生成 ──────────────────────────────────────────────────────

def ensure_local_props(project_root: Path) -> bool:
    """
    若 project_root 下没有 local.props，根据 config.json 自动生成。
    返回 True 表示成功生成（或已存在），False 表示配置不完整无法生成。
    """
    from config import get_config
    props_path = project_root / "local.props"
    if props_path.exists():
        return True

    cfg = get_config()
    sts2_path = cfg.get("sts2_path", "").strip()
    godot_path = cfg.get("godot_exe_path", "").strip()

    if not sts2_path or not godot_path:
        return False

    content = _TEXT_LOADER.render(
        "runtime_system.project_utils_local_props_template",
        {
            "sts2_path": sts2_path,
            "godot_path": godot_path,
        },
    )
    props_path.write_text(content, encoding="utf-8")
    return True


# ── 路径自动检测 ───────────────────────────────────────────────────────────────

def detect_paths() -> dict:
    """
    自动检测 STS2 游戏路径和 Godot 4.5.1 Mono 可执行文件路径。
    返回 {"sts2_path": str|None, "godot_exe_path": str|None, "notes": [str]}
    """
    import glob as _glob
    import sys

    notes: list[str] = []
    sts2_path: str | None = None
    godot_path: str | None = None

    # ── 检测 STS2 ──────────────────────────────────────────────────────────
    # 策略1：通过 Steam 注册表找安装路径
    if sys.platform == "win32":
        sts2_path, note = _find_sts2_via_registry()
        if note:
            notes.append(note)

    # 策略2：扫描常见 Steam 库目录
    if not sts2_path:
        sts2_path, note = _find_sts2_in_common_paths()
        if note:
            notes.append(note)

    # ── 检测 Godot 4.5.1 Mono ─────────────────────────────────────────────
    godot_path, note = _find_godot()
    if note:
        notes.append(note)

    return {
        "sts2_path": sts2_path,
        "godot_exe_path": godot_path,
        "notes": notes,
    }


def _find_sts2_via_registry() -> tuple[str | None, str]:
    """通过 Steam 注册表找 STS2 安装路径（Windows）。"""
    try:
        import winreg
        # 找 Steam 安装路径
        for hive in (winreg.HKEY_LOCAL_MACHINE, winreg.HKEY_CURRENT_USER):
            for sub in (r"SOFTWARE\Valve\Steam", r"SOFTWARE\WOW6432Node\Valve\Steam"):
                try:
                    with winreg.OpenKey(hive, sub) as k:
                        steam_path = Path(winreg.QueryValueEx(k, "InstallPath")[0])
                        result = _search_steam_libraries(steam_path)
                        if result:
                            return str(result), _TEXT_LOADER.render(
                                "runtime_system.project_utils_sts2_found_via_registry",
                                {"path": result},
                            )
                except (FileNotFoundError, OSError):
                    continue
    except ImportError:
        pass
    return None, ""


def _find_sts2_in_common_paths() -> tuple[str | None, str]:
    """在常见 Steam 路径下搜索 STS2。"""
    import os
    common_steam_roots = [
        Path("C:/Program Files (x86)/Steam"),
        Path("C:/Program Files/Steam"),
        Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Steam",
        Path("D:/Steam"),
        Path("E:/Steam"),
        Path("E:/steam"),
        Path(os.environ.get("USERPROFILE", "")) / ".steam" / "steam",
    ]
    for root in common_steam_roots:
        result = _search_steam_libraries(root)
        if result:
            return str(result), _TEXT_LOADER.render(
                "runtime_system.project_utils_sts2_found_in_common_paths",
                {"path": result},
            )
    return None, _TEXT_LOADER.load("runtime_system.project_utils_sts2_not_found")


def _search_steam_libraries(steam_root: Path) -> Path | None:
    """在 Steam 安装目录及其所有库路径中搜索 STS2。"""
    import re
    target = "Slay the Spire 2"

    # 直接检查默认 steamapps
    candidate = steam_root / "steamapps" / "common" / target
    if candidate.exists():
        return candidate

    # 解析 libraryfolders.vdf 找额外库
    vdf = steam_root / "steamapps" / "libraryfolders.vdf"
    if vdf.exists():
        try:
            text = vdf.read_text(encoding="utf-8", errors="replace")
            for m in re.finditer(r'"path"\s+"([^"]+)"', text):
                lib = Path(m.group(1).replace("\\\\", "/"))
                candidate = lib / "steamapps" / "common" / target
                if candidate.exists():
                    return candidate
        except Exception:
            pass
    return None


def _find_godot() -> tuple[str | None, str]:
    """搜索 Godot 4.5.1 Mono 可执行文件。"""
    import os, glob as _glob

    search_dirs = [
        "C:/Program Files/Godot",
        "C:/Program Files (x86)/Godot",
        "C:/tools",
        str(Path.home()),
        str(Path.home() / "Downloads"),
        str(Path.home() / "Desktop"),
        "D:/tools",
        "E:/tools",
        os.environ.get("LOCALAPPDATA", ""),
    ]
    pattern = "Godot_v4.5.1*mono*win64.exe"
    for d in search_dirs:
        if not d:
            continue
        matches = _glob.glob(str(Path(d) / "**" / pattern), recursive=True)
        if matches:
            return matches[0], _TEXT_LOADER.render(
                "runtime_system.project_utils_godot_found",
                {"path": matches[0]},
            )

    # 也搜索 PATH
    import shutil as _shutil
    for name in ("godot", "Godot"):
        found = _shutil.which(name)
        if found:
            return found, _TEXT_LOADER.render(
                "runtime_system.project_utils_godot_found_in_path",
                {"path": found},
            )

    return None, _TEXT_LOADER.load("runtime_system.project_utils_godot_not_found")

# ── 模板源 ─────────────────────────────────────────────────────────────────

# 默认模板位于仓库内 mod_template/ 目录，可通过 config.json 的 mod_template_path 覆盖
_DEFAULT_TEMPLATE = Path(__file__).parent.parent / "mod_template"
TEMPLATE_NAME = "ModTemplate"


def _get_template_source() -> Path:
    """返回模板路径：优先 config 里的自定义路径，其次仓库内默认路径。"""
    from config import get_config
    cfg_path = get_config().get("mod_template_path", "")
    if cfg_path:
        p = Path(cfg_path)
        if p.exists():
            return p
    return _DEFAULT_TEMPLATE

# 复制时跳过的目录/文件（在相对路径的任意层级出现即跳过）
_SKIP_DIRS = {
    ".git", "packages", "content", "bin", "obj", ".godot",
    "Cards", "Relics", "Powers", "Patches",   # 资产源码，每项目不同
}
_SKIP_SUFFIXES = {".log", ".uid", ".import"}
_SKIP_EXACT_FILES = {"Alchyr.Sts2.Templates.csproj", "README.md"}


def _should_skip(rel: Path) -> bool:
    parts = set(rel.parts)
    if parts & _SKIP_DIRS:
        return True
    if rel.name in _SKIP_EXACT_FILES:
        return True
    if rel.suffix in _SKIP_SUFFIXES:
        return True
    # 跳过 {ModName}/images/ 下的文件（图片由流程后续生成）
    parts_list = rel.parts
    if "images" in parts_list and len(parts_list) > parts_list.index("images") + 1:
        return True
    # 跳过 localization/ 下的 .json（由 Agent 生成）
    if "localization" in parts_list and rel.suffix == ".json":
        return True
    return False


def create_project_from_template(project_name: str, target_dir: Path) -> Path:
    """
    从 S01_IronStrike 模板克隆新项目，不需要 Claude。
    返回新项目的根目录路径。

    做的事：
    1. 遍历模板文件（跳过 git/packages/build/资产源码/图片）
    2. 把所有路径里的 TEMPLATE_NAME 替换为 project_name
    3. 把所有文本文件内容里的 TEMPLATE_NAME 替换为 project_name
    4. 建立空的 images/ 和 localization/ 目录结构
    """
    src = _get_template_source()
    if not src.exists():
        raise FileNotFoundError(
            _TEXT_LOADER.render(
                "runtime_system.project_utils_template_missing",
                {
                    "template_path": src,
                    "default_template_path": _DEFAULT_TEMPLATE,
                },
            )
        )

    project_root = target_dir / project_name
    project_root.mkdir(parents=True, exist_ok=True)

    old = TEMPLATE_NAME
    new = project_name

    for src_path in src.rglob("*"):
        rel = src_path.relative_to(src)
        if _should_skip(rel):
            continue

        # 路径里的旧名替换为新名
        new_parts = [p.replace(old, new) for p in rel.parts]
        dst_path = project_root / Path(*new_parts)

        if src_path.is_dir():
            dst_path.mkdir(parents=True, exist_ok=True)
            continue

        dst_path.parent.mkdir(parents=True, exist_ok=True)
        raw = src_path.read_bytes()
        try:
            text = raw.decode("utf-8")
            text = text.replace(old, new)
            dst_path.write_text(text, encoding="utf-8")
        except UnicodeDecodeError:
            # 二进制文件（mod_image.png 等）原样复制
            shutil.copy2(src_path, dst_path)

    # 建立空的图片目录（image gen 后会填充）
    res_dir = project_root / new
    for subdir in [
        "images/card_portraits/big",
        "images/relics/big",
        "images/powers/big",
    ]:
        (res_dir / subdir).mkdir(parents=True, exist_ok=True)

    # 建立空的 localization 目录
    for lang in ["eng", "zhs"]:
        (res_dir / "localization" / lang).mkdir(parents=True, exist_ok=True)

    # 确保 export_presets.cfg 的 include_filter 包含 *.json，
    # 否则 Godot all_resources 导出不会打包 localization JSON 文件
    presets_path = project_root / "export_presets.cfg"
    if presets_path.exists():
        cfg = presets_path.read_text(encoding="utf-8")
        cfg = cfg.replace('include_filter=""', 'include_filter="*.json"')
        presets_path.write_text(cfg, encoding="utf-8")

    ensure_local_props(project_root)
    return project_root
