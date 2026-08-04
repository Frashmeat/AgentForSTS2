# brainstorm: STS2 Pack Mod 类型与资源体验

## Goal

在现有 Stage 2 通用 Feature、Truth、Resource Workspace 和 STS2 Game Pack 边界上，扩展用户真正可用的 STS2 Mod 类型，并把资源准备从结构化 JSON/资源 ID 操作提升为可视化、可预览、可复用的工作流。

## What I already know

- 当前共享 catalog 已有 9 个通用 Feature，不需要为了增加 STS2 内容复制生成、日志、构建或打包功能。
- 当前 STS2 Stage 2 Pack 只正式注册 `custom_code` 和 `relic` 两个 item type。
- relic 已声明 C#、英/中本地化文件和 `normal/outline/big` 三种 PNG 资源角色。
- Resource Workspace 已支持 user upload、Pack default、AI media、版本、provenance 和 selection，但桌面端缺少专门的资源管理体验。
- 仓库有 card、character、power、potion 等旧 guidance，可作为迁移参考，但不能直接视为已实现的 Stage 2 合同。
- Single Generate 已使用 Pack item type 编译 run-scoped 精确输出 Schema；新增类型必须声明确定的 Truth 查询、生成文件角色、资源角色和验证方式。

## Assumptions

- Relic 用于地基 Gate；Gate 通过后连续交付首批四种内容类型，不为每种类型重新设计基础设施。
- 通用 Feature 合同保持游戏无关；STS2 类型知识继续放在 Game Pack/Truth 扩展中。
- 资源体验先聚焦静态图片，动画、音频和 3D 资源留作后续。

## Open Questions

- 无；方案与 8 个顺序 Orders 已执行完成。

## Requirements (evolving)

- 采用“地基优先、集中扩充”的两阶段交付：Relic 只作为地基验收样板，不把单类型实现当作最终范围。
- 地基必须一次性提供 Pack 驱动类型目录、通用类型表单、资源槽位工作台、真实媒体校验、确定性资源派生和通用类型测试框架。
- 地基通过独立 Gate 后，连续接入一批 STS2 类型；每种类型只新增 Pack/Truth/Recipe 数据和必要的类型专属字段，不复制 UI、资源管理或生成基础设施。
- 首批集中扩充范围确定为 `relic`、`card`、`potion`、`power`；现有 `custom_code` 迁移到同一通用体验但不算新增类型。
- 通用类型编辑器采用混合模式：Pack 描述关键结构化字段，用户锁定可控事实；自然语言描述行为意图；LLM 可补全计划和实现，但不得改写已锁定结构化字段。
- Plan result 必须同时保留 canonical structured values 与 behavior intent，Batch/Complex 和重跑复用同一输入，不从自然语言反向猜测已锁定字段。
- 图片资源采用 master + deterministic derivation：用户上传、AI 生成或选择 Pack default master，Pack 声明受限的确定性变换生成各最终 role，用户可对任一派生槽位单独覆盖。
- 派生资源必须记录 source resource/version、transform primitive/version、Pack identity 和输出 hash；不得由 LLM 直接伪造派生结果或目标路径。
- Resource Asset/Version 保持不可变；上传、AI 生成和派生首先创建候选版本，用户预览并显式确认后才更新 selected version。
- 候选准备或派生失败不得改变当前 selected version；Batch/Generate 必须绑定明确的 selected resource version，保证可复现。
- 地基支持新建、重新打开、编辑和重新生成已有 Mod item；`itemId` 是稳定身份，结构化字段、行为意图和资源绑定形成版本化 ItemDefinition。
- 每次 Plan/Generate 绑定明确的 ItemDefinition hash 并创建新的 Run/Artifact snapshot；旧 Run/Artifact 不可变，只有新生成与验证全部成功后才事务更新工程 published 文件。
- Batch/Complex 从 Item Library 可视化选择已保存 ItemDefinition，绑定每个 definition hash 并保留独立 child Run；原始 typed request 只读预览，不允许成为第二个可编辑事实来源。
- Batch 支持重试失败 Item，Complex 在选中 Item 的 Batch 成功策略满足后继续 Build/Package；所有组合复用同一 Item/Resource/Run 合同。
- 英中本地化均为 ItemDefinition 中显式 canonical 字段；用户选择主要编辑语言，AI 只能生成另一语言的候选翻译，用户确认后才写入 definition。
- 修改源语言后，已确认译文标记为 outdated 但不自动覆盖；Plan/Generate 只能使用已确认且未过期的必需 locale。
- Pack 未声明的类型不展示；Pack 已声明但当前 verified Truth 不满足全部 Evidence Query 的类型显示为 disabled，并提供安全、可操作的缺失原因与 Truth 重新导入入口。
- Capability readiness 在 Plan/模型调用前确定性计算；只有 `ready` 类型可以创建、编辑提交、Batch 或 Generate，Truth identity 变化后必须重新计算。
- 新增类型必须覆盖 Plan、Truth Evidence、Resource Roles、Generate、Compile、Artifact 和安装版人工验收。
- 提供资源导入、AI 生成、预览、派生尺寸、版本选择和生成前缺失检查。
- Batch/Complex 使用相同 item type 和资源合同，不建立平行实现。
- 失败保持 `truth.*`、`resource.*`、`model.*`、`validation.*`、`artifact.*` typed failure。

