# ADR 0004 - 通用执行内核、Game Pipeline Provider 与 Game Pack 边界

| 项 | 值 |
| --- | --- |
| 状态 | Accepted; Provider foundation、STS2 cutover、trusted Prepare executor 与机器门禁已完成 |
| 初始决议 | 2026-07-29 |
| 本次修订 | 2026-08-18 |
| 适用范围 | 多游戏 Mod 的生成、验证、构建、打包、恢复与发布 |
| 当前代码事实 | Provider v1/registry、`ats-game-sts2`、Provider-driven delivery 与 trusted Prepare executor 已接线；synthetic data-only 产品路径和完整机器门禁已通过；当前模型仍直接作者化原生文件，尚未实现 Behavior Adapter |
| 实施约束 | 本文不授权代码重构、candidate 构建、安装、发布或删除历史证据 |
| 相关方案 | [Stage 2 分层能力与资源架构](../90-归档/已完成基线/2026-08-02-Stage-2分层能力与资源架构方案.md)；[Typed Behavior IR 与 Game Pack 确定性生成架构](./Typed-Behavior-IR与Game-Pack确定性生成架构方案.md) |

## 1. 决议摘要

AgentTheSpire 不把 STS2 当前的 `C# -> localization -> dotnet -> Godot PCK -> ZIP` 顺序提升为跨游戏固定流水线，也不允许 Game Pack 注入任意命令。

采用四层结构：

```text
Product Feature / User Intent
              |
              v
+-----------------------------------------------+
| Core Execution Kernel                         |
| DAG / queue / checkpoint / Run / Artifact     |
| retry / circuit breaker / transaction/publish |
+----------------------+------------------------+
                       |
                       v
+-----------------------------------------------+
| Game Pipeline Contract                        |
| capability + typed node + input/output schema |
+----------------------+------------------------+
                       |
          +------------+-------------+
          |                          |
          v                          v
+---------------------+   +---------------------+
| STS2 Provider       |   | Other Game Provider |
| C# / BaseLib        |   | its own stages      |
| dotnet / Godot / ZIP|   | and toolchain       |
+----------+----------+   +----------+----------+
           ^                         ^
           | exact ID/version/SHA    |
           +-----------+-------------+
                       |
                  Game Pack
```

核心原则：

- Core 负责可靠地执行一张有类型的图，不决定某个游戏需要哪些阶段。
- `ats-game-context` 定义 Game Pipeline Provider、capability 和节点合同。
- 受信任的 Game Adapter 实现游戏专属阶段，并把具体任务物化为执行图。
- Game Pack 只通过精确 ID、version 和 SHA 选择已注册能力并提供声明数据。
- AI 是可选 Primitive；没有模型步骤的游戏流水线同样是一等路径。
- 用户只看到 Item 结果、校验状态和单 Item 调整入口，不直接操作 DAG、重试预算或编译诊断。

本文取代旧版“一条固定通用流水线 + Pack 声明所有构建步骤”的解释。旧版 Stage 1 已完成事实仍成立，但不再作为多游戏最终边界。

## 2. Scope / Trigger

下列情况必须使用本合同：

- 新接入一个游戏、Mod 框架或不同构建工具链；
- 为现有游戏增加新的生成、验证、构建、打包或发布路径；
- 当前 Feature 需要写死新的阶段顺序或游戏名称分支；
- Pack 需要表达 Core 当前不认识的游戏能力；
- 一个任务需要跨进程恢复、部分重试或最终原子发布。

下列内容不属于本合同：

- 某个模型或代理的响应格式兼容补丁；
- 某个游戏 API 的临时 Prompt 修辞；
- 将任意 shell、脚本或动态插件权限开放给 Pack；
- 用自动校验替代真实游戏人工行为与视觉验收。

## 3. 当前问题与证据

当前 `composition.generate` 的实际图固定为：

```text
item.000.plan -> item.000.single -> ... -> composition.finalize
-> registered validation -> build -> package -> publish
```

