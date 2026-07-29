# 全栈重写（索引）

> 文档定位：本文是 Rust + Tauri 全栈重写专题索引，负责说明当前阶段口径、文档状态和下钻入口。
>
> 事实依据：基于 `rust` 分支当前 `crates/`、`src-tauri/`、`src/` 代码事实，以及 `进行中/` 下既有计划与冒烟清单整理。
>
> 权威入口：上级入口为 `docs/03-方案/README.md`；当前事实以 `docs/02-现状/2026-07-27-当前进度说明.md` 为准，阶段范围以本文和当前收口计划为准。
>
> 最后更新：2026-07-27

## 当前阶段口径

当前阶段定义为 **Rust/Tauri 桌面端 MVP 可交付收口**。

纳入本阶段：

- 桌面端工程文件夹闭环：创建/打开工程、recent projects、items/artifacts/history/audit 落盘。
- 桌面端 9 类 job 链路：`text_generate`、`single_asset_plan`、`code_generate`、`asset_generate`、`batch_custom_code`、`build_project`、`package_project`、`log_analysis`、`knowledge_refresh`。
- Settings 热替换、Health/Capabilities 自检、Audit 展示、ML rembg 状态与 `-MlRembg` 打包验证。
- Windows v3 冒烟关键项：真实端到端、`mod_template` + `dotnet publish`、prod build / installer、ML rembg 打包。
- 与上述范围相关的文档口径同步。

不纳入本阶段：

- Web auth / admin / role / users / tokens。
- Web sqlx / Postgres / migrations / platform jobs。
- new-api 管理集成、火山方舟原生 SDK 直连。
- 自动更新、签名公证、Mac/Linux 打包验证。
- Stage 8 真合 main。

## 当前入口

- [当前进度说明](../../../../02-现状/当前进度说明.md)：当前代码事实、验证证据与未验证项。
- [桌面端 MVP 收口计划](./进行中/2026-05-29-桌面端MVP收口计划.md)：本阶段范围与验收标准。
- [Rust 重写后续执行计划](./进行中/2026-05-11-Rust重写后续执行计划.md)：历史执行总账 + 当前能力快照。
- [Q 决议汇总](./进行中/2026-05-11-Q决议汇总.md)：Q1-Q10 与 N1-N5 架构决议。
- [Windows 冒烟测试清单 v3](./进行中/2026-05-20-Windows冒烟测试清单-v3.md)：本阶段关键验收清单。
- [Rust + Tauri 全栈重写计划](./进行中/2026-05-11-Rust+Tauri全栈重写计划.md)：战略层 ADR 与历史路线图，部分早期设想已被后续决议覆盖。

## 文档状态

| 文档 | 当前状态 | 使用方式 |
| --- | --- | --- |
| `2026-05-29-桌面端MVP收口计划.md` | 进行中 | 本阶段收口主入口 |
| `2026-05-20-Windows冒烟测试清单-v3.md` | 待验证 | 本阶段必验清单 |
| `2026-05-11-Rust重写后续执行计划.md` | 进行中 | 历史总账与能力快照，需结合收口计划阅读 |
| `2026-05-11-Q决议汇总.md` | 已决 | 决策依据；其中未实现项不等于本阶段任务 |
| `2026-05-12-Windows冒烟测试清单-v2.md` | 已被 v3 覆盖 | 保留用于追溯 session-2 fix |
| `2026-05-11-Windows冒烟测试清单.md` | 已被 v2/v3 覆盖 | 保留用于追溯第一轮冒烟 |
| `2026-05-11-Rust+Tauri全栈重写计划.md` | 战略历史口径 | 看架构背景；当前范围以收口计划为准 |

## 阅读顺序

1. 先读本文确认阶段边界。
2. 再读 [当前进度说明](../../../../02-现状/当前进度说明.md)，确认代码与验证的最新事实。
3. 按 [桌面端 MVP 收口计划](./进行中/2026-05-29-桌面端MVP收口计划.md) 和 [Windows 冒烟测试清单 v3](./进行中/2026-05-20-Windows冒烟测试清单-v3.md) 推进验收。
4. 需要追溯决策时读 [Q 决议汇总](./进行中/2026-05-11-Q决议汇总.md)。