## Acceptance Criteria (evolving)

- [x] 新增一个普通 STS2 item type 不需要修改类型选择器、资源工作台或通用生成页面的类型分支。
- [x] UI 只展示当前 verified Pack 实际支持的 item type，不再硬编码或展示虚假能力。
- [x] 已声明但 Truth 不满足的类型显示 disabled 和缺失原因，且不会创建 Run 或调用模型；Truth 更新后 readiness 可恢复。
- [x] 用户可创建、保存、重新打开、编辑并重新生成 ItemDefinition，所有 Run/Artifact 可追溯到精确 definition hash。
- [x] 通用字段编辑器同时支持 canonical structured values、自然语言 behavior intent 和英中 confirmed/outdated 状态。
- [x] 用户无需手写资源 JSON 即可完成至少一个带图片资源的 STS2 Mod 生成。
- [x] 新增 item type 的所有文件角色、资源角色和 Truth 查询均由 Pack 声明并通过 schema 校验。
- [x] 用户上传与 AI 生成资源进入同一版本化 Resource Workspace，并可预览和切换选中版本。
- [x] Master 可确定性派生全部最终资源 role；候选失败或未确认时不改变 selected version，单槽位可覆盖。
- [x] 缺失、尺寸错误或媒体类型错误在模型调用前给出可操作失败。
- [x] Item Library 可视化选择多个 definition 执行 Batch/Complex，原始 typed request 仅只读预览，每个 Item 有独立 child Run。
- [x] `relic`、`card`、`potion`、`power` 均通过 Pack schema、Truth query、资源 completeness、真实 dotnet 校验与全新 Run/Artifact 验收。
- [x] 生成结果通过真实 dotnet 校验，RunRecord v3 与 ArtifactManifest v3/hash 可复算，无本轮 staging。

## Definition of Done

- 相关单元、集成、前端和桌面 E2E 通过。
- Stage 2 DAG、workspace check/test/clippy、前端 test/type/build 通过。
- Pack/schema/架构和用户流程文档同步。
- 新能力使用全新 Run/Artifact 验证，不复用历史验收证据。

## Out of Scope (explicit)

- 第二个真实游戏 Pack、动态插件或 marketplace。
- 动画、音频、3D 资源生成。
- `character` 及其跨 Item 引用、初始卡组/Relic、动画和复杂组合编辑留到下一批。
- Web 端工程执行、签名、自动发布和跨平台安装包。
- 在未验证 Truth 合同前批量宣称所有历史 STS2 guidance 已支持。

## Technical Notes

- `game_packs/sts2/stage2-game-pack.json`：当前 item type、Evidence Query、资源和构建/打包贡献。
- `crates/ats-features/src/mod_generate_single.rs`：run-scoped 输出合同和资源/Truth 前置校验。
- `crates/ats-features/src/resource_prepare.rs`、`crates/ats-adapters/src/resource_store.rs`：统一 Resource 合同和文件实现。
- `src/pages/ModEditorPage.tsx`：当前通过 JSON 输入 selected resources。
- `src/pages/BatchGenerationPage.tsx`：当前 Batch/Complex 仍以原始 JSON 为主。

## Research Notes

### Common product pattern

- 成熟的内容创作工具通常以“内容类型/元素编辑器”承载结构化字段，而不是让用户直接编辑生成请求 JSON。
- 资源通常先进入统一资源库，再绑定到具体内容槽位；上传、默认资源和生成资源共享预览、版本和引用模型。
- 派生尺寸、格式和轮廓应由确定性处理管线生成，生成前提供 completeness/validation gate，而不是把三个最终文件都交给用户手工准备。

外部检索在本轮超时，以上结论只作为通用产品模式参考，不作为第三方工具的精确功能声明。

### Current implementation gaps

- `ResourcePrepareRequest` 已支持 user upload、Pack default 和 AI generated，但没有桌面资源工作台。
- 当前 user upload 只按声明的 media type/role 入库，没有读取图片真实格式、像素尺寸或透明通道进行校验。
- `resource.prepare.specs` 描述最终 role 的宽高和路径，但尚未描述 master source、派生关系、裁剪/缩放/轮廓策略。
- `image.role-transform` 当前是 Pack 所需并已注册的 Primitive ID，但没有形成可执行的资源派生 Feature 流程。

