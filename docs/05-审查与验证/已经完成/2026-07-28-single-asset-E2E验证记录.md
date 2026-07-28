# 2026-07-28 single asset E2E 验证记录

> 文档定位：记录 Rust 桌面核心服务链第一次真实 single asset E2E 的范围、证据、结论和阻断项。
>
> 事实依据：2026-07-28 使用当前 `runtime/agentthespire.config.json` 中的真实 LLM、image generation 和 STS2 路径配置执行；敏感字段未写入报告。
>
> 权威入口：当前进度见 [`2026-07-27-当前进度说明`](../../02-现状/2026-07-27-当前进度说明.md)，后续处置见 [`2026-07-28-统一后续任务清单`](../../03-方案/2026-07-28-统一后续任务清单.md)。
>
> 当前状态：已经完成；结论为“真实 build/package 能通过，但自动 codegen 产物仍不可编译，不能视为主链验收通过”。
>
> 最后更新：2026-07-28

## 1. 验证范围

本次通过一次性、Git 忽略的 Rust 驱动直接调用 `ats-core` 公共服务，执行：

```text
ProjectFolder::create
  -> single_asset_plan（真实 LLM）
  -> asset_generate（真实图片 API + 背景处理 + 真实 LLM 代码生成）
  -> build_project（dotnet publish）
  -> package_project（zip）
```

只生成一个最小遗物，未执行 GUI 自动化、全量 workspace 测试、生产 Tauri 构建或安装版验证。为隔离变量，后续在 `.tmp` 工程中人工修正生成源码并补齐本地化，再通过正式 build/package job 验证剩余链路。

## 2. 执行结果

| 阶段 | 结果 | 证据 |
| --- | --- | --- |
| 工程创建 | 通过 | 工程脚手架、`project.json`、`local.props` 和标准目录均已生成 |
| `single_asset_plan` | 通过 | 真实 LLM 返回可解析 `PlanItem`，落盘 `items/e2e_energy_seed_relic.json` |
| `asset_generate` | 通过 | 原图、rembg 图、原始模型输出、artifact C# 和 `Generated/` C# 均已落盘 |
| history | 通过 | 最终 7 条 job，包含成功、失败和一次 LLM 空输出重试，终态均可追溯 |
| audit | 通过 | 最终 21 条生命周期事件，与 7 条 job 的 submitted/started/terminal 状态一致 |
| `dotnet publish` | 条件通过 | 原始生成源码失败；人工修正 C# 和本地化后，正式 `build_project` completed，DLL/PCK 均生成 |
| `package_project` | 通过 | completed；6 个文件、4,166,462 未压缩字节生成 3,429,609 字节 zip |
| Git 隔离 | 通过 | 驱动和运行产物均位于 `.tmp/`，由 `.gitignore` 排除 |

脱敏运行报告保留于 `.tmp/e2e-runs/1785207818-17000/report.json`。该目录只用于本机追溯，不属于仓库正式资料。

## 3. 已确认问题

### 3.1 P0：Godot 路径没有进入可配置的工程创建链

`local.props` 已根据 `knowledge.sts2_dll_path` 正确写入 `J:\SteamLibrary\steamapps`，但 Godot 路径仍沿用模板固定默认值。当前机器不存在该可执行文件，因此 MSBuild 的 `CheckDependencyPaths` 在实际 C# 编译前终止。

用户随后提供了 `I:\Godot`。已确认其中的 `Godot_v4.5.1-stable_win64.exe` 为官方 4.5.1，并在临时 E2E 工程的 `local.props` 中覆盖为真实可执行文件；`ModsPath` 同时重定向到 `.tmp`，没有写入真实游戏 Mods 目录。路径覆盖生效，构建成功越过 `CheckDependencyPaths`。

环境路径已经找到，但产品配置链仍没有保存和生成 `GodotPath` 的字段；当前验证使用的是临时人工覆盖。尽管该文件名不是 Mono/.NET 版，实际 headless export 已成功生成 PCK，因此这不再是当前阻断。

### 3.2 P0：生成 C# 与当前知识库 API 不一致

生成文件使用了 `BaseLib.Attributes`、`OnCombatStart()`、`EnergyGainCmd.Gain(Owner, 1)` 和 `Image` override。当前本地知识库显示：

