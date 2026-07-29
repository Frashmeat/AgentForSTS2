# 2026-04-28 后端 Prompt 声明式 Recipe 重构计划

> 文档定位：本文规划后端 AI 提示词构成逻辑的破坏性重构，目标是降低拼接复杂度，把 prompt 组合策略从业务代码中收口到声明式 Recipe。
>
> 事实依据：基于当前 `backend/app/shared/prompting/`、`backend/app/shared/resources/prompts/`、`backend/app/modules/codegen/`、`backend/app/modules/platform/runner/`、`backend/llm/` 与 2026-04-27 对当前 prompt 链路的静态审查。
>
> 权威入口：上级入口为 `docs/03-方案/后端专题/README.md`；当前接口口径见 `docs/02-现状/当前前后端接口文档.md`；协作规则见 `PROJECT_SPEC.md`。
>
> 当前状态：未开始。
>
> 最后更新：2026-04-28

## 1. 当前理解

后端当前 AI prompt 大体由三层组成：

1. `PromptLoader` 从 Markdown bundle 中读取模板，例如 `runtime_agent.platform_single_asset_server_user`、`codegen.asset_prompt`。
2. 各业务模块自行准备变量并调用模板渲染，例如 `PromptAssembler`、`single_asset_plan_handler`、`batch_custom_code_handler`。
3. `llm.custom_prompt` 由 `llm.prompt_builder.append_global_ai_instructions()` 在最终进入 text runner 或 agent runner 前追加。

这个结构已经避免了把大段 prompt 写死在代码里，但拼接逻辑仍然分散在多个模块中：

- 服务器文本方案链路在 `backend/app/modules/platform/runner/*_handler.py` 内组装输入、工作区摘要和知识变量。
- 真实代码生成链路在 `backend/app/modules/codegen/application/prompt_assembler.py` 内处理知识 fallback、构建策略、图片路径、本地项目结构说明。
- 全局自定义 prompt 由 `backend/llm/text_runner.py` 与 `backend/llm/agent_runner.py` 分别调用注入。
- Markdown bundle 负责模板正文，但不表达“这个场景需要哪些上下文块、顺序如何、缺失时如何处理”。

项目尚未上线，因此本轮允许破坏性变更，不需要为旧 bundle key、旧 fallback 路径或历史 prompt 变量长期保留兼容层。

## 2. 目标

本次重构目标是建立“声明式 Prompt Recipe 层”：

1. 业务代码只声明要渲染哪个场景，不再手写大段变量拼接。
2. Recipe 文件声明该场景使用哪些上下文块、模板、runner 类型和缺失策略。
3. 代码 provider 负责获取真实事实，例如知识包、服务器工作区摘要、上传资产信息、构建策略。
4. Markdown 模板只负责表达给模型看的文本，不负责决定上下文选择和拼接顺序。
5. 全局 `llm.custom_prompt` 注入统一收口，不再由 text runner 与 agent runner 分散处理。

目标状态示意：

```text
业务 handler / service
  -> PromptRecipeRenderer.render("server.single_asset.plan", input)
     -> Recipe
        -> Context Providers
        -> Template Renderer
        -> PromptEnvelope
  -> TextRunner / AgentRunner
```

## 3. 不纳入范围

本计划不同时解决以下问题：

- 不重新设计 STS2 / BaseLib 知识库真源。
- 不新增第二个游戏或第二个领域。
- 不重做平台任务、队列、配额、凭据池模型。
- 不优化具体 prompt 文案质量，除非文案必须配合结构收口。
- 不保留旧 prompt key 的长期兼容入口。

## 4. 目标架构

建议新增或重组为以下结构：

```text
backend/app/shared/prompting/
  prompt_loader.py
  recipe_renderer.py
  recipe_models.py
  context_registry.py
  prompt_envelope.py
  recipes/
    server.single_asset.plan.yaml
    server.custom_code.plan.yaml
    codegen.asset.yaml
    codegen.custom_code.yaml
    codegen.asset_group.yaml
    planning.mod.yaml
    approval.actions.yaml
  templates/
    server_single_asset_plan.md
    server_custom_code_plan.md
    codegen_asset.md
    codegen_custom_code.md
    codegen_asset_group.md
    planning_mod.md
    approval_actions.md
```

如果实现时继续沿用 `backend/app/shared/resources/prompts/` 作为模板目录，也可以接受；关键是 Recipe 成为“组合策略真源”，而不是文件夹名称本身。

## 5. 核心概念

### 5.1 PromptRecipe

Recipe 描述一个 prompt 场景的组合策略。

示例：

