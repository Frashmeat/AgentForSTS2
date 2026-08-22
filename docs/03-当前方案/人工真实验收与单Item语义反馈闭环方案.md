# 人工真实验收与单 Item 语义反馈闭环方案

> 文档定位：定义“机器生成成功不等于真实游戏语义验收通过”的产品和架构合同，以及人工发现单个 Item 行为偏差后的定向重调闭环。
>
> 事实依据：2026-08-22 真实 STS2 Character 验收已证明 Mod、Character、PCK 和 BaseLib 加载正常，但多张 Card 的可编译 Behavior 未完整实现已确认的 `behaviorIntent`。
>
> 权威入口：[`current plan`](./当前方案.md)、[`Typed Behavior IR`](./Typed-Behavior-IR与Game-Pack确定性生成架构方案.md) 和 Trellis 任务 `08-22-human-semantic-feedback-closure`。
>
> 最后更新：2026-08-22

## 1. 决议摘要

当前问题不应通过 Card 特例、C# 补丁、更大模型重试预算或自动猜测玩法正确性解决。底层合同应当是：

```text
ItemDefinition / behaviorIntent  = 已确认的预期语义
BehaviorProposal                 = AI 对预期语义的受限表达
Game Adapter                     = IR 到游戏文件的确定性转换
Machine gates                    = 结构、资源、编译、构建、打包和证据正确
Human acceptance                 = 真实玩法是否符合已确认的预期
```

系统不得把 `Run succeeded` 或编译通过表述为人工语义验收通过。人工反馈只能指向一个已确认 Item，触发该 Item 的 Behavior 重调、本地重渲染和整个 closure 复验。

## 2. 现有能力与真实缺口

### 2.1 已经正确的地基

当前实现已具备：

- `submit_composition_item_feedback` 绑定 succeeded source Graph revision、`itemId`、`expectedDefinitionHash` 和 `expectedBehaviorSha256`；
- 只激活目标 Behavior 节点，并确定性替换目标 Render checkpoint；
- 保留其他 Item 的已成功 checkpoint；
- 调整后重新执行 whole-closure validate/build/package/publish；
- 从 succeeded 结果调整时派生新 Graph/Run/Artifact，不篡改源证据；
- stale definition、无变化 Behavior hash 和不可归属的本地错误安全停止。

### 2.2 必须收口的缺口

1. 自动 `BehaviorFeedback` 只能发现 JSON/schema/Capability 问题，无法证明真实玩法符合预期。
2. 用户调整词当前复用 `runtime.custom_instructions`，没有独立的人工语义诊断身份。
3. 旧 `ItemAdjustment` 会把原始 `instruction` 写入 Run/Graph payload，与现行脱敏合同冲突。
4. Behavior checkpoint 当前持久化完整 `ModelRequestSnapshot`，其中包含已渲染请求。持久化证据应收敛为 identity/hash，不应保存完整 Prompt。
5. UI 仅显示泛化的 `Regenerate item`，没有明确区分“纠正已确认意图的实现偏差”与“修改 Item 设计”。

## 3. 权威与反馈边界

### 3.1 人工反馈是诊断证据，不是新定义

本闭环只处理：

```text
已确认预期：5 伤害并施加 1 层虚弱
真实结果：只造成 5 伤害
人工反馈：缺少已定义的虚弱效果
```

人工反馈不能直接修改：

- canonical fields；
- `behaviorIntent`；
- localization；
- Resource/reference bindings；
- ItemType 或 Composition 结构。

如果用户想改变设计，必须回到 Item/Draft 编辑并生成新 definition hash。不得利用反馈通道制造“定义说 A、实现做 B”的隐式状态。

### 3.2 机器成功与人工验收

```text
machine succeeded
       |
       v
ready for human acceptance
       |
       +-- 符合预期 --> 本轮人工验收证据由任务/候选流程记录
       |
       +-- 不符合预期 --> 向具体 Item 提交语义偏差反馈
```

产品 Runtime 不伪造“AI 自己确认玩法正确”的 acceptance state。真实游戏验收结论仍由人工提供。

