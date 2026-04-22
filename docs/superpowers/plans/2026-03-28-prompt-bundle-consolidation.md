# Prompt Bundle Consolidation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `codegen`、`image`、`analyzer`、`planning`、`approval`、`llm` 六组运行时 prompt 资源收敛到 `backend/app/shared/resources/prompts/*.md`，并让 Markdown bundle 成为唯一真实来源。

**Architecture:** 先建立六个 Markdown 真源文件，再扩展 `PromptLoader` 以支持 `bundle + key` 读取、旧路径桥接、结构校验与缓存。随后按低风险到高风险顺序逐模块切调用点，最后删除旧 `.txt` 并完成零旧引用验证与文档更新。

**Tech Stack:** Python, FastAPI, pytest, Markdown resource bundles, ripgrep

---

## Scope And Decisions

本计划固定采用以下执行决策：

- 统一真源目录固定为 `backend/app/shared/resources/prompts/`，不保留第二套可编辑运行时 prompt 真源。
- 六个 bundle 文件固定命名为 `codegen.md`、`image.md`、`analyzer.md`、`planning.md`、`approval.md`、`llm.md`。
- Markdown 运行时索引契约固定为 `## 键名`，键名字符集固定为 `^[a-z0-9_]+$`。
- 兼容期内允许旧调用方式继续存在，但它们必须通过中心化映射跳转到 Markdown 真源，禁止继续回读旧 `.txt` 内容。
- `llm` 的 `backend/llm/prompt_builder.py` 旁路直读必须被收口到统一加载入口，否则不允许进入旧文件删除阶段。
- 删除旧 `.txt` 之前，必须先完成 `image` 与 `codegen` 的关键输出快照或等价文本回归验证。

## File Map

### Unified Prompt Source

- `backend/app/shared/resources/prompts/codegen.md`
  新的 codegen prompt bundle，承载主模板与 `api_lookup` partial 键。
- `backend/app/shared/resources/prompts/image.md`
  新的 image prompt bundle，承载 `adapt`、`guide`、`fallback`、透明背景规则键。
- `backend/app/shared/resources/prompts/analyzer.md`
  新的 analyzer prompt bundle，承载 mod/log analyzer 的 system、user、extra context 键。
- `backend/app/shared/resources/prompts/planning.md`
  新的 planning prompt bundle，承载 `planner_prompt`。
- `backend/app/shared/resources/prompts/approval.md`
  新的 approval prompt bundle，承载 `action_prompt`。
- `backend/app/shared/resources/prompts/llm.md`
  新的 llm prompt bundle，承载 `global_prompt_header`。

### Prompt Loading And Compatibility

- `backend/app/shared/prompting/prompt_loader.py`
  扩展为支持 bundle/key 读取、Markdown 解析、缓存、契约校验、兼容桥接。
- `backend/app/shared/prompting/legacy_prompt_aliases.py`
  建议新增的中心化旧引用映射表，统一维护旧文件名/旧路径到 `bundle.key` 的桥接。
- `backend/tests/test_prompt_loader.py`
  加载器契约测试主入口，覆盖解析、校验、兼容桥接、缓存失效。

### Module Callers To Migrate

- `backend/app/modules/planning/application/services.py`
  planning 调用点，当前依赖 `planner_prompt.txt`。
- `backend/approval/action_prompt.py`
  approval 调用点，当前依赖 `action_prompt.txt`。
- `backend/llm/prompt_builder.py`
  llm 旁路调用点，当前直接读取 `global_prompt_header.txt`。
- `backend/routers/mod_analyzer.py`
  analyzer mod 分析调用点，当前依赖 `mod_analyzer_*.txt`。
- `backend/routers/log_analyzer.py`
  analyzer log 分析调用点，当前依赖 `log_analyzer_*.txt`。
- `backend/app/modules/codegen/application/prompt_assembler.py`
  codegen 主调用点，当前依赖多份主模板 `.txt`。
- `backend/agents/code_agent.py`
  codegen API lookup 旁路调用点，当前直连 `partials/api_lookup/*.txt`。
- `backend/image/prompt_adapter.py`
  image 调用点，当前依赖 `adapt`、`guide`、`fallback`、透明背景 `.txt`。

