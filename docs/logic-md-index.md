# 逻辑相关 Markdown 索引

日期：2026-04-02

## 说明

本索引按“与系统运行逻辑、AI 行为、业务规则、设计决策直接相关”筛选 Markdown 文件。

“运行时直接加载”列的含义：

- `是`：代码在运行时会直接读取该 `.md`
- `否`：主要用于设计、规划、审查、人工参考或测试说明

已排除的典型文件：

- 根目录 `README.md`
- `TUTORIAL.md`
- `tools/README.md`
- `AGENTS_CODEX.md`
- 第三方依赖、`node_modules`、`.venv`、`.worktrees` 下的 Markdown

## 1. 运行时 Prompt / 文本资源

| 路径 | 分类 | 运行时直接加载 | 作用 |
| --- | --- | --- | --- |
| `backend/app/shared/resources/prompts/analyzer.md` | 运行时资源 | 是 | 日志分析 / Mod 分析 prompt 与阶段消息 |
| `backend/app/shared/resources/prompts/codegen.md` | 运行时资源 | 是 | 代码生成与项目创建 prompt |
| `backend/app/shared/resources/prompts/image.md` | 运行时资源 | 是 | 图像 prompt、guide、fallback 文本 |
| `backend/app/shared/resources/prompts/runtime_agent.md` | 运行时资源 | 是 | 合并后的审批 / 规划 / LLM header bundle |
| `backend/app/shared/resources/prompts/runtime_system.md` | 运行时资源 | 是 | 合并后的配置测试 / 项目模板 / 路径探测 bundle |
| `backend/app/shared/resources/prompts/runtime_workflow.md` | 运行时资源 | 是 | 合并后的单资产 / 批量 / 构建部署消息 bundle |

## 2. 游戏知识库 / 规则文档

| 路径 | 分类 | 运行时直接加载 | 作用 |
| --- | --- | --- | --- |
| `backend/app/modules/knowledge/resources/sts2/card.md` | 知识库 | 是 | 卡牌建模、命名、本地化与实现规则 |
| `backend/app/modules/knowledge/resources/sts2/character.md` | 知识库 | 是 | 角色资产结构与实现规则 |
| `backend/app/modules/knowledge/resources/sts2/common.md` | 知识库 | 是 | 通用构建、项目结构、Harmony、陷阱说明 |
| `backend/app/modules/knowledge/resources/sts2/custom_code.md` | 知识库 | 是 | 自定义机制 / 代码型资产规则 |
| `backend/app/modules/knowledge/resources/sts2/planner_hints.md` | 知识库 | 是 | 规划阶段 API 提示压缩版 |
| `backend/app/modules/knowledge/resources/sts2/potion.md` | 知识库 | 是 | 药水实现规则 |
| `backend/app/modules/knowledge/resources/sts2/power.md` | 知识库 | 是 | Power / Buff 实现规则 |
| `backend/app/modules/knowledge/resources/sts2/relic.md` | 知识库 | 是 | 遗物实现规则 |

## 3. Agent / 分析参考文档

| 路径 | 分类 | 运行时直接加载 | 作用 |
| --- | --- | --- | --- |
| `backend/agents/sts2_api_reference.md` | Agent 参考 | 否 | STS2 反编译 API 参考，影响 codegen / planning |
| `backend/tests/scenarios.md` | 测试场景 | 否 | 集成场景说明，辅助验证业务流程 |

## 4. 当前仓库设计 / 决策 / 重构文档