## 4. 目标流程

```text
                    +-----------------------------+
                    | confirmed ItemDefinition    |
                    | behaviorIntent = authority  |
                    +--------------+--------------+
                                   |
                                   v
                         machine generation
                                   |
                                   v
                    validate / build / package
                                   |
                                   v
                         immutable candidate
                                   |
                          human real-game test
                                   |
                 +-----------------+-----------------+
                 |                                   |
              accepted                         mismatch found
                                                     |
                                                     v
                                      select exactly one Item
                                                     |
                                                     v
                                      bounded human feedback
                                                     |
                                                     v
                                  derive new Graph / Run / Artifact
                                                     |
                                                     v
                                    rerun target Behavior only
                                                     |
                                                     v
                                      deterministic re-render
                                                     |
                                                     v
                                      whole-closure revalidation
```

## 5. 数据合同

### 5.1 IPC 瞬时输入

`submit_composition_item_feedback` 取代语义模糊的 `adjust_composition_item`，不保留兼容别名：

```rust
struct HumanSemanticFeedbackInput {
    source_execution_graph_id: ExecutionGraphId,
    expected_source_revision: u64,
    item_id: ItemId,
    expected_definition_hash: Sha256Digest,
    expected_behavior_sha256: Sha256Digest,
    instruction: String, // bounded; IPC/task memory only
}
```

`instruction` 必须 trim 后非空、不含控制字符且最多 4,000 字符。原文只存在当前 IPC 调用、当前 worker 内存和当次 Provider 请求中。

Tauri 不得再让 worker 只从已持久化的 `RunRecord.request` 反解全部执行输入。命令在持久化安全 Run/Graph 前构建一个不实现 `Serialize/Deserialize` 的执行伴生输入：

```rust
enum EphemeralCompositionInput {
    None,
    HumanSemanticFeedback {
        feedback_ref: HumanSemanticFeedbackRef,
        instruction: String,
    },
}
```

worker closure 捕获该值，并单独传入 `Stage2Composition::execute` 和 Composition Feature。`RunRecord.request` 只包含安全的 feedback ref；Feature 在发起模型请求前校验伴生输入与 ref 的 hash 一致性。

### 5.2 持久化安全引用

```rust
struct HumanSemanticFeedbackRef {
    source_execution_graph_id: ExecutionGraphId,
    source_revision: u64,
    item_id: ItemId,
    expected_definition_hash: Sha256Digest,
    expected_behavior_sha256: Sha256Digest,
    instruction_sha256: Sha256Digest,
    created_at: DateTime<Utc>,
}
```

Run、Graph、checkpoint、Artifact、IPC projection 和 release evidence 只持久化这个引用，不保存原始 instruction。

### 5.3 Model 请求承诺

单独保存一个 `requestSha256` 无法证明它来自当时的 Recipe、Pack、Truth、输出合同和请求配置。`ModelRequestSnapshot` 因此破坏性升级为“内存原文 + 可重算安全承诺”：

```rust
struct ModelRequestCommitmentIdentity {
    schema_version: u32,
    feature_id: FeatureId,
    recipe: RecipeRef,
    game_pack: ModelGamePackRef,
    truth_snapshot_id: Option<Sha256Digest>,
    selected_resources: Vec<ModelResourceRef>,
    context_bindings: Vec<ModelContextBinding>,
    rendered_messages_sha256: Sha256Digest,
    output_contract_schema: SchemaRef,
    output_contract_sha256: Sha256Digest,
    max_output_tokens: u32,
    temperature_bits: Option<u32>,
    requested_model: Option<String>,
}

struct ModelRequestCommitment {
    identity: ModelRequestCommitmentIdentity,
    request_sha256: Sha256Digest,
}

struct ModelContextBinding {
    role: String,
    schema: SchemaRef,
    sha256: Sha256Digest,
}
```

