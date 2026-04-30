from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
INSTALL_SCRIPT = REPO_ROOT / "tools" / "install" / "install.ps1"


def test_install_script_adds_runtime_tools_to_path_for_ilspycmd():
    content = INSTALL_SCRIPT.read_text(encoding="utf-8")

    assert "Add-UserPathEntry -PathEntry $script:RuntimeToolsDir" in content
    assert "Sync-ToolCommandPath -ResolvedCommand $resolved" in content
    assert "Sync-ToolCommandPath -ResolvedCommand $ilspyExe" in content