### Order 3 resolution

- ResourceAsset 与 Resource Prepare request/result 已升级 v2；所有 upload、AI、Pack default 和派生结果先成为未选择 candidate。
- `pack.resource-specs` v2 已声明 STS2 Relic 的 512 master 与 normal/outline/big direct derivation graph。
- PNG Adapter 已真实探测 media/dimensions/alpha，并提供受限 deterministic resize/outline；Pack 只能引用已声明的 v1 Primitive。
- master 四候选使用 batch staging/rollback；失败不改变既有 selected，ProjectSession 持有唯一 Resource repository。
- Order 3 完整门禁为 workspace 126 tests、frontend 15 tests，加 check/clippy、ML/E2E compile、DAG、CLI、fmt/diff；可视化 preview/confirm 属于 Order 4。

### Feasible approaches

**Approach A: Relic vertical slice first (Recommended)**

- 完成 relic 资源槽位工作台、master image、派生 normal/outline/big、预览/版本/校验，再迁移 card/potion/power。
- 优点：最小范围同时验证 Pack 类型扩展与资源体验，E2E 清晰，风险最低。
- 缺点：第一轮新增内容类型较少。

**Approach B: Register card/relic/potion/power together**

- 先扩 Pack 类型和生成模板，资源 UI 只做通用上传/选择。
- 优点：目录中的可选类型快速增加。
- 缺点：Truth、资源角色和实际游戏验收面过大，容易形成“能选但不好用”的半成品。

**Approach C: Resource manager first**

- 先建设完整资源库、预览、版本和 AI 生成，不新增 Mod 类型。
- 优点：资源基础最完整。
- 缺点：缺少新内容类型驱动，容易做出脱离真实生成流程的通用资产管理器。

## Decision (ADR-lite, evolving)

**Context**: 当前逐类型扩展会重复修改硬编码下拉框、JSON 资源输入和类型特例；仅完成 Relic 会验证纵向链路，但不能达到用户期望的集中扩充效率。

**Decision**: 先建设可由 Game Pack 数据驱动的通用类型与资源地基，用 Relic 验证地基；地基 Gate 通过后，连续扩充一组 STS2 类型。

**Consequences**: 第一阶段的可见类型数量增长较慢且涉及 Pack/UI/Feature/Adapter 跨层合同升级；一旦完成，后续类型主要成为 Pack/Truth/Recipe 内容工作，能以一致门禁批量交付。首批集中扩充 `relic/card/potion/power`；Character 明确留到下一批，避免把动画、跨 Item 引用和复杂组合提前压入本轮地基。

### Editor decision

**Context**: 纯自然语言难以保证费用、稀有度、目标和数值等可重复事实；纯表单又无法自然表达复杂行为。

**Decision**: 采用结构化字段 + 自然语言行为意图的混合编辑器。Pack 只能使用受限的通用字段描述，不携带任意 UI 组件；用户锁定的结构化值是 canonical input。

**Consequences**: Pack schema 和 Plan request/result 需要版本升级，前端要实现通用字段渲染和锁定值回显；换来可重复生成、可靠 Batch、可编辑重跑和更少模型漂移。

### Resource derivation decision

**Context**: 要求用户分别准备每个最终图片角色会产生重复劳动和风格漂移；要求图片 Provider 一次返回整套严格资源又会绑定供应商能力。

**Decision**: 使用 master + deterministic derivation。Master 可来自上传、AI 或 Pack default；Pack 只声明注册过的变换步骤和输出角色，Adapter 执行并校验；每个派生结果允许用户单独覆盖。

**Consequences**: `resource.prepare`、Pack resource specs、provenance 和媒体 Adapter 需要版本升级并实现真实媒体探测/变换；换来统一、可重复、provider-neutral 的资源体验。

### Resource selection decision

**Context**: 自动选择新上传或 AI 生成结果会覆盖用户已确认资源，且派生部分失败时可能产生槽位版本不一致。

**Decision**: 所有新资源先作为不可变候选版本完成探测、派生和预览；只有用户显式确认才更新 selected version。

**Consequences**: UI 需要候选/已选状态和确认操作，Resource Workspace 需要稳定的版本列表查询；换来非破坏编辑、失败安全和 Batch/重跑可复现性。

### Item lifecycle decision

**Context**: 只支持新建会迫使用户手工重输内容，并在扩充多个类型后再次重做数据模型；资源版本和结构化字段也无法形成长期工程资产。

**Decision**: 引入稳定 Item identity 和版本化 ItemDefinition，支持新建、打开、编辑、重新 Plan/Generate。Run request 与 Artifact provenance 记录 definition hash；重新生成不覆盖历史证据。

**Consequences**: Workspace、Feature request/result、Artifact context 和 UI 需要新增 Item repository/transport；换来可维护内容、可靠重跑、变体基础和后续 Character 跨 Item 引用的稳定锚点。