`temperatureBits` 使用已通过有限范围校验的 `f32::to_bits()` 结果，避免跨序列化器的浮点文本差异。`requestSha256` 仅对不包含自身的 `ModelRequestCommitmentIdentity` canonical representation 求 hash，不存在循环定义。`contextBindings` 按 `role + schema` 稳定排序并拒绝重复 key；`selectedResources` 沿用稳定排序和 Resource ID 唯一性合同。内存中的 `ModelRequestSnapshot` 在 HTTP 前必须同时证明：

1. 完整 messages 可重算为 `renderedMessagesSha256`；
2. 完整 JSON Schema 可重算为 `outputContractSha256`；
3. 安全承诺域可重算为 `requestSha256`；
4. `human.semantic_feedback` 的 context binding 精确匹配 `HumanSemanticFeedbackRef.instructionSha256`。

`ModelRequestSnapshot` 删除 `Serialize/Deserialize` 实现，由类型系统限制为进程内对象；只有 `ModelRequestCommitment` 可序列化。Behavior checkpoint 序列化 commitment、response model 和 TokenUsage。TokenUsage 是响应证据，不参与 request identity。Graph node request hash、commitment 和 Artifact provenance 必须精确一致；checkpoint 可以验证承诺内部完整性和复用 succeeded checkpoint，但不声称能从承诺重建 Prompt。

### 5.4 Repair cause

ExecutionGraph 不再使用 `adjustment: Option<_>` 猜测是人工还是自动修复，改为封闭 tagged enum：

```rust
enum RepairCause {
    AutomaticTypedFeedback {
        validation_fingerprint: Sha256Digest,
    },
    HumanSemanticFeedback {
        feedback_ref: HumanSemanticFeedbackRef,
    },
}
```

Human cause 必须恰好一个 target。不另行持久化 `inputRequirement`：它从 target status 和 active Behavior checkpoint 唯一派生，避免两份状态漂移。`Pending/Active` 表示仍需要原文，`Completed` 必须同时存在 replacement Behavior checkpoint 并表示原文已消费；`ExecutionGraph::validate` 拒绝所有交叉组合。其首次人工调整请求是该 derived Graph 的 baseline；如果 replacement 仍产生 JSON/IR typed 问题，后续修订才消耗共享/per-node feedback allowance。Automatic cause 保留现有 typed feedback 计数语义。

## 6. 恢复与安全停止

人工反馈没有持久化原文，因此它与可重建的机器 typed feedback 不同。恢复必须按目标 Behavior checkpoint 是否已提交分阶段处理：

```text
Human target Pending / Active
  -> replacement Behavior checkpoint 尚未持久化
  -> 原文仍是必需输入
  -> canResume=false
  -> 从 source succeeded Graph 重新提交

Human target Completed
  -> replacement Behavior checkpoint 已持久化
  -> feedback 原文已消费
  -> Render/Validate/Build/Package/Publish 可通用 Resume
```

- 同一进程内的 timeout/429/5xx 仍可在 FIFO 队列中有界重试；
- Pending/Active human target 在项目关闭、应用重启或 worker 丢失后不得仅根据 hash 恢复模型请求；
- 这类 Graph 投影 `canResume=false` 和 typed `composition.feedback.input_unavailable`；
- Completed human target 后的本地或发布阶段失败可正常 Resume，不丢弃已持久化的 replacement Behavior；
- 用户重新提交时只校验当前 source revision、definition hash 和 Behavior hash，不要求匹配以前某个失败 derived Graph 的 instruction hash；
- source 必须是 succeeded；人工反馈不允许对 paused source 原地调整。

这个取舍符合“不保留原始反馈数据”的当前决策，同时保证源 Artifact 和 source Graph 始终可恢复。

## 7. Behavior 请求优先级

Behavior Recipe 新增独立 slot：

```text
human.semantic_feedback
```

它不与 `runtime.custom_instructions` 或自动 `behavior.feedback` 复用：

| 输入 | 职责 |
| --- | --- |
| `runtime.custom_instructions` | 项目级补充偏好，不能改写定义 |
| `behavior.feedback` | JSON/schema/Capability 的自动 typed 问题 |
| `human.semantic_feedback` | 真实玩法偏离已确认预期的单 Item 诊断 |

Recipe 必须要求：