### Legacy Prompt Files To Retire

- `backend/app/modules/planning/resources/prompts/planner_prompt.txt`
- `backend/approval/resources/prompts/action_prompt.txt`
- `backend/llm/resources/global_prompt_header.txt`
- `backend/app/modules/analyzer/resources/prompts/*.txt`
- `backend/app/modules/codegen/resources/prompts/*.txt`
- `backend/app/modules/codegen/resources/prompts/partials/api_lookup/*.txt`
- `backend/app/modules/image/resources/prompts/*.txt`

### Tests And Docs

- `backend/tests/test_planning_module.py`
- `backend/tests/test_action_prompt.py`
- `backend/tests/test_prompt_builder.py`
- `backend/tests/test_mod_analyzer.py`
- `backend/tests/test_log_analyzer.py`
- `backend/tests/test_codegen_module.py`
- `backend/tests/test_code_agent_lookup.py`
- `backend/tests/test_image_prompt_adapter.py`
- `docs/01-总览/Prompt-文本资源总览.md`

## Execution Order

1. 建立六个 Markdown 真源并完成键名清单。
2. 扩展 `PromptLoader` 与中心化兼容映射，先让新真源可读、可校验、可桥接。
3. 先切 `planning`、`approval`、`llm` 这三条相对单纯的调用链。
4. 再切 `analyzer`，验证多模板 user/system/extra-context 组合读取。
5. 然后切 `codegen`，同时收口 `api_lookup` partials。
6. 最后切 `image`，完成高风险 `guide/fallback` 组合读取回归。
7. 所有调用点稳定后删除旧 `.txt`，跑零旧引用扫描、回归测试和文档更新。

## Chunk 1: Establish Markdown Source Of Truth

### Task 1: 创建统一 bundle 目录与六个 Markdown 真源文件

**Files:**
- Create: `backend/app/shared/resources/prompts/codegen.md`
- Create: `backend/app/shared/resources/prompts/image.md`
- Create: `backend/app/shared/resources/prompts/analyzer.md`
- Create: `backend/app/shared/resources/prompts/planning.md`
- Create: `backend/app/shared/resources/prompts/approval.md`
- Create: `backend/app/shared/resources/prompts/llm.md`
- Test: `backend/tests/test_prompt_loader.py`

- [ ] **Step 1: 先列出每个旧 `.txt` 对应的新键名**

Run: `rg --files backend/app/modules/planning/resources/prompts backend/approval/resources/prompts backend/llm/resources backend/app/modules/analyzer/resources/prompts backend/app/modules/codegen/resources/prompts backend/app/modules/image/resources/prompts`

Expected: 返回六组模块下全部旧 prompt 文件路径，便于逐个映射到稳定键名。

- [ ] **Step 2: 为六个 bundle 写模块标题和键标题骨架**

要求：
- 每个 bundle 都有清晰的文件级说明标题
- 每个运行时片段都以 `## 键名` 开头
- 同一 bundle 内不得出现重复键

- [ ] **Step 3: 把旧 `.txt` 正文迁入对应 `## 键名` 段落**

要求：
- 不改写语义
- 保留模板变量和换行语义
- `codegen` 的 `partials/api_lookup/*.txt` 合并进 `codegen.md`
- `image` 的 `guide_*`、`fallback_*`、`transparent_bg_rule.txt` 全部合并进 `image.md`

- [ ] **Step 4: 为每个 bundle 补最小可读性说明，但不引入运行时歧义**

要求：
- 说明文字只能放在一级标题或非运行时区域
- 不新增会被误识别为运行时键的非法二级标题

- [ ] **Step 5: 先写存在性测试或 fixture 约束**