```yaml
id: server.single_asset.plan
runner: text
template: server_single_asset_plan.md
contexts:
  - provider: input.single_asset_basic
    required: true
  - provider: input.uploaded_asset_metadata
    required: false
  - provider: server.workspace_snapshot
    required: false
  - provider: knowledge.sts2_asset
    required: true
missing_policy: fail_fast
global_instructions: append_runtime_custom_prompt
output_contract:
  first_line_prefix: "摘要："
```

Recipe 不直接读取文件系统、不调用 LLM、不理解 STS2 细节，只声明组合关系。

### 5.2 ContextProvider

Provider 是代码实现，负责把真实输入转换为模板变量。

第一阶段建议提供：

- `input.single_asset_basic`
- `input.custom_code_basic`
- `input.codegen_asset`
- `input.codegen_custom_code`
- `input.planning_requirements`
- `input.uploaded_asset_metadata`
- `server.workspace_snapshot`
- `knowledge.sts2_asset`
- `knowledge.sts2_custom_code`
- `knowledge.sts2_planner`
- `codegen.build_policy`
- `codegen.image_paths`
- `approval.requirements`

Provider 应该是窄职责函数或类，不负责最终文案。

### 5.3 PromptEnvelope

Envelope 负责统一表达最终发送给模型的 prompt 结构：

```python
PromptEnvelope(
    system_instructions=str | None,
    task_prompt=str,
    global_custom_instructions=str | None,
    runner="text" | "agent",
)
```

对 LiteLLM：

- `system_instructions + global_custom_instructions` 尽量进入 system message。
- `task_prompt` 进入 user message。

对 Claude CLI / Codex CLI：

- 降级拼成单个纯文本 prompt，但拼接格式由一个统一函数负责。

## 6. 关键设计决策

### 6.1 破坏性删除 legacy fallback

当前 `PromptAssembler._build_legacy_prompt_knowledge()` 这类兼容路径会增加理解成本。由于项目未上线，本次建议删除长期兼容逻辑：

- resolver 不可用时直接失败，或由 provider 返回显式的 `无可用事实`。
- 不再自动把旧 `docs / api_lookup` 兼容为新变量。
- 测试直接转向新 Recipe 输出，不再保护旧变量名。

### 6.2 Recipe 管组合，不管事实

Recipe 只描述需要哪些上下文块。事实获取仍由 Python provider 完成。

这样避免 YAML 变成伪代码，也避免重新把复杂度藏进配置文件。

### 6.3 模板正文继续外置

模板仍使用 Markdown，负责模型可读文本：

- 任务说明
- 输出格式
- 优先级规则
- 风险与边界
- 语言要求

模板不应该承担以下职责：

- 选择知识来源
- 扫描工作区
- 判断是否需要构建
- 判断是否需要图片

### 6.4 全局 custom prompt 统一注入

`llm.custom_prompt` 继续作为运行时全局提示词来源，但注入动作从 runner 中剥离到 Envelope 层。

目标是：

- `text_runner` 不再调用 `build_text_prompt()` 去追加全局提示词。
- `agent_runner` 不再调用 `build_agent_prompt()` 去追加全局提示词。
- runner 只负责执行，不负责 prompt 策略。

### 6.5 Recipe key 替代旧 bundle key

业务代码不再直接调用：

```python
PromptLoader().render("runtime_agent.platform_single_asset_server_user", variables)
```

而是调用：

```python
prompt = renderer.render("server.single_asset.plan", input_payload)
```

旧 bundle key 不再作为业务入口。

## 7. 迁移范围

### 7.1 第一批必须迁移

优先迁移服务器模式最混乱、最能验证价值的链路：

1. `single.asset.plan`
   - 当前入口：`backend/app/modules/platform/runner/single_asset_plan_handler.py`
   - 目标 Recipe：`server.single_asset.plan`
2. `batch.custom_code.plan`
   - 当前入口：`backend/app/modules/platform/runner/batch_custom_code_handler.py`
   - 目标 Recipe：`server.custom_code.plan`
3. `code.generate`
   - 当前入口：`backend/app/modules/platform/runner/code_generate_handler.py`
   - 目标 Recipe：`codegen.custom_code`
4. `asset.generate`
   - 当前入口：`backend/app/modules/platform/runner/asset_generate_handler.py`
   - 目标 Recipe：`codegen.asset`

### 7.2 第二批迁移

1. `planning.mod`
   - 当前入口：`backend/app/modules/planning/application/services.py`
2. `codegen.asset_group`
   - 当前入口：`backend/app/modules/codegen/application/prompt_assembler.py`
3. `approval.actions`
   - 当前入口：`backend/approval/action_prompt.py`
4. `build.project`
   - 当前入口：`backend/app/modules/platform/runner/build_project_handler.py`

### 7.3 第三批清理

1. 删除或瘦身 `PromptAssembler`
2. 合并或迁移 `runtime_agent.md`、`codegen.md` 中剩余旧 key
3. 清理 README 中过期的 prompt bundle 描述
4. 更新接口文档中的 prompt 行为说明