1. `ItemDefinition.behaviorIntent` 仍是权威预期；
2. 人工反馈只指出实现偏差，不能替换定义；
3. 返回当前 Item 的完整 replacement invocation list，不返回 patch；
4. 保留未被反馈指出的已确认行为；
5. 如果 replacement 使用 Catalog 无法表达的 capability，本地 IR validator 返回 typed 失败，不发明原生代码。

系统只能确定性证明 ItemDefinition 字节/hash 未变且 replacement IR 仍绑定该 definition；它不声称能在模型请求前理解 free-text 是否暗含设计变更。“修改设计”必须由结构化 UI 操作返回 Item/Draft 编辑，不使用第二个模型分类反馈。

## 8. 模块职责

| 模块 | 本方案职责 | 不负责 |
| --- | --- | --- |
| React / Shell | 显示“待人工验收”，选择单 Item 并收集有界反馈 | 判断游戏逻辑、展示 Graph/Prompt/预算 |
| Tauri command | 校验 source Graph/revision/Item/hash，分离瞬时输入与持久化引用 | 操作真实游戏 UI |
| `ats-features::composition_generate` | 编排目标 Behavior 重调、Render 替换和 closure 复验 | 持有游戏特例或 HTTP retry |
| `ats-runtime` | 派生 Graph、CAS、checkpoint、source evidence 不变性和安全停止 | 理解 Card/Relic 或自然语言 |
| Game Pack | 声明 Capability Catalog 和可表达范围 | 保存反馈或调度重试 |
| Game Adapter | 将 replacement IR 确定性渲染为当前游戏文件 | 请求模型或猜测用户意图 |
| Game Pipeline Provider | 执行当前游戏的 validate/build/package | 判断创作预期 |

不同游戏可有不同的 Capability、Adapter 和 Pipeline，但共用同一人工反馈包络、Graph 派生和安全停止语义。

## 9. 用户交互

默认界面只呈现结果和用户需要做的事：

```text
机器检查已通过
请在真实游戏中确认内容是否符合预期。

Lacerate
[反馈行为偏差]  [修改 Item 设计]

行为偏差
+------------------------------------------------+
| 实际只造成 5 点伤害，没有施加已定义的 1 层虚弱。 |
+------------------------------------------------+

[提交反馈并重新生成此项]
```

界面不暴露：

- Prompt slot；
- Behavior/Render node；
- semantic request count；
- compiler/API 诊断；
- Graph revision 或 Artifact 派生细节。

本任务不增加无持久化意义的“结果符合预期”按钮。没有反馈不会修改产品状态；真实 candidate 验收结论仍记录于 Trellis/验收证据。“修改 Item 设计”是结构化导航，不提交反馈模型请求。

## 10. 失败路由

| 情况 | 处理 | 是否请求 AI |
| --- | --- | --- |
| 人工报告已定义行为缺失/错误 | 目标 Behavior 重调 | 是，单 Item |
| 反馈与 definition/Behavior hash 过期 | `composition.feedback.stale` | 否 |
| 用户选择修改结构/定义 | 返回 Item/Draft 编辑，产生新 definition | 否 |
| Catalog 无法表达已确认语义 | `behavior.capability_unsupported` | 有界停止 |
| replacement Behavior hash 不变 | `composition.feedback.no_progress` | 停止 |
| Adapter 不能渲染合法 IR | `game.adapter_unsupported/invalid` | 否，本地修复 |
| 编译/构建/资源/存档错误 | owning local failure | 否 |
| 重启后 Human target 尚未提交 Behavior | `composition.feedback.input_unavailable` | 否，需重新提交 |
| Human target 已提交 Behavior，后续本地失败 | 通用 Resume 继续 Render/Validate/Deliver | 否 |

## 11. 破坏性迁移

本项目地基不保留补丁式兼容路径：

