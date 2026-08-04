# STS2 Pack 最终候选与安装态验收

## Goal

为已提交的 STS2 Pack Item/Resource Orders 1-8 生成唯一的新 Windows 候选，并通过安装版桌面应用验证 Pack v3、Truth、ItemDefinition、Batch v4、Complex v3、真实 Build/Package、Run/Artifact 和工程锁合同。不得复用旧候选、旧 Run 或人工重命名产物。

## Bound Commit And Candidate

```text
commit: e00f0fa8545ef4113e6b14d932b980c03f548b33
build ID: rc-20260804T132520Z-e00f0fa8545e
verification: artifacts/release/rc-20260804T132520Z-e00f0fa8545e/release-verification.json
variant used for installed E2E: ml
installer: artifacts/release/rc-20260804T132520Z-e00f0fa8545e/ml/nsis/AgentTheSpire_0.1.0_x64-setup.exe
```

## Acceptance Criteria

- [x] 候选绑定干净提交 `e00f0fa8`，baseline 与 ML 共 10/10 步成功。
- [x] baseline/ML BuildInfo 和 release manifest 验证通过。
- [x] 4/4 installer 大小与 SHA-256 独立复算一致，候选 staging/temp/transaction 残留为 0。
- [x] 明确记录四个 installer 均为 `NotSigned`，不把未签名描述为发布就绪。
- [x] 安装版报告 `ml` variant、正确 Build ID、`sts2` Pack 和 9 个 Features。
- [x] 使用与 Pack v3 identity 匹配的全新 verified Truth；不复用旧 Truth identity。
- [x] 创建全新 `custom_code` ItemDefinition，并绑定精确 definition hash。
- [x] Batch v4 的 parent/Plan/Single child Runs 全部为 RunRecord v3 `succeeded`。
- [x] Complex v3 的 parent/Batch/Plan/Single/Build/Package Runs 全部为 RunRecord v3 `succeeded`。
- [x] 真实 dotnet publish 与 Godot PCK 导出成功，Package ZIP 内容、大小和 SHA-256 可复算。
- [x] ArtifactManifest v3、生成文件、snapshot/published 文件和 provenance 可复算，无本轮 `.staging-*`。
- [x] 正常退出后工程 OS lock 可重新获取并释放。
- [x] 临时 stub 停止，用户 Provider 配置和工程 `local.props` 字节级恢复；真实 E2E 证据保留。
- [x] 用户于 2026-08-04 明确确认本次安装态结果验收。

## Installed E2E Evidence

### Pack, Truth And Item

```text
Truth snapshot: f379cdf78b586f3eb032f01f6d506da1c5f8bcee71ba261db350a001a19db818
Pack schema: v3
Pack SHA-256: 5bb8158fdfd1f05c92af5cf5110e4a2ceed8ab73a086dfa876429b97756ecc03
Truth sources/indexes: 2 / 2
Truth staging residue: 0
Item ID: installed-rc-e00f0fa8-20260804-2253
Item type: custom_code
definition hash: 6164b28f12292d99a6d2bfad2ee8ea13c6a60c075cc331a182472eec2aa00baa
```

### Batch v4

```text
parent: run-000000000000000018c8a1eca7f8fe78-00000000
plan: run-000000000000000018c8a1ecabfb05c0-00000001
single: run-000000000000000018c8a1ecf567f628-00000002
status: all succeeded
manifest SHA-256: 05e50988ec2db788fb13dc1d9290ef752d634f5e122f3afb056165bc45f2a363
generated file SHA-256: 206786ca2e3174c408c390fa1a60b55b9e8cf00773533c387c9a320a1b228512
snapshot/published: byte-identical
```

### Complex v3, Build And Package

```text
complex: run-000000000000000018c8a23e06251350-00000003
batch: run-000000000000000018c8a23e0a834778-00000004
plan: run-000000000000000018c8a23e0a866200-00000005
single: run-000000000000000018c8a23e52c1b3bc-00000006
build: run-000000000000000018c8a23eeddb30d0-00000007
package: run-000000000000000018c8a240b92aa990-00000008
status: all succeeded
package manifest SHA-256: 49ffa93ecae8f58f8566859d7c127167edac4911f2bec622ab9a23ce90c74036
ZIP bytes: 894049
ZIP SHA-256: 868a539bbe5146396c870b8dd4fb53708326ffcbbe035afa45a52c8fdc599133
ZIP entries: 6 exact BaseLib/ATSReleaseSmoke DLL/PCK/JSON files
Artifact staging residue: 0
project/package transaction children: 0
```

Preserved evidence:

```text
.tmp/real-game-smoke/20260801-2326/projects/ATSReleaseSmoke/artifacts/installed-rc-e00f0fa8-20260804-2253/
.tmp/real-game-smoke/20260801-2326/projects/ATSReleaseSmoke/artifacts/installed-rc-e00f0fa8-package-20260804-2258/
.tmp/real-game-smoke/20260801-2326/projects/ATSReleaseSmoke/packages/installed-rc-e00f0fa8-20260804-2258.zip
artifacts/e2e/installed-rc-20260804T132520Z-e00f0fa8545e-20260804T144539Z/stub-requests.jsonl
```

## Explicit Boundaries

- 未调用真实外部 Model/Media Provider；本轮只证明 provider-neutral 装配、stub 协议和本地生成链路。
- 未验收 AI 图片生成质量或 Resource Workbench 的真实图片 Provider 体验。
- 未在真实 STS2 游戏中加载并检查 Mod 行为；dotnet publish、PCK、Package 成功不等同于游戏行为正确。
- 四个安装器均未签名，未验证签名、SmartScreen、自动发布或升级安装。
- 历史工程仍有 `Generated/Release_Spark.cs(29,59)` 的 `CS8602` warning；Build 退出码为 0，本轮没有借机修改历史文件。

## Completion

用户已于 2026-08-04 明确确认本次安装态验收。任务可以完成并使用 `task.py archive --no-commit` 归档；Git push、rebase、发布继续保持独立授权门禁。
