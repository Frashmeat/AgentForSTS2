# 2026-07-28 single asset E2E 验证记录

> 文档定位：记录 Rust 桌面核心服务链第一次真实 single asset E2E 的范围、证据、结论和阻断项。
>
> 事实依据：2026-07-28 使用当前 `runtime/agentthespire.config.json` 中的真实 LLM、image generation 和 STS2 路径配置执行；敏感字段未写入报告。
>
> 权威入口：当前进度见 [`2026-07-27-当前进度说明`](../../02-现状/2026-07-27-当前进度说明.md)，后续处置见 [`2026-07-28-统一后续任务清单`](../../03-方案/2026-07-28-统一后续任务清单.md)。
>
> 当前状态：已经完成；结论为“核心生成链可运行，完整工程构建仍被环境和产物质量问题阻断”。
>
> 最后更新：2026-07-28

## 1. 验证范围

本次通过一次性、Git 忽略的 Rust 驱动直接调用 `ats-core` 公共服务，执行：

```text
ProjectFolder::create
  -> single_asset_plan（真实 LLM）
  -> asset_generate（真实图片 API + 背景处理 + 真实 LLM 代码生成）
  -> build_project（dotnet publish）
```

只生成一个最小遗物，未执行 GUI 自动化、package、全量 workspace 测试、生产 Tauri 构建或安装版验证。

## 2. 执行结果

| 阶段 | 结果 | 证据 |
| --- | --- | --- |
| 工程创建 | 通过 | 工程脚手架、`project.json`、`local.props` 和标准目录均已生成 |
| `single_asset_plan` | 通过 | 真实 LLM 返回可解析 `PlanItem`，落盘 `items/e2e_energy_seed_relic.json` |
| `asset_generate` | 通过 | 原图、rembg 图、原始模型输出、artifact C# 和 `Generated/` C# 均已落盘 |
| history | 通过 | 3 条 job：规划 completed、资产 completed、构建 failed |
| audit | 通过 | 9 条生命周期事件，与 3 条 job 的 submitted/started/terminal 状态一致 |
| `dotnet publish` | 阻断 | exit code 1；`GodotPath` 指向不存在的 `C:/megadot/MegaDot_v4.5.1-stable_mono_win64.exe` |
| Git 隔离 | 通过 | 驱动和运行产物均位于 `.tmp/`，由 `.gitignore` 排除 |

脱敏运行报告保留于 `.tmp/e2e-runs/1785207818-17000/report.json`。该目录只用于本机追溯，不属于仓库正式资料。

## 3. 已确认问题

### 3.1 P0：Godot 路径没有进入可配置的工程创建链

`local.props` 已根据 `knowledge.sts2_dll_path` 正确写入 `J:\SteamLibrary\steamapps`，但 Godot 路径仍沿用模板固定默认值。当前机器不存在该可执行文件，因此 MSBuild 的 `CheckDependencyPaths` 在实际 C# 编译前终止。

这是本次 `dotnet publish` 的首个可行动阻断。需要先安装匹配的 Godot/MegaDot 4.5.1，或让桌面设置能够保存并写入真实 `GodotPath`，然后重跑构建。

### 3.2 P0：生成 C# 与当前知识库 API 不一致

生成文件使用了 `BaseLib.Attributes`、`OnCombatStart()`、`EnergyGainCmd.Gain(Owner, 1)` 和 `Image` override。当前本地知识库显示：

- `PoolAttribute` 位于 `BaseLib.Utils`；
- `AbstractModel` 的战斗开始 hook 是 `BeforeCombatStart()`；
- 当前 Relic 图像约定使用 `CustomPackedIconPath` 等属性。

由于构建先被 Godot 路径检查拦截，本次尚未得到编译器对这些差异的完整错误列表，但产物已经不满足“依据当前 Code Facts 生成可编译代码”的质量要求。

### 3.3 P0：图片透明背景是假透明

原图是 1024×1024、24bpp RGB PNG，模型把棋盘格直接画进了图像像素。simple rembg 虽输出 32bpp ARGB，但抽样像素 alpha 仍为 255，棋盘格没有被移除。因此文件格式和 job 状态成功，不等于图像可以直接作为透明游戏图标使用。

## 4. 结论与下一次验收

核心 job 编排、真实外部模型调用、产物落盘、history 和 audit 已得到第一轮真实证据，不再属于“完全未验证”。完整主链路仍未通过，package 和 GUI 也未验证。

下一次验收应按以下顺序执行：

1. 配置有效的 Godot/MegaDot 4.5.1 路径并重跑 `dotnet publish`，取得真实编译错误。
2. 修复 codegen 的知识约束与生成后校验，直至最小遗物 C# 编译通过。
3. 调整图片 prompt/透明度检查，拒绝棋盘格假透明产物或启用有效的 ML rembg。
4. 构建通过后继续 package，再从桌面 GUI 重跑主流程和跨页面进度恢复。