1. ModelRequestSnapshot、Composition Generate request/result、Blueprint、Behavior checkpoint 和 ExecutionGraph 使用新 schema version。
2. 删除持久化原始 `ItemAdjustment.instruction` 的合同。
3. 删除 Behavior checkpoint 中的完整 `ModelRequestSnapshot`，仅保留可重算的 `ModelRequestCommitment`。
4. `ExecutionRepairCampaign.adjustment: Option<_>` 替换为 tagged `RepairCause`，删除 paused source 人工原地调整路径。
5. 不为旧 Graph 提供 reader、resume 或 command alias；旧数据原样保留为历史证据。
6. 任何代码变化都使当前 candidate 失效，必须使用新 candidate、Truth、Project、Run、Graph、Artifact 复验。

## 12. 验证方案

### 12.1 Contract / Runtime

- IPC 接收有界反馈，持久化 JSON 只出现 feedback hash/ref。
- stale source revision、definition hash 或 Behavior hash 在模型请求前失败。
- succeeded source 派生新 Graph，source Graph/Run/Artifact 字节不变。
- 目标 Behavior/Render checkpoint 变更，其他 Item checkpoint 不变。
- 无变化 Behavior hash 安全停止，不发布新 Artifact。
- Pending/Active human target 重启后不使用 hash 伪造反馈原文，且 `canResume=false`。
- Completed human target 重启后可复用 replacement Behavior 继续本地阶段。
- persisted Run/Graph/checkpoint/Artifact JSON canary 不包含反馈原文或完整 Prompt。
- 内存 Snapshot 可从原文重算 messages/output-contract digests；持久化 Commitment 可从 safe fields 和 opaque content digests 重算 request identity，篡改任一字段都失败。
- Snapshot 类型不可序列化；commitment identity 不包含自身 hash，bindings/resources 稳定排序并拒绝重复。
- Human target status 与 replacement Behavior checkpoint 存在性组合由 Graph validation 强制，不存在第二份 input requirement 状态。

### 12.2 Feature / Prompt

- `human.semantic_feedback` 与 global custom instructions、automatic typed feedback 完全分离。
- 请求继续绑定 ItemDefinition、Pack、Truth、Catalog 和 Adapter identity。
- 反馈仅作用于目标 Item，返回完整 replacement IR。
- 未声明 capability、参数、引用和 native-source 字段仍严格拒绝。

### 12.3 Shell / E2E

- UI 明确显示“机器通过，待人工真实验收”。
- 用户只选择具体 Item 和输入行为偏差，不操作内部 Graph。
- 提交后监控新 Graph/Run，不把源 Artifact 显示为已被覆盖。
- 使用 synthetic Model 证明 target-only retry 和全闭包复验。
- 使用 fresh candidate 交由用户验证一张已知偏差 Card 的修正前后行为。

## 13. 实施顺序

1. 定义 `EphemeralCompositionInput`、persisted feedback ref 和可重算 `ModelRequestCommitment`。
2. 破坏性升级 ModelRequestSnapshot identity、Runtime `RepairCause` 和 Composition checkpoint schema。
3. 将人工反馈从 `runtime.custom_instructions` 分离到独立 Recipe slot。
4. 改造 Tauri command/worker 伴生输入，只从 succeeded source 派生新执行证据。
5. 实现 Pending/Active 与 Completed human target 的分阶段 Resume 投影。
6. 收口 Composition Studio 的待验收状态和单 Item 反馈交互。
7. 同步 stable specs、Prompt 总览、当前进度和 Trellis。
8. 完成 focused tests 后再申请全量机器门禁与 replacement candidate 授权。

## 14. 完成条件

- 机器成功和人工语义验收在产品文案和持久化合同中完全分离。
- 真实游戏偏差只能通过具体 Item 反馈进入模型。
- 反馈不能改写 ItemDefinition，设计变更必须回到 Item/Draft。
- 原始反馈、完整 Prompt 和 Provider body 不进入 Run/Graph/checkpoint/Artifact。
- 源 Graph/Run/Artifact 不变，修正结果通过新 Graph/Run/Artifact 发布。
- 其他游戏可复用同一反馈闭环，而保留各自的 Capability、Adapter 和 Pipeline。
- 当前已知 Card 逻辑偏差经人工反馈、新 candidate 和真实 STS2 复验后被确认修正。
