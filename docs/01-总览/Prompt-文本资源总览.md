# Prompt 资源现状总览

更新时间：2026-03-30

## 结论

运行时 Prompt 资源已经完成收口，当前唯一真实来源是：

- `backend/app/shared/resources/prompts/planning.md`
- `backend/app/shared/resources/prompts/approval.md`
- `backend/app/shared/resources/prompts/llm.md`
- `backend/app/shared/resources/prompts/analyzer.md`
- `backend/app/shared/resources/prompts/codegen.md`
- `backend/app/shared/resources/prompts/image.md`

当前工作树中的旧分散 `.txt` Prompt 资源已删除。
本事项的最终完成判定仍以零旧引用扫描、关键定点验证和收尾文档一致性为准。

## 加载约定

统一由 `backend/app/shared/prompting/prompt_loader.py` 负责运行时读取。

当前约定：

- 每个模块一个 Markdown bundle
- 运行时索引使用 `## 键名`
- 调用方式统一为 `bundle.key`

示例：

- `planning.planner_prompt`
- `approval.action_prompt`
- `llm.global_prompt_header`
- `analyzer.mod_analyzer_system`
- `codegen.asset_prompt`
- `image.adapt_prompt`

## 当前 bundle 与主要键

### planning

文件：

- `backend/app/shared/resources/prompts/planning.md`

主要键：

- `planning.planner_prompt`

主要调用点：

- `backend/app/modules/planning/application/services.py`

### approval

文件：

- `backend/app/shared/resources/prompts/approval.md`

主要键：

- `approval.action_prompt`

主要调用点：

- `backend/approval/action_prompt.py`

### llm

文件：

- `backend/app/shared/resources/prompts/llm.md`

主要键：

- `llm.global_prompt_header`

主要调用点：

- `backend/llm/prompt_builder.py`

### analyzer

文件：

- `backend/app/shared/resources/prompts/analyzer.md`

主要键：

- `analyzer.mod_analyzer_system`
- `analyzer.mod_analyzer_user`
- `analyzer.log_analyzer_system`
- `analyzer.log_analyzer_user`
- `analyzer.log_analyzer_extra_context`

主要调用点：

- `backend/routers/mod_analyzer.py`
- `backend/routers/log_analyzer.py`

### codegen

文件：

- `backend/app/shared/resources/prompts/codegen.md`

主要键：

- `codegen.asset_prompt`
- `codegen.asset_group_prompt`
- `codegen.custom_code_prompt`
- `codegen.build_prompt`
- `codegen.create_mod_project_prompt`
- `codegen.package_prompt`
- `codegen.api_lookup_title`
- `codegen.api_lookup_baselib`
- `codegen.api_lookup_sts2_local`
- `codegen.api_lookup_sts2_fallback`

主要调用点：

- `backend/app/modules/codegen/application/prompt_assembler.py`
- `backend/agents/code_agent.py`

### image

文件：

- `backend/app/shared/resources/prompts/image.md`

主要键：

- `image.adapt_prompt`
- `image.transparent_bg_rule`
- `image.fallback_prompt_cn_suffix`
- `image.fallback_prompt_cn_transparent_suffix`
- `image.fallback_prompt_en_suffix`
- `image.fallback_prompt_en_transparent_suffix`
- `image.fallback_sdxl_negative_prompt`
- `image.guide_flux2_formula`
- `image.guide_flux2_rules`
- `image.guide_flux2_example`
- `image.guide_sdxl_formula`
- `image.guide_sdxl_rules`
- `image.guide_sdxl_example`
- `image.guide_sdxl_negative_example`
- `image.guide_jimeng_formula`
- `image.guide_jimeng_rules`
- `image.guide_jimeng_example`
- `image.guide_wanxiang_formula`
- `image.guide_wanxiang_rules`
- `image.guide_wanxiang_example`

主要调用点：

- `backend/image/prompt_adapter.py`

## 迁移状态

已完成迁移的运行时链路：

- `planning`
- `approval`
- `llm`
- `analyzer`
- `codegen`
- `image`

当前运行时代码不应再依赖历史 Prompt 文件路径；如后续新增 Prompt，必须直接落在 shared bundle 中。