在 `backend/tests/test_prompt_loader.py` 中补充：
- 六个 bundle 文件存在
- 每个 bundle 至少解析出一个键
- 关键键如 `planning.planner_prompt`、`approval.action_prompt`、`llm.global_prompt_header` 存在

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_prompt_loader.py -v`

Expected: 初次运行可先因解析能力未实现而失败，但测试名称和失败原因应清晰暴露缺口。

### Task 2: 前置清点全仓旧引用消费者与删除影响面

**Files:**
- Modify: `docs/superpowers/plans/2026-03-28-prompt-bundle-consolidation.md`

- [ ] **Step 1: 在开始切调用点前扫描 repo-wide 旧 `.txt` 消费者**

Run: `rg -n "resources/prompts/.*\\.txt|global_prompt_header\\.txt|planner_prompt\\.txt|action_prompt\\.txt|mod_analyzer_.*\\.txt|log_analyzer_.*\\.txt|guide_.*\\.txt|fallback_.*\\.txt|partials/api_lookup/.*\\.txt|PromptLoader\\(root=.*prompts|read_text\\(" backend tests docs`

Expected: 列出当前所有旧 prompt `.txt` 消费者和历史说明位置，形成迁移前清单；若出现计划外调用点，必须先补入后续 chunk。

- [ ] **Step 2: 按调用类型给清单打标签**

要求：
- 区分运行时代码消费者、测试消费者、文档历史说明
- 标记哪些调用点预计由 `PromptLoader` 兼容桥接兜底
- 标记哪些调用点必须在删除前主动改造

- [ ] **Step 3: 以清单校对后续迁移范围**

要求：
- 后续 chunk 中必须覆盖所有运行时代码消费者
- 若发现计划未覆盖的调用点，先补对应任务再进入实现
- 删除阶段的零旧引用扫描范围固定为 `backend tests docs`

## Chunk 2: Extend PromptLoader And Legacy Bridge

### Task 3: 让 `PromptLoader` 支持 Markdown bundle/key 读取

**Files:**
- Modify: `backend/app/shared/prompting/prompt_loader.py`
- Test: `backend/tests/test_prompt_loader.py`

- [ ] **Step 1: 先为 Markdown 解析契约写失败测试**

至少覆盖：
- 正常解析多个 `## 键名`
- 重复键报错
- 非法键名报错
- 空片段报错
- fenced code block 内的 `##` 不被识别为键
- 缺失键报稳定异常

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_prompt_loader.py -k "bundle or markdown or parser" -v`

Expected: FAIL，明确提示缺少 bundle/key 支持或契约未满足。

- [ ] **Step 2: 在加载器中引入 bundle/key API，同时保留旧 `load()` / `render()` 风格**

要求：
- 支持显式读取 `bundle + key`
- 旧调用方仍可使用现有公开入口
- 模板变量替换行为与当前一致

- [ ] **Step 3: 实现 Markdown 解析、缓存与缓存失效**

要求：
- 解析结果以 `key -> text` 缓存
- 文件内容变化后可刷新缓存
- 不因缓存而吞掉结构错误

- [ ] **Step 4: 运行加载器契约测试**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_prompt_loader.py -v`

Expected: PASS，Markdown 解析、校验、缓存相关测试全部通过。

### Task 4: 新增中心化兼容映射并禁止旧文件回读

**Files:**
- Create: `backend/app/shared/prompting/legacy_prompt_aliases.py`
- Modify: `backend/app/shared/prompting/prompt_loader.py`
- Test: `backend/tests/test_prompt_loader.py`

- [ ] **Step 1: 先写旧路径桥接测试**

至少覆盖：
- 旧文件名映射到唯一 `bundle.key`
- 旧相对路径映射到唯一 `bundle.key`
- 未命中的旧引用不会退回磁盘 `.txt` 读取
- 通过旧引用读取的结果与直接读取 `bundle.key` 一致

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_prompt_loader.py -k "alias or legacy or bridge" -v`

Expected: FAIL，当前尚无中心化映射或仍存在旧文件回读。

- [ ] **Step 2: 建立完整旧引用映射表**

映射至少覆盖：
- `planning.planner_prompt`
- `approval.action_prompt`
- `llm.global_prompt_header`
- analyzer 五个旧模板
- codegen 六个主模板与四个 `api_lookup` partial
- image 全量 `adapt/guide/fallback/transparent` 旧模板

- [ ] **Step 3: 把旧 `template_name`、旧相对路径和必要的旧绝对根目录场景统一导向映射表**

要求：
- 兼容桥只负责查表跳转
- 不保留“找不到映射就回读旧 `.txt`”的退路
- 错误消息中带出缺失的旧引用，便于扫描残留调用点

- [ ] **Step 4: 运行加载器测试确认桥接生效**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_prompt_loader.py -v`