这条路径对当前 STS2 Character 有效，也已经具备 checkpoint、Resume、多 Item repair 和原子发布能力。但它混合了两类不同稳定性：

| 稳定、应跨游戏复用 | 随游戏变化、不应进入 Core |
| --- | --- |
| DAG、claim、CAS、checkpoint、Resume | 是否需要 Plan 或 AI 生成 |
| FIFO、退避、取消、circuit breaker | 生成源码还是编辑数据文件 |
| typed failure、Run、Artifact、hash | 使用 C#、Java、Lua 或其他语言 |
| staging、事务、原子发布、回滚 | 使用 dotnet、Gradle、Godot 或专属工具 |
| 输入输出 schema 校验 | DLL、JAR、PCK 或游戏目录布局 |

2026-08-15 安装态 STS2 closure 还证明：即使 Core 的串行、退避和恢复机制正确，具体生成结果仍可能在 localization 形状或游戏 API 上失败。把更多 STS2 步骤写进通用 Feature 只会扩大耦合，不能解决多游戏差异。

## 4. 分层职责

### 4.1 Core Execution Kernel

建议归属：`ats-runtime`，由 `ats-features` 调用，`ats-workspace` 和 `ats-adapters` 提供持久化与外部端口实现。

Core 负责：

- 校验并执行有向无环图；
- claim、revision CAS、checkpoint、Resume 和 crash recovery；
- 全局/图级队列、速率限制、退避、重试和 circuit breaker；
- RunRecord、ArtifactManifest、输入输出 hash 和 provenance；
- typed cancellation、failure、redaction 和 bounded evidence；
- staging、事务、publish barrier、原子提交和回滚；
- 只运行已注册、受信任、带版本的 Primitive。

Core 禁止认识：

- `character`、`card`、`relic` 等游戏 Item 类型；
- C#、Java、DLL、JAR、PCK、BaseLib 或 Godot；
- 某个 Provider/model 的专用 Prompt 或降级逻辑；
- 某个游戏的目录、hook、资源键或存档语义。

### 4.2 Game Pipeline Contract

建议归属：`ats-game-context`。

该层只定义稳定接口和值对象：

- `GamePipelineProvider` 注册、身份与版本；
- `GameCapabilityId`、`PipelineProfileId` 和 readiness；
- `ResolvedPipelineGraph` 与 typed node schema；
- 输入、输出、checkpoint、retry、validation 和 publish barrier 合同；
- Pack 选择 Provider 时的精确绑定和 hash 校验。

它不实现 HTTP、filesystem、进程调用、Prompt 或某个游戏的 stage。

### 4.3 Game Adapter / Pipeline Provider

建议每个真实游戏使用独立受信任模块，例如后续新增 `ats-game-sts2`；当前 STS2 实现迁移前仍留在既有 Feature/Adapter 代码中。

Provider 负责：

- 判断当前 capability 是否可由该游戏、Pack、Truth 和工具链满足；
- 把产品请求解析为确定性的 `ResolvedPipelineGraph`；
- 定义游戏专属 stage 的输入输出和 validator；
- 组合通用 Primitive 与少量受信任的游戏专属 Primitive；
- 提供游戏专属失败映射和人工验收清单；
- 保证图中不出现未注册 executor 或不受控命令。

STS2 的 source/localization 合并、BaseLib 编译约束、Godot export 和 ZIP 布局属于 STS2 Provider，不属于 Core 通用术语。

### 4.4 Game Pack

建议继续位于 `game_packs/<game-id>/`，由 `ats-game-context` 受控加载。

Pack 负责声明：

- 精确 game/pack identity、version 和 content SHA；
- Item types、fields、locales、references、Resource profiles 和 Truth Queries；
- 允许选择的 `providerId + providerVersion + pipelineProfileId`；
- 模板、静态资源、校验数据和受限参数；
- 当前游戏的用户验收清单。

Pack 不得包含：

- 任意 shell 命令或可拼接命令行；
- 原生动态插件或未注册二进制；
- Provider credential、环境秘密或绝对调用方路径；
- 可绕过 typed node、validation 或 publish barrier 的完整工作流实现。