- `PoolAttribute` 位于 `BaseLib.Utils`；
- `AbstractModel` 的战斗开始 hook 是 `BeforeCombatStart()`；
- 当前 `RelicModel` 图像约定使用 `PackedIconPath`、`PackedIconOutlinePath` 和 `BigIconPath`。

补充真实 Godot 路径后，`dotnet publish` 已进入编译并确认 6 个错误：

- `CS0234`：`BaseLib.Attributes` 不存在；
- 两个 `CS0246`：`PoolAttribute` / `Pool` 无法解析；
- 两个 `CS0115`：`Image` 与 `OnCombatStart()` 没有可重写成员；
- `CS0534`：没有实现抽象成员 `RelicModel.Rarity`。

人工按当前 Code Facts 修正源码并补齐本地化后，`dotnet build --no-restore` 达到 0 warning / 0 error，随后正式 `build_project` completed。这证明模板、BaseLib、Harmony、STS2 和 Godot 依赖链可工作，自动生成内容是编译失败的直接原因。

又使用 GUI 主链等价的小写 `asset_type = "relic"` 做了一次只调用 LLM、不重复生图的探针：

- 小写类型能够命中 `BaseLib.Utils`、`Pool` 和 `Rarity` guidance；大写 `Relic` 不会命中类型 guidance，公共字符串契约缺少归一化。
- 第一次相同请求返回空流并 failed 为 `model produced no code (empty output)`；第二次成功，说明空响应具有瞬时性，而业务层不会在该错误上重试。
- 第二次生成仍使用不存在的 `OnCombatStart(IRunState)`，并漏掉 `RelicRarity` using；job 状态是 completed，但定向构建稳定得到 2 个编译错误。
- prompt 要求模型读取 `MainFile.cs` / API reference，但普通 LLM 请求没有文件工具，也没有内联真实工程 namespace 或正确战斗开始 hook。

### 3.3 P0：本地化输出契约不可执行

asset prompt 同时要求创建英/中本地化文件，并强制“只输出一个完整 C# fence”。实际模型把双语 JSON 放在 C# fence 之后，但 handler 只提取第一个 C# fence并写入 `.cs`；本地化不会落盘，且模型生成的 key 缺少分析器要求的 Mod ID 前缀。人工修正 C# 后，构建下一层即由 `STS001` 确认缺少 `.title/.description/.flavor`。

因此这不是单纯 prompt 文案问题：当前 `asset_generate` 的结果模型和落盘逻辑没有表达多文件资产的能力。

### 3.4 P0：图片透明背景是假透明

原图是 1024×1024、24bpp RGB PNG，模型把棋盘格直接画进了图像像素。simple rembg 虽输出 32bpp ARGB，但抽样像素 alpha 仍为 255，棋盘格没有被移除。因此文件格式和 job 状态成功，不等于图像可以直接作为透明游戏图标使用。

### 3.5 P0：PCK 包含内部工程资料

Godot 的 `BasicExport` 使用 `export_filter="all_resources"` 和 `include_filter="*.json"`。真实导出日志确认 PCK 除图像、本地化和 Godot 元数据外，还包含：

- `artifacts/` 原始/处理图片；
- `history/` 全部 job JSON；
- `items/` 规划 JSON；
- `packages/` 中的 BaseLib JSON；
- `project.json`。

最终 PCK 约 3.01 MB。history、items 和 project metadata 不应进入交付包，既增加体积，也扩大内部信息暴露面。

## 4. 结论与下一次验收

核心 job 编排、真实外部模型调用、DLL/PCK 生成、zip、history 和 audit 均已得到真实证据。人工正确夹具可以完整 build/package，但自动 codegen 产物仍不可编译，因此 single asset 自动主链仍未通过；GUI 也未验证。

下一次验收应按以下顺序执行：

1. 把 asset codegen 收口为可验证的多文件结果：类型归一化、内联必要 API/namespace、C#、本地化和资源路径统一落盘，并在 completed 前执行编译门禁。
2. 对空输出增加有限重试与清晰错误分类。
3. 收紧 Godot export filter，禁止 `history/`、`items/`、`artifacts/`、`packages/` 和 `project.json` 进入 PCK。
4. 把 `GodotPath` 纳入桌面设置和 `local.props` 生成链。
5. 调整图片 prompt/透明度检查，拒绝棋盘格假透明产物或启用有效的 ML rembg。
6. 修复后从桌面 GUI 重跑主流程和跨页面进度恢复。