Expected: PASS，旧引用桥接和禁止旧文件回读的测试全部通过。

## Chunk 3: Migrate Planning, Approval, And LLM

### Task 5: 切换 planning 与 approval 到 bundle/key 读取

**Files:**
- Modify: `backend/app/modules/planning/application/services.py`
- Modify: `backend/approval/action_prompt.py`
- Test: `backend/tests/test_planning_module.py`
- Test: `backend/tests/test_action_prompt.py`
- Test: `backend/tests/test_planner.py`

- [ ] **Step 1: 先补调用点测试，锁定新读取契约**

测试要证明：
- planning 从 `planning.planner_prompt` 读取
- approval 从 `approval.action_prompt` 读取
- fallback 模板仍只作为兜底文本，不再依赖旧 `.txt`

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_planning_module.py .\backend\tests\test_action_prompt.py .\backend\tests\test_planner.py -v`

Expected: 初次应因断言仍写着旧 `.txt` 名称而失败，或缺少新断言。

- [ ] **Step 2: 修改 planning 与 approval 调用点到 bundle/key**

要求：
- 不改变上层 service/public function 的外部行为
- 仅替换资源寻址方式
- 保持现有 fallback 模板常量

- [ ] **Step 3: 跑模块测试确认 prompt 内容无回归**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_planning_module.py .\backend\tests\test_action_prompt.py .\backend\tests\test_planner.py -v`

Expected: PASS，关键内容断言仍成立。

### Task 6: 收口 `llm` 旁路直读到统一加载入口

**Files:**
- Modify: `backend/llm/prompt_builder.py`
- Test: `backend/tests/test_prompt_builder.py`

- [ ] **Step 1: 先补 `global_prompt_header` 来自 bundle 的测试**

测试要证明：
- `prompt_builder` 不再 `Path(...).read_text()`
- 自定义全局 prompt 开启时读取 `llm.global_prompt_header`
- 空白自定义 prompt 行为保持不变

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_prompt_builder.py -v`

Expected: FAIL，当前仍存在旁路直读。

- [ ] **Step 2: 改造 `prompt_builder` 使用统一 `PromptLoader`**

要求：
- 通过统一加载入口读取 `llm` bundle
- 删除对 `backend/llm/resources/global_prompt_header.txt` 的运行时依赖

- [ ] **Step 3: 重新运行 llm 相关测试**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_prompt_builder.py -v`

Expected: PASS，`global_prompt_header` 行为保持稳定且无磁盘直读。

## Chunk 4: Migrate Analyzer Call Sites

### Task 7: 切换 mod/log analyzer 到统一 bundle

**Files:**
- Modify: `backend/routers/mod_analyzer.py`
- Modify: `backend/routers/log_analyzer.py`
- Test: `backend/tests/test_mod_analyzer.py`
- Test: `backend/tests/test_log_analyzer.py`

- [ ] **Step 1: 先补 analyzer 调用契约测试**

测试要覆盖：
- mod analyzer 的 system/user 模板走 `analyzer` bundle
- log analyzer 的 system/user/extra context 模板走 `analyzer` bundle
- 渲染变量和原有 fallback 常量保持不变

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_mod_analyzer.py .\backend\tests\test_log_analyzer.py -v`

Expected: 初次应暴露旧 `.txt` 名称断言或仍走旧模板名。

- [ ] **Step 2: 修改两个 router 的加载方式**

要求：
- system prompt 使用 bundle/key 读取
- user prompt 和 extra context 使用 bundle/key 渲染
- 不改事件流、异常处理和 WS 输出格式

- [ ] **Step 3: 重新运行 analyzer 测试**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_mod_analyzer.py .\backend\tests\test_log_analyzer.py -v`

Expected: PASS，分析器行为与输出内容保持稳定。

## Chunk 5: Migrate Codegen And API Lookup

### Task 8: 切换 codegen 主模板到 `codegen.md`