### 4.5 Product Feature 与 Shell

`ats-features` 继续拥有公开产品能力与用户意图，例如生成一个 Item、生成一个组合、调整一个 Item。Feature 负责请求 Provider 解析图并交给 Core 执行，不再硬编码每个游戏的阶段表。

React/Tauri 只展示：

```text
准备中 -> 生成中 -> 检查中 -> 构建中 -> 可使用
                         |
                         +-> 需要调整 -> 选择具体 Item -> 输入调整词
```

普通用户不需要选择 Primitive、编辑 DAG、阅读 retry class 或控制每个内部 node。

## 5. Signatures

以下合同的 identity、registry、request、node、graph 和 digest 类型已在 `ats-game-context` 实现；capability/output-ref 的进一步通用化仍属于后续工作：

```rust
pub trait GamePipelineProvider: Send + Sync {
    fn identity(&self) -> GamePipelineProviderIdentity;

    fn capabilities(
        &self,
        context: &VerifiedGameContext,
    ) -> Result<Vec<GamePipelineCapability>, PipelineResolutionError>;

    fn resolve(
        &self,
        request: ResolveGamePipelineRequest,
        context: &VerifiedGameContext,
    ) -> Result<ResolvedPipelineGraph, PipelineResolutionError>;
}

pub struct ResolveGamePipelineRequest {
    pub owner_feature_id: FeatureId,
    pub capability_id: GameCapabilityId,
    pub pipeline_profile_id: PipelineProfileId,
    pub root_input: TypedValueRef,
    pub expected_pack_sha256: Sha256Digest,
    pub expected_truth_sha256: Sha256Digest,
}

pub struct ResolvedPipelineGraph {
    pub schema_version: u32,
    pub provider: GamePipelineProviderIdentity,
    pub owner_feature_id: FeatureId,
    pub nodes: Vec<PipelineNode>,
    pub outputs: Vec<TypedOutputRef>,
    pub graph_digest: Sha256Digest,
}

pub struct PipelineNode {
    pub id: ExecutionNodeId,
    pub primitive_id: PrimitiveId,
    pub primitive_version: u32,
    pub consumes: Vec<TypedInputRef>,
    pub produces: Vec<TypedOutputSpec>,
    pub depends_on: Vec<ExecutionNodeId>,
    pub checkpoint_policy: CheckpointPolicy,
    pub retry_class: RetryClass,
    pub validation: Vec<ValidationBinding>,
    pub publish_barrier: PublishBarrier,
}
```

约束：

- `owner_feature_id` 表示谁拥有用户操作和最终结果；node 不是新的 catalog Feature。
- `primitive_id + primitive_version` 必须在受信任 registry 中精确注册。
- `consumes/produces` 使用 schema ID 和 content hash，不使用隐式临时路径传值。
- `depends_on` 必须闭合、无环且顺序确定；相同输入必须得到相同 graph digest。
- `checkpoint_policy` 明确可恢复边界，不能由 executor 临时猜测。
- `retry_class` 只分类失败；具体次数、退避和总预算由 Core policy 固定。
- 任何对最终目录可见的 node 都必须位于同一个 `publish_barrier` 之后。

## 6. Primitive 与 AI 边界

Primitive 是受信任、版本化、有类型的最小执行能力，例如：

```text
model.generate.typed@1
media.generate.raster@1
resource.transform.png@1
files.merge.typed@1
process.dotnet.publish@1
archive.zip.exact@1
sts2.localization.validate@1
sts2.godot.export_pack@1
```

前六项是否真正通用，由第二个真实游戏用例和测试证明；带 `sts2.` 前缀的能力明确留在 STS2 Provider。

AI 不具有特殊编排地位：

- 需要创作时，Provider 可以加入一个或多个 bounded model/media node；
- 只转换现有文件时，图可以完全没有 AI node；
- AI 输出永远先进入 typed validation/checkpoint，不直接写最终工程；
- 技术修复与用户单 Item 调整仍走同一 Provider 解析出的受控子图；
- Core 不按模型名称切换格式、Prompt 或游戏逻辑。

