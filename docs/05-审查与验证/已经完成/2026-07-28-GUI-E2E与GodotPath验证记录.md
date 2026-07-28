# 2026-07-28 - GUI E2E 与 GodotPath 验证记录

> 文档定位：记录 `GodotPath` 产品配置链、`local.props` 受控同步和真实 Tauri GUI 主链的验收证据。
>
> 事实依据：2026-07-28 在 Windows、真实 Godot 4.5.1、真实 .NET 与 STS2 DLL 环境执行的隔离 E2E 和定点检查。
>
> 权威入口：当前状态见 [`2026-07-27-当前进度说明`](../../02-现状/2026-07-27-当前进度说明.md)；后续事项见 [`统一后续任务清单`](../../03-方案/2026-07-28-统一后续任务清单.md)。
>
> 当前状态：实现与自动验证完成，代码尚未提交。
>
> 最后更新：2026-07-28

## 1. 验证范围

本轮覆盖以下真实桌面链路：

```text
Settings GUI
  -> Tauri settings command
  -> config.json 持久化与进程内热替换
  -> 工程创建 / local.props 同步
  -> single_asset_plan
  -> asset_generate + compile gate
  -> 页面切换与 job 恢复
  -> build_project (dotnet publish + Godot export)
  -> package_project
```

LLM 与 image API 使用仅监听本机回环地址的确定性 stub；Tauri IPC、文件系统、.NET、STS2 DLL 和 Godot 4.5.1 均为真实调用。

## 2. 隔离边界

- `SPIREFORGE_CONFIG_PATH` 指向本轮临时 `config.json`。
- `SPIREFORGE_APP_DATA_ROOT` 指向本轮临时 app-data。
- 工程、`ModsPath`、package 输出和 stub 请求记录均位于同一临时根。
- runner 启动前检查派生目录没有逃逸临时根。
- 成功后自动删除临时根；失败时保留现场路径供诊断。
- E2E 路径通过 `ATS_E2E_GODOT_PATH`、`ATS_E2E_STS2_DLL_PATH` 或 ignored 本地配置注入，仓库不保存本机工具绝对路径。

## 3. GUI E2E 结果

执行方式：

```powershell
$env:ATS_E2E_GODOT_PATH='<Godot 4.5.1 executable>'
$env:ATS_E2E_STS2_DLL_PATH='<STS2 DLL>'
npm run test:e2e:gui
```

最终结果：

```text
3 passing (15.2s)
Spec Files: 1 passed, 1 total
```

通过场景：

1. 不存在的 Godot 文件被 GUI 拒绝，随后真实 4.5.1 路径保存成功；隔离 config 和新建工程 `local.props` 均持久化该值。
2. delayed plan 期间切换到 System 再返回 Dashboard，页面恢复同一任务并继续完成 asset generation；C#、双语本地化和三张运行时图片落盘，compile gate 通过。
3. 已有 `local.props` 的隔离 `ModsPath` 和自定义 `E2EMarker` 在 asset/build 前同步后仍保留。
4. GUI `build_project` 完成并在隔离 Mods 目录生成 DLL/PCK；GUI `package_project` 完成并生成 zip。
5. GUI 明确清空 Godot 路径后，空值写入隔离 config；后续 build 在创建 job 前返回可行动的“未配置”错误。
6. isolated `recent_projects.json` 记录临时工程，证明 recent projects 使用 E2E app-data root。

## 4. 定点检查

| 检查 | 结果 |
| --- | --- |
| `npm run test:frontend` | 4/4 通过 |
| `npx tsc --noEmit` | 通过 |
| E2E runner/stub/spec `node --check` | 通过 |
| `godot_toolchain`、`local_props`、`toolchain_config` | 通过 |
| 真实 Godot 4.5.1 校验回归 | 通过，约 0.77 秒 |
| `cargo check -p agentthespire-desktop` | 通过 |
| `cargo check -p agentthespire-desktop --features e2e` | 通过 |
| `git diff --check` | 通过 |

额外边界回归覆盖：

- `4.5.10` 不会被误识别为 `4.5.1`。
- `local.props` 中自闭合的托管字段会被替换，不会产生重复节点。
- Godot `--version` 的 stdout/stderr 在等待期间并发排空，不再因 pipe 缓冲写满稳定超时。

## 5. 已知限制

- `@wdio/tauri-service` 的 embedded provider 在 Windows 会打印“binary permissions 666”和“tauri-driver not found”诊断；实际 embedded session、3 个用例和真实 Tauri IPC 均正常通过。这是当前依赖的诊断噪声，不是外部 `tauri-driver` 前置条件。
- E2E 证明 PCK 由真实 Godot 生成并进入 package，但没有启动 Slay the Spire 2 验证 Mod 运行行为。
- 本轮图片 fixture 只用于确定性资源导入，不代表真实 image generation 的透明度或 ML rembg 质量。
- 未验证 baseline/ML 安装包、ML 首次 prewarm 和双进程工程锁。