**Files:**
- Modify: `backend/app/modules/codegen/application/prompt_assembler.py`
- Modify: `backend/app/modules/codegen/application/services.py`
- Test: `backend/tests/test_codegen_module.py`
- Test: `backend/tests/test_review_fixes.py`

- [ ] **Step 1: 先补 codegen 主模板读取测试**

测试至少覆盖：
- `asset_prompt`
- `asset_group_prompt`
- `custom_code_prompt`
- `build_prompt`
- `create_mod_project_prompt`
- `package_prompt`

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_codegen_module.py .\backend\tests\test_review_fixes.py -v`

Expected: 初次应因断言仍使用 `*.txt` 模板名而失败，或缺少 `codegen` bundle 断言。

- [ ] **Step 2: 修改 `prompt_assembler` 到 bundle/key 读取**

要求：
- 六个主模板全部改为 `codegen` bundle key
- 模板变量与 fallback 常量不变
- `services.py` 只做最小接线变更

- [ ] **Step 3: 跑 codegen 主模板测试**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_codegen_module.py .\backend\tests\test_review_fixes.py -v`

Expected: PASS，生成 prompt 的关键文本断言仍成立。

### Task 9: 收口 `api_lookup` partials 到 `codegen` bundle

**Files:**
- Modify: `backend/agents/code_agent.py`
- Modify: `backend/app/modules/codegen/application/prompt_assembler.py`
- Test: `backend/tests/test_code_agent_lookup.py`
- Test: `backend/tests/test_codegen_module.py`

- [ ] **Step 1: 先补 `api_lookup` 通过 bundle 读取的测试**

测试要证明：
- `code_agent` 不再从 `partials/api_lookup/` 目录逐文件读取
- `prompt_assembler` 仍能拿到完整 `api_lookup` section
- 本地反编译资料命中与 fallback 路径行为保持不变

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_code_agent_lookup.py .\backend\tests\test_codegen_module.py -k "api_lookup" -v`

Expected: FAIL，当前仍使用分散 partial `.txt`。

- [ ] **Step 2: 实现新的 `api_lookup` 拼装方式**

要求：
- 从 `codegen.md` 读取 `lookup_title`、`lookup_baselib`、`lookup_sts2_local`、`lookup_sts2_fallback`
- 保持现有“有本地资料优先，无本地资料回退”的行为

- [ ] **Step 3: 运行 `api_lookup` 相关测试**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_code_agent_lookup.py .\backend\tests\test_codegen_module.py -k "api_lookup" -v`

Expected: PASS，`api_lookup` 段落内容和分支行为稳定。

## Chunk 6: Migrate Image Prompt Assembly

### Task 10: 切换 image 模块到 `image.md`

**Files:**
- Modify: `backend/image/prompt_adapter.py`
- Test: `backend/tests/test_image_prompt_adapter.py`

- [ ] **Step 1: 先补 image bundle 契约测试**

测试至少覆盖：
- `adapt_prompt`
- 四组 `guide_*_rules/formula/example`
- `guide_sdxl_negative_example`
- `transparent_bg_rule`
- 中英文 fallback 后缀
- `fallback_sdxl_negative_prompt`

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_image_prompt_adapter.py -v`

Expected: 初次应暴露旧 `.txt` 名称断言、旧路径读取或缺少 bundle 断言。

- [ ] **Step 2: 修改 `prompt_adapter` 的所有资源读取入口**

要求：
- provider guide 资源全部从 `image` bundle 读取
- fallback 分支全部从 `image` bundle 读取
- 不改变 provider 分支、透明背景判断和 LLM 失败回退逻辑

- [ ] **Step 3: 跑 image 测试，确认高风险组合路径稳定**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_image_prompt_adapter.py -v`

Expected: PASS，关键 guide/fallback/transparent 组合断言全部通过。

## Chunk 7: Retire Legacy TXT Files, Verify, And Update Docs

### Task 11: 删除旧 `.txt` 资源并完成零旧引用扫描