## 7. Pipeline 示例

### 7.1 STS2 Character

```text
resolve typed Item closure
        |
        v
plan/generate each owned source bundle       [optional AI]
        |
        v
merge source + flat localization             [STS2 rule]
        |
        v
validate ownership / API / resources         [STS2 validators]
        |
        v
dotnet publish -> Godot export-pack          [STS2 toolchain]
        |
        v
collect exact DLL/PCK/manifest -> ZIP
        |
        v
one publish barrier -> Artifact + project
```

### 7.2 数据驱动游戏

```text
load typed Item definitions
        |
        v
render JSON/data tables                       [no AI required]
        |
        v
schema + reference validation
        |
        v
copy exact assets -> archive -> publish
```

### 7.3 Java Mod 游戏

```text
generate or update Java/resources             [AI optional]
        |
        v
game-specific validation -> Gradle build
        |
        v
collect JAR/metadata -> publish
```

三条路径复用相同的 graph、Run、checkpoint、retry、Artifact 和事务语义，但不共享一张固定阶段表。

## 8. Validation & Error Matrix

| 条件 | 归属与稳定结果 | 是否执行外部工作 |
| --- | --- | --- |
| Pack 指向未知 Provider/version/profile | `pack.contribution_invalid` | 否 |
| Provider 与 pinned Pack/Truth 不匹配 | `game.pipeline.context_mismatch` | 否 |
| 图有环、缺依赖、重复 node 或 digest 不匹配 | `game.pipeline.invalid` | 否 |
| Primitive 未注册或版本漂移 | `game.pipeline.primitive_unavailable` | 否 |
| node 输入 schema/hash 不匹配 | `game.pipeline.input_invalid` | 否 |
| Provider transport/rate limit | 原始 `model.*`/`media.*`，按 Core bounded retry | 仅当前 node |
| AI typed output 不合法 | Feature feedback policy；无宽松解析 | 仅受控修订 node |
| 游戏 validator 发现 generated-content issue | Provider 归属后形成有界 repair subgraph | 不发布 |
| 本地工具链、锁或配置失败 | typed local failure；不发送给模型 | 不重试 AI |
| checkpoint 与 pinned context 不一致 | `game.pipeline.checkpoint_invalid` | 不猜测恢复 |
| build/package 失败 | graph paused/failed；保留 checkpoint | 不发布 |
| publish 前取消或崩溃 | cleanup/recovery 后 paused/cancelled | 无部分最终目录 |
| publish intent 已提交后崩溃 | Core 只前滚恢复 | 不回到 AI node |
| 人工真实游戏验收失败 | 新 adjustment/revision Run | 不篡改旧 Artifact |

所有失败必须保留 typed stage、retryability、safe action 和脱敏 evidence。Provider body、credential、raw completion 和任意用户路径不得进入 Graph、Run 或 IPC。

## 9. Good / Base / Bad Cases

### Good

- STS2 Provider 解析出 C#、localization、dotnet、Godot 和 ZIP 节点；Core 只按 typed DAG 执行。
- 数据型游戏 Provider 生成一张没有模型节点、没有编译节点的图，仍得到完整 Run 和 Artifact。
- 某个 Item 修复后只替换该 Item 的 checkpoint，随后运行 Provider 声明的完整闭包验证。
- 新增游戏时新增 Provider、Pack 和 fixture，不修改 Runtime 的游戏分支。

### Base

- 单 Item、小输出仍可解析为一至数个 node，不强迫所有请求使用大型 composition 图。
- 多个游戏可以复用相同 Primitive，但各自拥有不同依赖关系、参数 schema 和人工验收项。
- Provider 可以声明一个步骤不可重试；Core 仍统一记录失败、释放 claim 和清理 staging。

### Bad