### Batch/Complex decision

**Context**: 当前原始 JSON 输入会绕过 Item 编辑、资源选择和 definition identity；同时维护表单与可编辑 JSON 会产生两个事实来源。

**Decision**: Batch/Complex 直接从 Item Library 选择 ItemDefinition；UI 可展示只读 typed request preview，但不允许独立编辑。

**Consequences**: Batch 页面需要改造成 Library selection、执行策略和 child Run 状态视图；换来单项/批量/复杂生成一致、可复现且不绕过资源门禁。

### Localization decision

**Context**: 生成时临时翻译会随 Provider 漂移且无法在构建前校对；只支持一种语言又会立即欠下现有英中 Pack 合同升级债。

**Decision**: 双语字段显式保存在 ItemDefinition。AI translation 只创建候选，用户确认后成为 canonical；源文本变化使译文 outdated。

**Consequences**: 通用字段 schema、Item editor 和 completeness gate 要支持 localized value/status；换来可审阅术语、稳定重跑和未来 locale 扩展。

### Capability readiness decision

**Context**: 隐藏已声明但 Truth 暂不满足的类型会混淆“不支持”和“环境未就绪”；允许提交后失败会浪费模型调用并污染 Batch Run。

**Decision**: 未声明类型不展示；已声明但 Evidence Query 未满足的类型显示 disabled 和安全原因；只有 ready 类型允许执行。Truth identity 变化后重新计算 readiness。

**Consequences**: Game Context/Composition 需要提供 Pack capability/readiness 查询和 typed transport，UI 需要 ready/blocked 状态；换来真实能力展示、前置失败和更可靠的批量执行。

## Technical Approach

```text
Verified Pack + Truth
-> Item Type Capability Catalog / readiness
-> ItemDefinition Repository + generic editor
-> Resource Workbench / candidate versions / deterministic derivation
-> Plan + Single Generate
-> Item Library Batch/Complex
-> Build / Package / Run / Artifact
```

- Kernel/Runtime 只增加稳定值合同和 provenance，不认识 STS2 类型。
- `ats-game-context` 解析并验证受限 Item Type/field/resource/derivation descriptors，结合当前 Truth 计算 readiness。
- `ats-workspace` 持有版本化 ItemDefinition repository 和既有 Resource Workspace；原子写入、路径和 OS lock 合同保持。
- `ats-features` 升级 Plan/Resource/Single/Batch/Complex typed schema，消费 canonical ItemDefinition，不增加 `game_id == "sts2"` 分支。
- `ats-adapters` 实现真实 PNG 探测、受限确定性 image transform、Item/Resource persistence；图片依赖须在实现前做安全/许可证/体积评估。
- Tauri 暴露 typed capability、Item、Resource 查询/命令；React 使用 Pack-driven 通用编辑器、Resource Workbench 和 Item Library，不维护平行 JSON 状态。
- Runtime 继续加载一个 canonical pinned Pack；若为扩展维护性拆分 authoring fragments，必须由确定性工具汇编并验证同一最终 Pack/hash，不能引入运行时任意脚本。

## Implementation Plan (continuous Orders)

1. **Order 1 — Pack/Item contracts and readiness**：Item Type descriptor、通用字段 schema、ItemDefinition/hash、capability readiness、Pack validation/authoring gate和后端测试。
2. **Order 2 — Item repository and generic editor**：Item Library、新建/打开/编辑、混合字段编辑、英中候选/确认/outdated、typed Tauri transport 和前端测试。
3. **Order 3 — Resource Prepare v2 foundation**：真实媒体探测、master/derived、确定性 transform、candidate/selected version、provenance、失败原子性和 Adapter/Feature 集成测试。
4. **Order 4 — Resource Workbench and Relic Gate**：预览、上传/AI/default、派生/覆盖/确认、completeness gate；Relic 从 ItemDefinition 到真实 compile/Artifact E2E。
5. **Order 5 — Card expansion**：经 Truth 验证的字段、Evidence Query、资源/本地化/输出角色和真实纵向测试。
6. **Order 6 — Potion expansion**：同一地基上的 Potion Pack/Truth/Recipe 内容与纵向测试。
7. **Order 7 — Power expansion**：生命周期/stack Truth 门禁、Pack/Recipe 内容与纵向测试。
8. **Order 8 — Visual Batch/Complex and final gates**：Item Library selection、child Run/retry、Build/Package、四类型组合测试、桌面 GUI E2E、全量机器门禁和文档；新候选与人工验收为后续独立授权门禁。

每个 Order 先完成针对性与受影响机器门禁，再按用户持续授权自动 Git commit 并继续下一 Order。完整 release candidate、安装器和外部图片 Provider 质量测试仍使用独立授权门禁。