**Files:**
- Delete: `backend/app/modules/planning/resources/prompts/planner_prompt.txt`
- Delete: `backend/approval/resources/prompts/action_prompt.txt`
- Delete: `backend/llm/resources/global_prompt_header.txt`
- Delete: `backend/app/modules/analyzer/resources/prompts/mod_analyzer_system.txt`
- Delete: `backend/app/modules/analyzer/resources/prompts/mod_analyzer_user.txt`
- Delete: `backend/app/modules/analyzer/resources/prompts/log_analyzer_system.txt`
- Delete: `backend/app/modules/analyzer/resources/prompts/log_analyzer_user.txt`
- Delete: `backend/app/modules/analyzer/resources/prompts/log_analyzer_extra_context.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/asset_prompt.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/asset_group_prompt.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/custom_code_prompt.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/build_prompt.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/create_mod_project_prompt.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/package_prompt.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/partials/api_lookup/title.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/partials/api_lookup/baselib.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/partials/api_lookup/sts2_local.txt`
- Delete: `backend/app/modules/codegen/resources/prompts/partials/api_lookup/sts2_fallback.txt`
- Delete: `backend/app/modules/image/resources/prompts/adapt_prompt.txt`
- Delete: `backend/app/modules/image/resources/prompts/transparent_bg_rule.txt`
- Delete: `backend/app/modules/image/resources/prompts/fallback_prompt_cn_suffix.txt`
- Delete: `backend/app/modules/image/resources/prompts/fallback_prompt_cn_transparent_suffix.txt`
- Delete: `backend/app/modules/image/resources/prompts/fallback_prompt_en_suffix.txt`
- Delete: `backend/app/modules/image/resources/prompts/fallback_prompt_en_transparent_suffix.txt`
- Delete: `backend/app/modules/image/resources/prompts/fallback_sdxl_negative_prompt.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_flux2_rules.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_flux2_formula.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_flux2_example.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_jimeng_rules.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_jimeng_formula.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_jimeng_example.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_sdxl_rules.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_sdxl_formula.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_sdxl_example.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_sdxl_negative_example.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_wanxiang_rules.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_wanxiang_formula.txt`
- Delete: `backend/app/modules/image/resources/prompts/guide_wanxiang_example.txt`
- Modify: `backend/app/shared/prompting/legacy_prompt_aliases.py`
- Test: `backend/tests/test_prompt_loader.py`
- Test: `backend/tests/test_planning_module.py`
- Test: `backend/tests/test_action_prompt.py`
- Test: `backend/tests/test_planner.py`
- Test: `backend/tests/test_prompt_builder.py`
- Test: `backend/tests/test_mod_analyzer.py`
- Test: `backend/tests/test_log_analyzer.py`
- Test: `backend/tests/test_codegen_module.py`
- Test: `backend/tests/test_code_agent_lookup.py`
- Test: `backend/tests/test_image_prompt_adapter.py`

- [ ] **Step 1: 先确认删除硬门槛全部满足**

检查项：
- 仓库内无运行时代码直读旧 `.txt`
- 与旧 `.txt` 相关的中心化兼容映射已全部下线
- `llm` 旁路已收口
- `image` 与 `codegen` 关键回归已通过
- `planning` 相关回归已通过，包含 `backend/tests/test_planner.py`

- [ ] **Step 2: 运行删除前残留清点扫描**

Run: `rg -n "resources/prompts/.*\\.txt|global_prompt_header\\.txt|planner_prompt\\.txt|action_prompt\\.txt|mod_analyzer_.*\\.txt|log_analyzer_.*\\.txt|guide_.*\\.txt|fallback_.*\\.txt|partials/api_lookup/.*\\.txt|PromptLoader\\(root=.*prompts|read_text\\(" backend tests docs`

Expected: 删除前允许命中历史引用；结果应作为残留清单逐项处理，并确认命中只来自待删除资源、待更新文档或计划内测试调整点；若出现计划外消费者，先补迁移任务，不得直接进入删除。

- [ ] **Step 3: 先下线全部旧 `.txt` 兼容映射，再删除旧资源文件**

要求：
- 先从 `legacy_prompt_aliases.py` 中删除全部旧 `.txt` 相关映射
- 不允许“删文件时仍保留旧名桥接”
- 物理删除全部旧 prompt `.txt`
- 删除完成后，兼容层不再承担任何旧 `.txt` 引用转发职责