- 在 Core 中写 `if game_id == "sts2"` 后执行 Godot。
- 让所有游戏固定经过 Plan、Single、Build、PCK 和 ZIP。
- 让 Pack 提供 `command: "..."`、任意参数或动态库路径。
- 因 Provider 不稳定在同一 Run 内自动换模型、endpoint 或 response format。
- 将模型输出直接写入最终工程，再尝试验证或回滚。
- 为兼容旧 Graph 猜测缺失 stage、输入 schema 或 checkpoint 内容。

## 10. Wrong vs Correct

Wrong - Feature 固定跨游戏阶段：

```rust
plan()?;
single_generate()?;
dotnet_build()?;
godot_export()?;
zip()?;
```

Correct - Feature 固定用户意图，Provider 决定阶段，Core 可靠执行：

```rust
let provider = game_pipeline_registry.resolve(&verified_context)?;
let graph = provider.resolve(request, &verified_context)?;
let graph = execution_kernel.validate_and_persist(graph)?;
execution_kernel.run(graph).await
```

Wrong - Pack 注入任意命令：

```json
{ "command": "powershell", "args": ["..."] }
```

Correct - Pack 选择受信任的版本化能力：

```json
{
  "providerId": "sts2",
  "providerVersion": 1,
  "pipelineProfileId": "character_mod_v1"
}
```

## 11. 物理落点

| 位置 | 目标改动 | 明确不放入 |
| --- | --- | --- |
| `ats-kernel` | 稳定 ID、schema/hash value objects | 游戏阶段和 executor |
| `ats-runtime` | 通用 pipeline graph、queue、retry、checkpoint、commit 状态机 | STS2/语言/工具链名称 |
| `ats-game-context` | Provider trait、registry、capability、resolved graph schema、Pack binding | 外部 IO 和产品编排 |
| `ats-workspace` | graph/checkpoint/revision stores 与 publication journal | 游戏规则 |
| `ats-features` | 用户意图、Provider resolution、执行与结果映射 | 固定游戏 stage list |
| `ats-adapters` | HTTP、filesystem、process、archive、media 等通用 port 实现 | Feature 或 Pack 业务判断 |
| `ats-game-sts2`（拟新增） | STS2 Provider、专属 Primitive/validator、图物化 | Core 状态机复制品 |
| `game_packs/sts2` | pinned data、profile、templates、resources、Truth Queries | 任意脚本和 credential |
| Tauri / React | typed command、进度投影、Item 结果与调整入口 | DAG 编辑器和技术策略面板 |

若实践证明一个 Primitive 只有 STS2 使用，就保留在 `ats-game-sts2`；只有第二个真实 Provider 复用并且语义一致时，才提升到通用 Adapter。

## 12. 破坏性迁移计划

用户已允许不保留旧数据兼容。实施时采用 schema cutover，不增加兼容读取或双写：

1. 在 `ats-game-context` 定义 Provider v1、Pipeline Graph v1、typed nodes 和 registry；stable specs 同步为可执行合同。
2. 在 Runtime 抽取通用 graph executor、graph-total policy 和 publish barrier；保留现有 Run/Artifact 不变量。
3. 建立 STS2 Provider，将当前 `plan -> single -> finalize -> validate -> build -> package` 精确迁移为 STS2 图，先做到行为等价。
4. 用不含 C#、Godot、DLL/PCK 和 AI 的 synthetic Provider 证明 Core 中没有 STS2 隐式假设。
5. 将 `composition.generate` 改为请求 Provider 物化图，删除原硬编码阶段表和旧 schema reader。
6. 完成 focused、workspace、DAG、desktop、frontend、GUI、真实 STS2 build/package 和安装态 E2E。
7. 代码变化后构建全新 candidate、全新 Truth/Item/Run/Graph/Artifact；旧候选只保留诊断价值。

每一步都必须直接完成 cutover 或保持尚未接线，禁止同时维护“旧固定流程 + 新 Provider 流程”的长期双主链。

## 13. Tests Required

### Contract tests

- Provider identity/version/profile 与 Pack SHA 精确绑定。
- graph 闭合、DAG、稳定排序、digest、typed input/output 和 Primitive registry。
- 未注册 Primitive、循环依赖、schema 漂移、unsafe path 在执行前拒绝。
- Pack 不能表达任意 shell、credential 或动态 plugin。

