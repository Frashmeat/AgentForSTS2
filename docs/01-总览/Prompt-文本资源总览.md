# Prompt 与知识资源总览

> 文档定位：本文说明当前 Rust 实现中的内嵌 Prompt、STS2 模板、运行时知识资源及其加载边界。
>
> 事实依据：`crates/ats-core/prompts/`、`templates/`、`src/prompting/` 和 `src/knowledge/`。
>
> 权威入口：系统结构见 `项目架构总览.md`。
>
> 最后更新：2026-07-29

## 内嵌 Prompt

当前内置 Prompt 位于：

```text
crates/ats-core/prompts/
  analyzer.md
  codegen.md
  image.md
  runtime_agent.md
  runtime_system.md
  runtime_workflow.md
```

`PromptLoader::built_in()` 使用 `include_str!` 将六个文件编译进二进制，生产运行时不从磁盘读取这些模板。

## 加载契约

`crates/ats-core/src/prompting/loader.rs` 支持两种访问：

- `codegen.md`：读取完整文件。
- `codegen.asset_prompt`：按 Markdown `## asset_prompt` 读取 bundle 分段。

分段键必须匹配 `^[a-z0-9_]+$`；重复键、空分段、缺失文件和缺失模板变量都会返回明确错误。模板变量使用 `{{ name }}` 语法。

## STS2 稳定模板

当前 STS2 资源模板位于：

```text
crates/ats-core/templates/sts2/
  common.md
  planner_guidance.md
  card.md
  relic.md
  power.md
  potion.md
  character.md
  custom_code.md
```

这些文件用于表达工程骨架、资源类型和稳定约束，不应固化容易随游戏版本变化的行为答案。具体 API、hook 和调用顺序应优先从当前版本的游戏事实中检索。

## 运行时知识

运行时知识根目录为 `runtime/knowledge/`：

```text
runtime/knowledge/
  game/                  游戏反编译 C#
  baselib/               BaseLib 反编译结果
  resources/sts2/        STS2 规则和补充资源
  cache/                 可失效缓存
  packs/                 知识包
  knowledge-manifest.json
  active-knowledge-pack.json
```

`knowledge` 模块负责目录、manifest、反编译、BaseLib、事实检索、guidance、lookup 和知识包；`PromptContextAssembler` 将结果整理为 facts、guidance、lookup、warnings 和 summary，再交给 codegen/planning 流程。

## 维护边界

- 新的通用 Prompt 放入 `crates/ats-core/prompts/`，并显式注册到 loader。
- 新的 STS2 稳定结构模板放入 `crates/ats-core/templates/sts2/`。
- 当前游戏版本事实进入 `runtime/knowledge/`，不得复制进长期手写 Prompt 作为权威答案。
- 修改 bundle 键时同时更新调用点和相关测试。
- 旧 Python `backend/app/shared/resources/prompts/` 仅属于归档历史，不是当前 Rust 分支真源。
