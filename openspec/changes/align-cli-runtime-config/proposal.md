## Why

当前工作站设置页展示的是应用保存到 `config.json` 的值，但 Claude CLI / Codex CLI 实际运行时还会继续读取用户主目录下的全局 CLI 配置，导致“界面显示值”和“实际生效值”发生分叉。这个问题已经影响到 `base_url`、`model`、认证信息与其他 CLI 运行参数，需要先把配置所有权和可见性边界固定下来，再进入实现。

## What Changes

- 为应用新增“CLI 运行时配置对齐”能力，要求 Claude CLI / Codex CLI 的运行配置以应用设置为主，不再被用户全局 CLI 配置静默覆盖。
- 为应用新增“设置生效视图”能力，让设置接口和设置面板能区分“已保存配置”“运行时生效配置”“检测到的外部覆盖来源”。
- 明确 Claude CLI 与 Codex CLI 的配置注入边界，包括 `model`、`base_url`、认证信息和关键执行参数。
- 约定本次变更的落地顺序为：先固定运行时配置归属，再补足设置可观测性，最后补定向验证与文档。

## Capabilities

### New Capabilities
- `cli-runtime-config-alignment`: 约定工作站在调用 Claude CLI / Codex CLI 时，应用设置如何成为运行时配置的主来源，以及如何处理用户全局 CLI 配置。
- `settings-runtime-observability`: 约定设置接口与设置面板如何展示保存值、生效值和外部覆盖信息，避免用户误判当前实际配置。

### Modified Capabilities
- 无

## Impact

- 后端 CLI 调用链路：`backend/llm/agent_backends/claude_cli.py`、`backend/llm/agent_backends/codex_cli.py`、共享子进程运行器与相关配置读取逻辑。
- 后端设置接口：`backend/routers/config_router.py` 及可能的配置 facade 返回结构。
- 前端设置面板与共享 API 类型：`frontend/src/components/SettingsPanel.tsx`、`frontend/src/shared/api/config.ts`。
- 运行与发布文档：工作站配置保存位置、debug 打包沿用设置、CLI 全局配置覆盖行为的说明文档。