### Runtime tests

- node success/failure/cancel、claim/CAS、checkpoint、crash/Resume 和 no-replay。
- FIFO、间隔、bounded retry、circuit breaker 与 graph-total budget。
- publish barrier 前零最终写入；commit intent 后只前滚。
- Run/Artifact/hash、redaction、staging cleanup 和 project lock recovery。

### Provider tests

- STS2 Character 的 source/localization/compile/PCK/ZIP 图和当前输出行为等价。
- synthetic data-only Provider 不调用 AI、dotnet 或 Godot。
- generated-content issue 只能由 owning Provider 分类并形成有界 repair subgraph。
- local/toolchain/configuration issue 不进入模型修复。

### End-to-end gates

- 精确 Feature catalog 和 Cargo dependency DAG。
- workspace tests、check、clippy、frontend tests、TypeScript 和 production build。
- isolated GUI E2E 覆盖创建、暂停、恢复、取消和单 Item 调整。
- 新 candidate 的 fresh installed closure、Artifact/ZIP/最终目录/零残留。
- 用户在真实游戏中完成加载、游玩、奖励、存档恢复和日志验收。

## 14. 影响、风险与授权

### 正向结果

- 不同游戏可以拥有真正不同的构建链路，而不复制可靠性内核。
- AI 失败、工具链失败和游戏规则失败保持分层，不互相伪装成重试。
- Pack 仍然可审查、可哈希、可固定，同时不获得任意代码执行能力。
- 新游戏接入时，改动集中在 Provider + Pack；Core 的变化必须由真实复用需求证明。

### 风险与取舍

- 引入 Provider 合同和 STS2 迁移会触及 Composition Generate 主链，影响面高于局部修复。
- 过度细化 Primitive 会把图变成难维护的内部 DSL；过度粗化又会复制 checkpoint/retry。
- 只有 STS2 一个真实样本时，不能把所有 STS2 helper 提前改名为“通用”。
- Provider 是受信任代码，需要随应用发布；Pack 不能在运行时扩展任意执行能力。

### Candidate 影响

Provider foundation 的跨 crate cutover 和机器门禁已完成，但后续 `rc-20260817T093831Z-b7d8e698ace8` installed closure 证明“模型直接生成原生文件”仍是 Provider 上层的责任泄漏。Behavior IR/Adapter 改造不否定本 ADR 的 Pipeline Provider 边界，而是将“渲染游戏文件”从 AI 移入受信 Game Adapter。任何后续代码变化都需要新提交、新 candidate 和全新物理验收根。

### 需要明确授权的步骤

- 开始跨 crate 的 Provider/Runtime/Feature 代码重构；
- 构建或安装新的 release candidate；
- 发布、push、rebase 或历史改写；
- 操作真实游戏 UI；
- 删除旧 schema 数据或任何历史验证证据。

当前已允许未来 schema 破坏性升级，但该允许不等同于删除历史 evidence；旧目录保持只读证据，不做兼容读取。

## 15. 完成定义

Provider foundation 只有同时满足下列机器条件才算实现，而不是仅凭 ADR 标记 Accepted；当前均已满足：

1. Core 执行图合同不包含游戏、语言、工具链或 AI 必选假设。
2. STS2 Provider 完整承接当前构建链并通过行为等价门禁。
3. 至少一个 data-only synthetic Provider 证明不同阶段图可运行。
4. Pack 只能选择注册能力，无法注入任意执行逻辑。
5. 所有恢复、失败、事务、Artifact 和用户调整不变量继续成立。
上述 cutover 与 foundation 机器门禁已经完成；stable backend/frontend specs 和现有实现是当前可执行事实。父任务当前转入 Behavior IR/Adapter 子任务；该子任务完成后仍须新 candidate fresh installed closure 和用户真实 STS2 验收。这些后续门禁不反向改变 Provider foundation 的机器完成状态。