## 8. 分阶段计划

### 阶段一：建立 Recipe 基础设施

目标：不改业务行为，先建立新渲染入口。

任务：

- 新增 `recipe_models.py`
- 新增 `recipe_renderer.py`
- 新增 `context_registry.py`
- 新增 `prompt_envelope.py`
- 新增 Recipe 加载测试
- 新增固定输入的 prompt 渲染快照或关键片段断言

验收：

- 能通过 Recipe id 渲染一个最小 prompt。
- 缺 required provider 时 fail fast。
- 未注册 provider 时给出明确错误。
- 全局 `llm.custom_prompt` 可被 Envelope 层追加。

### 阶段二：迁移服务器文本方案链路

目标：把 `single_asset_plan_handler` 与 `batch_custom_code_handler` 的 prompt 拼接挪出 handler。

任务：

- 新增 `server.single_asset.plan` Recipe 与模板。
- 新增 `server.custom_code.plan` Recipe 与模板。
- 将 `render_server_workspace_snapshot()` 包装为 provider。
- 将 STS2 知识 resolver 包装为 provider。
- 修改两个 handler，只保留输入校验、step 转发和结果摘要。

验收：

- `single_generate/card`、`single_generate/relic`、`single_generate/power`、`single_generate/character` 仍能生成服务器文本方案。
- `batch_generate/custom_code` 仍能生成文本实现方案。
- prompt 中仍包含工作区摘要、知识事实、输出格式要求。

### 阶段三：迁移真实代码生成链路

目标：将 `PromptAssembler` 的资产代码生成与 custom_code 生成迁入 Recipe。

任务：

- 新增 `codegen.asset` Recipe 与模板。
- 新增 `codegen.custom_code` Recipe 与模板。
- 新增 `codegen.asset_group` Recipe 与模板。
- 将图片路径、构建策略、api ref path、project root、mod name 等变量拆成 provider。
- 删除 `PromptAssembler._build_legacy_prompt_knowledge()`。
- 逐步减少 `PromptAssembler` 职责，最终只保留兼容入口或直接删除。

验收：

- `asset.generate` 可渲染有图片路径的代码生成 prompt。
- `code.generate` 可渲染 custom_code prompt。
- prompt 明确包含 `Code Facts > Rules And Guidance > Further Lookup`。
- skip build 与 build steps 由 provider 或 recipe 输出，不再散落在多个方法中。

### 阶段四：统一 LLM runner 注入策略

目标：runner 只执行，不再负责 prompt 拼接策略。

任务：

- 将 `append_global_ai_instructions()` 的调用迁移到 Recipe renderer / Envelope 层。
- 修改 `text_runner.complete_text()`、`text_runner.stream_text()`，让其接收已完成的 prompt 或 message envelope。
- 修改 `agent_runner.run_agent_task_with_llm_config()`，移除内部 custom prompt 追加。
- 更新 `test_text_runner_selection.py`、`test_prompt_builder.py` 的断言。

验收：

- `llm.custom_prompt` 空白时不改变 prompt。
- `llm.custom_prompt` 非空时统一出现在 Envelope 指定位置。
- LiteLLM 与 CLI Agent 的降级拼接格式明确且可测试。

### 阶段五：文档与旧结构清理

目标：删除旧 prompt 口径，更新文档事实。

任务：

- 更新 `docs/02-现状/当前前后端接口文档.md` 中 `llm.custom_prompt` 与服务器生成链路说明。
- 更新 `README.md` 中 runtime prompts 列表。
- 更新或归档 `2026-03-27-硬编码Prompt与知识文本模板化改造计划.md` 的旧阶段描述。
- 在本计划中记录实际落地差异。

验收：

- 文档中不再说运行时 prompt bundle 是 `planning.md / approval.md / llm.md` 这一旧口径。
- 文档明确 Recipe 是 prompt 组合策略真源。
- 旧 bundle key 若被删除，文档同步说明。

## 9. 影响范围

### 9.1 后端代码

高影响文件：

- `backend/app/shared/prompting/prompt_loader.py`
- `backend/app/modules/platform/runner/single_asset_plan_handler.py`
- `backend/app/modules/platform/runner/batch_custom_code_handler.py`
- `backend/app/modules/platform/runner/code_generate_handler.py`
- `backend/app/modules/platform/runner/asset_generate_handler.py`
- `backend/app/modules/codegen/application/prompt_assembler.py`
- `backend/app/modules/planning/application/services.py`
- `backend/approval/action_prompt.py`
- `backend/llm/prompt_builder.py`
- `backend/llm/text_runner.py`
- `backend/llm/agent_runner.py`

### 9.2 测试

需要重点更新：

