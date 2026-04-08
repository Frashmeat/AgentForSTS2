from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


def _load_script_params(relative_path: str) -> dict[str, dict[str, object]]:
    script_path = REPO_ROOT / relative_path
    command = rf"""
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$cmd = Get-Command -Name '{script_path}'
$items = foreach ($entry in $cmd.Parameters.GetEnumerator()) {{
    $parameter = $entry.Value
    $paramAttr = $parameter.Attributes | Where-Object {{ $_ -is [System.Management.Automation.ParameterAttribute] }} | Select-Object -First 1
    [pscustomobject]@{{
        name = $entry.Key
        aliases = @($parameter.Aliases)
        position = if ($null -eq $paramAttr) {{ -2147483648 }} else {{ $paramAttr.Position }}
        help = if ($null -eq $paramAttr) {{ '' }} else {{ [string]$paramAttr.HelpMessage }}
    }}
}}
$items | ConvertTo-Json -Depth 4
"""
    completed = subprocess.run(
        ["pwsh", "-NoProfile", "-Command", command],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    payload = json.loads(completed.stdout)
    if isinstance(payload, dict):
        payload = [payload]
    return {item["name"]: item for item in payload}


def _run_script_without_args(relative_path: str) -> subprocess.CompletedProcess[str]:
    script_path = REPO_ROOT / relative_path
    return subprocess.run(
        ["pwsh", "-NoProfile", "-File", str(script_path)],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )


def test_package_release_script_exposes_simplified_parameter_aliases_and_help():
    params = _load_script_params("tools/latest/package-release.ps1")

    assert params["Target"]["position"] == 0
    assert "t" in params["Target"]["aliases"]
    assert "打包目标" in params["Target"]["help"]

    assert "o" in params["OutputRoot"]["aliases"]
    assert "输出目录" in params["OutputRoot"]["help"]

    assert "n" in params["ReleaseName"]["aliases"]
    assert "发布目录名" in params["ReleaseName"]["help"]

    assert "NoFrontend" in params["SkipFrontendBuild"]["aliases"]
    assert "跳过前端构建" in params["SkipFrontendBuild"]["help"]

    assert "NoZip" in params["SkipZip"]["aliases"]
    assert "跳过 zip" in params["SkipZip"]["help"]

    assert "Debug" in params
    assert "db" in params["Debug"]["aliases"]

    assert "h" in params["Help"]["aliases"]
    assert "显示帮助" in params["Help"]["help"]


def test_deploy_docker_script_exposes_simplified_parameter_aliases_and_help():
    params = _load_script_params("tools/latest/deploy-docker.ps1")

    assert params["Target"]["position"] == 0
    assert "t" in params["Target"]["aliases"]
    assert "部署目标" in params["Target"]["help"]

    assert "r" in params["ReleaseRoot"]["aliases"]
    assert "release 目录" in params["ReleaseRoot"]["help"]

    assert "c" in params["ConfigPath"]["aliases"]
    assert "运行时配置" in params["ConfigPath"]["help"]

    assert "n" in params["ProjectName"]["aliases"]
    assert "Compose 项目名" in params["ProjectName"]["help"]

    assert "dbn" in params["PostgresDb"]["aliases"]
    assert "数据库名" in params["PostgresDb"]["help"]

    assert "ResetDb" in params["ResetDatabase"]["aliases"]
    assert "重建数据库" in params["ResetDatabase"]["help"]

    assert "Rebuild" in params["RebuildImages"]["aliases"]
    assert "重建镜像" in params["RebuildImages"]["help"]

    assert "h" in params["Help"]["aliases"]
    assert "显示帮助" in params["Help"]["help"]


def test_build_workstation_installer_script_exposes_short_aliases_and_help():
    params = _load_script_params("tools/latest/build-workstation-installer.ps1")

    assert "py" in params["PythonVersion"]["aliases"]
    assert "Python 版本" in params["PythonVersion"]["help"]

    assert "NoRelease" in params["SkipReleaseBuild"]["aliases"]
    assert "跳过 release 打包" in params["SkipReleaseBuild"]["help"]

    assert "NoExe" in params["SkipInstallerExe"]["aliases"]
    assert "跳过安装器 EXE" in params["SkipInstallerExe"]["help"]

    assert "p" in params["Port"]["aliases"]
    assert "工作站端口" in params["Port"]["help"]

    assert "h" in params["Help"]["aliases"]
    assert "显示帮助" in params["Help"]["help"]


def test_stop_deploy_script_exposes_simplified_parameter_aliases_and_help():
    params = _load_script_params("tools/latest/stop-deploy.ps1")

    assert params["Target"]["position"] == 0
    assert "t" in params["Target"]["aliases"]
    assert "部署目标" in params["Target"]["help"]

    assert "r" in params["ReleaseRoot"]["aliases"]
    assert "release 目录" in params["ReleaseRoot"]["help"]

    assert "h" in params["Help"]["aliases"]
    assert "显示帮助" in params["Help"]["help"]


def test_package_release_script_prints_help_when_run_without_args():
    completed = _run_script_without_args("tools/latest/package-release.ps1")

    assert completed.returncode == 0
    assert "package-release.ps1" in completed.stdout
    assert "PARAMETERS" in completed.stdout
    assert "-Target" in completed.stdout


def test_deploy_docker_script_prints_help_when_run_without_args():
    completed = _run_script_without_args("tools/latest/deploy-docker.ps1")

    assert completed.returncode == 0
    assert "deploy-docker.ps1" in completed.stdout
    assert "PARAMETERS" in completed.stdout
    assert "-Target" in completed.stdout


def test_build_workstation_installer_script_prints_help_when_run_without_args():
    completed = _run_script_without_args("tools/latest/build-workstation-installer.ps1")

    assert completed.returncode == 0
    assert "build-workstation-installer.ps1" in completed.stdout
    assert "PARAMETERS" in completed.stdout
    assert "-PythonVersion" in completed.stdout


def test_stop_deploy_script_prints_help_when_run_without_args():
    completed = _run_script_without_args("tools/latest/stop-deploy.ps1")

    assert completed.returncode == 0
    assert "stop-deploy.ps1" in completed.stdout
    assert "PARAMETERS" in completed.stdout
    assert "-Target" in completed.stdout