- [ ] **Step 4: 运行核心回归测试**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_prompt_loader.py .\backend\tests\test_planning_module.py .\backend\tests\test_action_prompt.py .\backend\tests\test_planner.py .\backend\tests\test_prompt_builder.py .\backend\tests\test_mod_analyzer.py .\backend\tests\test_log_analyzer.py .\backend\tests\test_codegen_module.py .\backend\tests\test_code_agent_lookup.py .\backend\tests\test_image_prompt_adapter.py -v`

Expected: PASS，所有 prompt 读取链路都只依赖统一 Markdown 真源。

### Task 12: 更新说明文档并记录新真源布局

**Files:**
- Modify: `docs/01-总览/Prompt-文本资源总览.md`

- [ ] **Step 1: 更新 `.txt` 总览文档中的 prompt 资源结论**

要求：
- 明确六组运行时 prompt 已迁移到 `backend/app/shared/resources/prompts/*.md`
- 旧 `.txt` 状态改为已退役
- 不再保留任何旧 prompt 文件名或旧路径字符串，避免与最终零扫描 gate 冲突

- [ ] **Step 2: 补充统一 bundle 布局与键契约说明**

至少说明：
- 每模块一个 Markdown 文件
- `## 键名` 是唯一运行时索引
- `PromptLoader` 负责 bundle/key 解析与兼容桥接

- [ ] **Step 3: 复查文档与代码现状一致**

Run: `rg -n "global_prompt_header\\.txt|planner_prompt\\.txt|action_prompt\\.txt|mod_analyzer_.*\\.txt|log_analyzer_.*\\.txt|guide_.*\\.txt|fallback_.*\\.txt|api_lookup/.+\\.txt" docs/01-总览/Prompt-文本资源总览.md`

Expected: 零命中；文档中不再保留任何旧 prompt `.txt` 文件名或旧路径字符串。

- [ ] **Step 4: 运行最终零旧引用 gate 扫描**

Run: `rg -n "resources/prompts/.*\\.txt|global_prompt_header\\.txt|planner_prompt\\.txt|action_prompt\\.txt|mod_analyzer_.*\\.txt|log_analyzer_.*\\.txt|guide_.*\\.txt|fallback_.*\\.txt|partials/api_lookup/.*\\.txt|PromptLoader\\(root=.*prompts|read_text\\(" backend tests docs`

Expected: `backend`、`tests`、`docs` 范围内零命中；若仍有命中，必须显式更新或删除，不能以兼容桥接或历史文档遗漏通过。

## Follow-Up Checklist

- [ ] 六个 Markdown bundle 已全部建立并完成键名清单
- [ ] `PromptLoader` 已支持 bundle/key、Markdown 解析、缓存与结构校验
- [ ] 中心化兼容映射已在迁移期接管旧引用，并在删除前全部下线
- [ ] `planning`、`approval`、`llm` 已切到统一加载入口
- [ ] `analyzer` 已切到统一加载入口
- [ ] `codegen` 与 `api_lookup` 已切到 `codegen.md`
- [ ] `image` 已切到 `image.md`
- [ ] 旧 prompt `.txt` 已删除且零运行时旧引用扫描通过
- [ ] `docs/01-总览/Prompt-文本资源总览.md` 已更新为新布局

## Definition Of Done

满足以下条件才算 prompt bundle consolidation 完成：

- `backend/app/shared/resources/prompts/` 成为六组运行时 prompt 的唯一真实来源。
- 所有调用链通过统一 `PromptLoader` 或其统一入口读取 Markdown bundle，不再旁路直读旧文件。
- 旧文件名与旧路径的兼容逻辑集中在加载层，不分散到业务模块。
- 与旧 `.txt` 相关的兼容映射已全部下线，不再承担删除后的旧名桥接。
- `image` 与 `codegen` 的关键 prompt 组合回归通过，输出未出现非预期漂移。
- 仓库内不再存在运行时代码对旧 prompt `.txt` 的依赖。
- `backend`、`tests`、`docs` 范围内最终零旧引用 gate 扫描通过。
- 文档描述与代码现状一致，且不再包含旧 prompt 文件名或旧路径字符串。