| 路径 | 分类 | 运行时直接加载 | 作用 |
| --- | --- | --- | --- |
| `docs/architecture.md` | 架构文档 | 否 | 当前系统架构概览 |
| `docs/当前后端前端接口文档.md` | 接口文档 | 否 | 当前前后端 HTTP / WebSocket 接口与前端消费映射 |
| `docs/当前项目结构报告.md` | 结构文档 | 否 | 项目结构与模块关系说明 |
| `docs/需求计划.md` | 需求文档 | 否 | 需求与实现目标记录 |
| `docs/refactor-verification-checklist.md` | 验证清单 | 否 | 重构后的检查项与验收点 |
| `docs/codex-approval-fix-plan.md` | 修复计划 | 否 | 审批流相关修复方案 |
| `docs/txt-files-overview.md` | 文本资源盘点 | 否 | 文本文件用途与分布说明 |
| `docs/2026-03-24-模块化解耦重构计划书.md` | 重构计划 | 否 | 模块化解耦方案 |
| `docs/2026-03-24-重构后项目分析.md` | 分析文档 | 否 | 重构后结构与问题分析 |
| `docs/2026-03-27-代码审查结论.md` | 审查文档 | 否 | 代码审查发现与结论 |
| `docs/2026-03-27-硬编码Prompt与知识文本模板化改造计划.md` | 改造计划 | 否 | 文本模板化迁移计划 |
| `docs/2026-03-30-prompt-bundle-consolidation-progress.md` | 进展记录 | 否 | prompt bundle consolidation 进展 |
| `docs/2026-03-30-hardcoded-text-cleanup.md` | 审查记录 | 否 | 本轮运行时硬编码文本清理总结 |
| `docs/前端计划/2026-04-01-前端稳定化四阶段计划.md` | 前端计划 | 否 | 前端稳定化四阶段总计划、阶段 4 当前停点与下一步重点 |
| `docs/前端计划/2026-04-01-阶段1-工作流会话层实施计划.md` | 前端计划 | 否 | 阶段 1 WebSocket 会话层实施计划 |
| `docs/前端计划/2026-04-01-阶段2-状态机收敛实施计划.md` | 前端计划 | 否 | 阶段 2 页面状态机收敛实施计划 |
| `docs/前端计划/2026-04-01-阶段3-异常恢复与可恢复性实施计划.md` | 前端计划 | 否 | 阶段 3 异常恢复与可恢复性实施计划 |
| `docs/前端计划/2026-04-01-阶段4-统一接口层实施计划.md` | 前端计划 | 否 | 阶段 4 统一接口层实施计划、接口消费映射与近期状态命名收口记录 |
| `docs/后端/2026-04-02-工作站端与Web端边界说明.md` | 架构文档 | 否 | 固化当前代码实现中的工作站端 / Web 端职责、入口、路由与部署边界 |
| `docs/后端/2026-04-02-新增接口与服务归属决策模板.md` | 架构文档 | 否 | 固化后续新增接口、服务、facade 与数据落库需求的归属判定模板 |

## 5. 后端规划集合

| 路径 | 分类 | 运行时直接加载 | 作用 |
| --- | --- | --- | --- |
| `docs/2026-03-27-后端规划集合/2026-03-27-仓储接口草案.md` | 后端规划 | 否 | 仓储接口边界设计 |
| `docs/2026-03-27-后端规划集合/2026-03-27-产品讨论纪要.md` | 后端规划 | 否 | 产品与技术约束讨论结论 |
| `docs/2026-03-27-后端规划集合/2026-03-27-后端设计草案.md` | 后端规划 | 否 | 后端总体设计草案 |
| `docs/2026-03-27-后端规划集合/2026-03-27-后端设计详情.md` | 后端规划 | 否 | 详细设计分解 |
| `docs/2026-03-27-后端规划集合/2026-03-27-数据库表设计草案.md` | 后端规划 | 否 | 数据表草案 |
| `docs/2026-03-27-后端规划集合/2026-03-27-api-service-split-migration-plan.md` | 后端规划 | 否 | API / service 拆分迁移方案 |
| `docs/2026-03-27-后端规划集合/2026-03-27-ORM模型草案.md` | 后端规划 | 否 | ORM 模型草案 |
| `docs/2026-03-27-后端规划集合/2026-03-27-SQL-DDL草案.md` | 后端规划 | 否 | SQL DDL 草案 |

## 6. Superpowers 计划 / 规格

| 路径 | 分类 | 运行时直接加载 | 作用 |
| --- | --- | --- | --- |
| `docs/superpowers/plans/2026-03-23-global-ai-prompt.md` | 计划文档 | 否 | 全局 AI prompt 计划 |
| `docs/superpowers/plans/2026-03-24-approval-failure-handling.md` | 计划文档 | 否 | 审批失败处理计划 |
| `docs/superpowers/plans/2026-03-24-no-approval-codex-plan.md` | 计划文档 | 否 | Codex 无审批流程计划 |
| `docs/superpowers/plans/2026-03-24-platform-approval-popup-recovery.md` | 计划文档 | 否 | 平台审批弹窗恢复方案 |
| `docs/superpowers/plans/2026-03-24-review-fixes.md` | 计划文档 | 否 | 审查问题修复计划 |
| `docs/superpowers/plans/2026-03-24-unified-ai-approval-module.md` | 计划文档 | 否 | 统一 AI 审批模块计划 |
| `docs/superpowers/plans/2026-03-26-compat-layer-cleanup-plan.md` | 计划文档 | 否 | 兼容层清理计划 |
| `docs/superpowers/plans/2026-03-28-prompt-bundle-consolidation.md` | 计划文档 | 否 | prompt bundle consolidation 计划 |
| `docs/superpowers/specs/2026-03-23-global-ai-prompt-design.md` | 规格文档 | 否 | 全局 AI prompt 设计 |
| `docs/superpowers/specs/2026-03-28-prompt-bundle-consolidation-design.md` | 规格文档 | 否 | prompt bundle consolidation 设计 |

## 建议维护规则

后续新增逻辑相关 `.md` 时，建议同时更新本索引，并优先归到以下四类之一：

- 运行时资源
- 知识库 / 规则
- 架构 / 计划 / 审查
- 测试场景 / 验证
