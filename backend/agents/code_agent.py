"""兼容旧导入路径，实际实现已迁到 app.modules.codegen.api。"""
from app.modules.codegen.api import (
    build_and_fix,
    create_asset,
    create_asset_group,
    create_custom_code,
    create_mod_project,
    package_mod,
    run_claude_code,
)