- `backend/tests/test_prompt_loader.py`
- `backend/tests/test_prompt_builder.py`
- `backend/tests/test_text_runner_selection.py`
- `backend/tests/test_codegen_module.py`
- `backend/tests/test_planning_module.py`
- `backend/tests/platform/runner/test_single_asset_plan_handler.py`
- `backend/tests/platform/runner/test_batch_custom_code_handler.py`
- `backend/tests/platform/runner/test_code_generate_handler.py`
- `backend/tests/platform/runner/test_asset_generate_handler.py`

建议新增：

- `backend/tests/test_prompt_recipe_renderer.py`
- `backend/tests/test_prompt_context_registry.py`
- `backend/tests/test_prompt_envelope.py`

### 9.3 文档

必须更新：

- `README.md`
- `docs/02-现状/当前前后端接口文档.md`
- `docs/03-方案/后端专题/README.md`

视实际落地更新：

- `docs/03-方案/后端专题/进行中/2026-03-27-硬编码Prompt与知识文本模板化改造计划.md`
- `docs/03-方案/后端专题/进行中/2026-04-21-知识库代码优先与跨游戏Prompt组装改造方案.md`

## 10. 风险

### 10.1 Prompt 输出漂移

即使业务逻辑不变，模板拆分和上下文重排也会改变模型行为。

缓解：

- 对核心场景建立关键片段断言。
- 保留人工可读的 render debug 输出。
- 第一阶段只迁移两条服务器文本方案链路，确认结构后再迁移真实写代码链路。

### 10.2 YAML 复杂化

如果 Recipe 试图表达条件分支、字段转换和事实查询，配置文件会变成另一种代码。

缓解：

- Recipe 只允许声明 provider 列表、模板、runner、缺失策略。
- 条件逻辑留在 provider 中。

### 10.3 Provider 粒度失控

Provider 太粗会回到旧 assembler，太细会导致 recipe 过长。

缓解：

- 第一版 provider 按稳定语义切分，而不是按字段切分。
- 每个 provider 输出一个明确变量包。

### 10.4 Runner 行为差异

LiteLLM 支持 system/user message，CLI Agent 通常只能吃单段 prompt。

缓解：

- 用 Envelope 做统一表达。
- 对 CLI 明确降级格式，避免各 runner 自行拼接。

### 10.5 与知识库代码优先方案重叠

`2026-04-21-知识库代码优先与跨游戏Prompt组装改造方案.md` 已经规划了知识 query、facts、guidance、lookup 分层。

缓解：

- 本计划不重写知识 provider。
- 本计划只接管 prompt 组合策略。
- 知识 provider 输出仍沿用 `facts / guidance / lookup / knowledge_warnings`。

## 11. 验收标准

整体完成标准：

- 业务代码不再直接调用旧 bundle key 拼大 prompt。
- 核心场景都通过 Recipe id 渲染 prompt。
- Recipe 是 prompt 组合策略的唯一入口。
- `llm.custom_prompt` 注入位置统一且可测试。
- 缺必要输入时 fail fast，不生成残缺 prompt。
- 文档同步更新当前 prompt 架构和运行时行为。

第一阶段最小验收：

- `server.single_asset.plan` 与 `server.custom_code.plan` 已迁移到 Recipe。
- 两条链路的 handler 不再直接调用 `PromptLoader.render()` 组装完整 prompt。
- 对应测试覆盖工作区摘要、上传资产元数据、知识变量和输出格式要求。

## 12. 建议验证命令

阶段一和阶段二建议先跑定向测试：

```powershell
python -m pytest `
  backend/tests/test_prompt_recipe_renderer.py `
  backend/tests/test_prompt_context_registry.py `
  backend/tests/platform/runner/test_single_asset_plan_handler.py `
  backend/tests/platform/runner/test_batch_custom_code_handler.py -q
```

阶段三增加代码生成相关测试：

```powershell
python -m pytest `
  backend/tests/test_codegen_module.py `
  backend/tests/platform/runner/test_code_generate_handler.py `
  backend/tests/platform/runner/test_asset_generate_handler.py -q
```

阶段四增加 LLM runner 相关测试：

```powershell
python -m pytest `
  backend/tests/test_prompt_builder.py `
  backend/tests/test_text_runner_selection.py `
  backend/tests/test_agent_runner_selection.py -q
```

是否执行全量测试由人工确认；本计划不默认要求在每个阶段跑全量测试。

## 13. 推荐第一步

建议第一步先做“Recipe 基础设施 + 两条服务器文本方案链路”：

1. 新增 Recipe renderer、context registry、envelope。
2. 迁移 `server.single_asset.plan`。
3. 迁移 `server.custom_code.plan`。
4. 增加定向测试。
5. 人工对比渲染后的 prompt 是否仍符合预期。

这一步不触碰真实写代码链路，风险可控；同时能验证 Recipe 是否真的降低复杂度。
