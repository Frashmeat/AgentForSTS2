# Prompt Bundle Consolidation 进度说明

更新时间：2026-03-30

## 当前结论

`backend/app/shared/resources/prompts/*.md` 已经成为主要运行时 Prompt 真源；
`planning`、`approval`、`llm`、`analyzer`、`codegen`、`image` 六条核心调用链都已切到共享 `PromptLoader` + `bundle.key` 读取方式。
当前工作树中的旧 Prompt `.txt` 资源已删除，且本轮已补齐零旧引用扫描与关键定点验证。
按当前证据看，这个事项已经满足技术完成条件。

## 当前阶段判断

当前阶段应定义为：`Prompt Bundle Consolidation 已达到封板条件`。

这个阶段的特点是：

- 主体迁移已经完成，运行时真源与调用链收口已经落地
- 旧 `.txt` 文件在当前工作树中已删除，不再是继续迁移的阻塞项
- 剩余问题集中在最终一致性校准，而不是新增功能开发
- 当前剩余重点不再是技术迁移，而是把完成结论写清楚并归档

## 已完成事项

### 1. 共享 Prompt 真源已建立

已建立并接入以下 Markdown bundle：

- `backend/app/shared/resources/prompts/planning.md`
- `backend/app/shared/resources/prompts/approval.md`
- `backend/app/shared/resources/prompts/llm.md`
- `backend/app/shared/resources/prompts/analyzer.md`
- `backend/app/shared/resources/prompts/codegen.md`
- `backend/app/shared/resources/prompts/image.md`

所有 bundle 已统一为设计文档要求的 `## 键名` 结构，不再使用早期的 `<!-- PROMPT_BUNDLE:START/END -->` 哨兵格式。

### 2. PromptLoader 已完成核心改造

`backend/app/shared/prompting/prompt_loader.py` 已支持：

- `bundle.key` 方式读取 Prompt
- Markdown `## 键名` 解析
- 非法键名、重复键、空片段校验
- fenced code block 内伪标题忽略
- bundle 缓存

当前源码目录中仅看到 `prompt_loader.py` 与 `__init__.py`；
未见独立的 `legacy_prompt_aliases.py` 源文件，因此“兼容映射独立文件”这一表述不再适合作为当前状态描述。

### 3. 业务调用链迁移已完成

已切换到共享 bundle 的模块：

- `planning`
- `approval`
- `llm`
- `analyzer`
- `codegen`
- `image`

已完成的关键收口点包括：

- `backend/app/modules/planning/application/services.py`
- `backend/approval/action_prompt.py`
- `backend/llm/prompt_builder.py`
- `backend/routers/mod_analyzer.py`
- `backend/routers/log_analyzer.py`
- `backend/app/modules/codegen/application/prompt_assembler.py`
- `backend/agents/code_agent.py`
- `backend/image/prompt_adapter.py`

其中 `codegen` 的 `api_lookup` partial 也已收口到 `codegen.md`，不再从旧的 `api_lookup` 分散目录读取。

### 4. 回归测试已通过

已通过的核心测试覆盖：

- `test_prompt_loader.py`
- `test_planning_module.py`
- `test_action_prompt.py`
- `test_prompt_builder.py`
- `test_agent_runner_selection.py`
- `test_text_runner_selection.py`
- `test_mod_analyzer.py`
- `test_log_analyzer.py`
- `test_codegen_module.py`
- `test_code_agent_lookup.py`
- `test_image_prompt_adapter.py`

最近一次聚合验证结果：`98 passed`

本轮收尾阶段补充验证结果：

- `test_codegen_module.py`
- `test_code_agent_lookup.py`
- `test_image_prompt_adapter.py`

定点验证结果：`31 passed in 0.25s`

本轮其余迁移链路补充验证结果：

- `test_prompt_loader.py`
- `test_planning_module.py`
- `test_action_prompt.py`
- `test_prompt_builder.py`
- `test_mod_analyzer.py`
- `test_log_analyzer.py`
- `test_agent_runner_selection.py`
- `test_text_runner_selection.py`

定点验证结果：`66 passed in 0.29s`

## 当前剩余工作

### 1. 归档说明

当前已完成一轮定点扫描，结论如下：

- 运行时代码目录 `backend/app`、`backend/approval`、`backend/llm`、`backend/routers`、`backend/agents`、`backend/image` 中未扫描到旧 prompt `.txt` 文件名、旧 prompt 路径或 `PromptLoader(root=...prompts)` 形式的旧入口
- `backend/tests` 中未扫描到旧 prompt 文件名或旧资源路径依赖
- 剩余旧 `.txt` 名称主要存在于 `docs/superpowers/specs/2026-03-28-prompt-bundle-consolidation-design.md` 与 `docs/superpowers/plans/2026-03-28-prompt-bundle-consolidation.md`，属于历史设计/计划记录，不是运行时残留
- 本文档此前对 `legacy_prompt_aliases.py` 的引用属于文档滞后，现已修正

### 2. 最终封板说明

在设计文档定义的退役门槛里，本轮已经补齐了核心技术证据：

- 零旧引用扫描结论已经落地
- `image`、`codegen` 高风险路径定点验证已通过
- 其余迁移链路定点验证已通过

最终判断如下：

- 历史设计/计划文档中的旧文件名仅用于记录迁移过程，不计入运行时残留
- 本事项已满足完成条件，可以按已完成事项处理

## 下一阶段目标

### 目标 1：完成归档

- 把进度文档与总览文档维持在同一口径
- 明确历史设计/计划文档中的旧文件名只承担归档说明职责
- 将后续动作从“继续迁移”收敛为“归档与提交”

### 目标 2：完成退役闭环

- 以工作树现状为准，确认旧 `.txt` 删除后的运行时链路、测试链路均无反向依赖
- 如仍存在兼容桥或历史常量残留，明确其是否真实存在、是否必须继续保留
- 让“旧资源已删除”从文件状态结论升级为可审计结论

### 目标 3：完成封板验证

- 保留本轮定点测试结果作为最终收尾验证依据
- 对 `image`、`codegen` 两个高风险路径维持结论化说明
- 形成可以作为本事项完成判定的最终证据链

## 推荐的推进顺序

1. 保留当前扫描结果与两轮定点验证结果作为封板证据。
2. 将本事项按已完成状态归档。
3. 如后续还要继续推进，只处理提交、PR 或后续独立事项。

## 风险提示

- 当前最大风险已经不再是主链迁移，而是历史文档口径被误读为运行时残留。
- 如果后续继续改动历史计划/设计文档，需要避免把归档记录与当前实现状态混写。
- 当前技术风险已显著下降，剩余主要是收尾表述风险。
