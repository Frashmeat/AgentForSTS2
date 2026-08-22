# Prompt、Game Pack 与真相源文本资源总览

> 文档定位：生产 Prompt、Recipe、Pack/Capability Catalog、Truth Evidence、Item/Resource 与模型
> transport 的唯一所有权说明。
>
> 不包含：具体游戏的全部 guidance 文本、Provider 故障流水或当前 candidate 状态。
>
> 架构入口：[`项目架构总览`](./项目架构总览.md)。
>
> 最后更新：2026-08-22

## 1. 请求装配

```text
typed Feature request
  + versioned Recipe
  + verified Pack contribution / Capability Catalog
  + exact ItemDefinition and selected Resources
  + bounded Truth Evidence
  + sanitized Project Context
  + llm.custom_prompt
  + strict typed output contract
  -> ModelRequestSnapshot v2 (内存原文)
  -> ModelRequestCommitment v2 (持久化安全身份)
  -> shared FIFO ModelRequestQueue
  -> Provider HTTP Adapter
  -> protocol decode
  -> Feature domain validation
```

所有 slot 精确匹配、顺序确定并参与 snapshot hash。缺失、重复、未知 slot、schema、Pack/Truth/
Catalog/Adapter identity 或 hash 不匹配必须在 HTTP 请求前失败。

`ModelRequestSnapshot` 不实现 `Serialize/Deserialize`，完整 messages 和 output contract 只在 Provider 请求内存中存在。
持久化 Run、Graph、Behavior checkpoint 和 Artifact provenance 只保存 `ModelRequestCommitment`：它包含
Recipe/Pack/Truth/resource/context binding、消息摘要、输出合同摘要和请求参数，并对不含自身的
canonical identity 计算 `requestSha256`。人工真实验收反馈通过独立 `human.semantic_feedback` slot
和瞬时 worker 输入进入模型，不复用 `llm.custom_prompt`，也不把原文写入证据。

## 2. 唯一所有者

| 内容 | 唯一所有者 | 禁止替代位置 |
| --- | --- | --- |
| 跨游戏任务结构 | `crates/ats-features/recipes/*.json` | Tauri、React、HTTP Adapter |
| request/result 与 domain validation | Feature typed contracts | Prompt prose、Provider schema |
| Item 字段、Resource/reference/profile | Pack v5 `itemTypes` | Runtime、React 游戏分支 |
| Behavior capability 与参数 | Pack v5 Capability Catalog | Prompt 临时发明、模型自由源码 |
| 当前游戏/API/工具链事实 | Truth Snapshot v2 | Pack prose、模型记忆 |
| 工程创作事实 | Stored ItemDefinition v2 | caller-authored Plan 或 completion |
| 媒体选择 | ResourceAsset v2 selected version | 模型返回的路径 |
| confirmed localization | ItemDefinition + deterministic renderer | Behavior response |
| 原生 namespace/class/path/manifest | registered Game Adapter/renderer | 模型 response |
| 用户补充偏好 | `llm.custom_prompt` | Recipe、Pack、硬编码分支 |
| transport 与 response format | Runtime ModelClient + Adapter config | Feature 内 HTTP SDK |

`llm.custom_prompt` 只能补充偏好，不能覆盖 schema、安全、Pack/Truth/Catalog/Adapter identity 或
typed validation。

## 3. Recipe 目录

当前生产 Recipe：

```text
mod-plan.json
mod-generate-single.json
composition-plan.json
composition-suite-brief.json
composition-plan-node.json
composition-retry-node.json
composition-behavior.json
log-analyze.json
```

- `mod-plan`：单 Item typed plan。
- `mod-generate-single`：standalone Single 的独立生成合同；Composition 不调用它。
- `composition-plan` / `composition-suite-brief` / `composition-plan-node`：生成可审阅 Draft。
- `composition-retry-node`：只替换一个 Draft 逻辑节点。
- `composition-behavior`：为一个 pinned Item 输出 strict BehaviorProposal。
- `log-analyze`：使用 Pack 日志贡献和有界日志输入返回 typed analysis。

Build、Package、Project Create、file-based Resource prepare、Behavior render 和 whole-closure
publication 都是本地确定性执行，不需要 Recipe。

Recipe 以编译期 bytes 和 pinned SHA 加载。修改 Recipe 必须同步 hash，并通过 loader/assembly
测试证明 schema、slot、输出预算和渲染顺序。

## 4. 两类生成合同

### Composition

Composition 当前只允许：

```text
BehaviorProposal (schema v1)
  itemId
  itemType
  capabilities[]
    capabilityId
    arguments
    references
```

禁止 Behavior output 出现：

```text
source
relativePath
namespace
className
localization contents
resource paths
project/build/package commands
```

Feature 使用 Pack v5 Catalog 编译 run-scoped strict JSON Schema，要求已声明 capability、参数类型、
required/optional 字段、reference slot 和 `additionalProperties=false`。Provider structured output
只是 transport 约束，Feature 仍必须 strict decode、normalize、validate 和 hash。

### Standalone Single

`mod.generate.single` 仍保留自己的 role-keyed output contract 和 published/
`composition_staged` discriminator，供独立 Single、Batch 和 Complex 使用。它不是 Composition
的 fallback，也不能向 Composition 提供 checkpoint、contribution 或 repair 输入。

新增或修复 Composition 时不得复用 Single native-source response 作为 Behavior/Render 旁路。

## 5. Pack v5 与 Capability Catalog

Pack 加载先校验 schema、pinned SHA、Catalog identity/hash、Adapter identity/implementation SHA 和
Pipeline Provider identity。Pack 只选择已注册受信代码，不能注入任意 shell 或 native plugin。

`itemTypes` 继续拥有：

- canonical/localization fields 和 required locales；
- `evidenceQueries`；
- typed identity/pinned references；
- conditional Resource profiles；
- composition profiles、node groups、dependencies 和 bindings；
- standalone Feature 仍需要的受控 file-role declarations。

`behavior` 额外拥有：

- Catalog ID/version；
- 每类 Item 的 capability allowlist；
- capability arguments、value types、bounds 和 required fields；
- exact registered Adapter ID/version/implementation SHA。

模型看到 Catalog-scoped 语义能力，不看到原生类名、路径、项目结构或 package manifest。

## 6. Truth、Item、Reference 与 Resource

Truth Evidence 只来自 immutable Snapshot v2 的有界 query。Pack guidance、用户描述和模型记忆不能
证明当前 API。Readiness 在模型调用前拒绝缺失 query、Pack/Snapshot mismatch 或 Adapter drift。

ItemDefinition 保存 canonical fields、confirmed localization、behavior intent、Resource bindings 和
typed references。Identity reference 绑定 `itemId + expectedItemType`；pinned reference 绑定
`itemId + definitionHash + quantity`。

模型不得：

- 返回 caller path、Resource current pointer 或任意媒体 bytes；
- 用类名字符串代替 Item reference；
- 修改 confirmed localization；
- 为其他 Item 创建隐式 identity。

Resource prepare 只接受 upload、精确 Pack default 或 AI media source。只有显式 select 才更新
ItemDefinition binding；Composition render 只消费 pinned `resourceId + selectedVersion`。

## 7. Behavior 请求与反馈

每个 Behavior checkpoint 绑定：

```text
itemId + definitionHash
Pack ID/version/SHA
Truth Snapshot ID/SHA
Capability Catalog ID/version/SHA
Adapter ID/version/implementation SHA
ModelRequestSnapshot SHA
normalized BehaviorProposal SHA
```

错误路由：

| 错误 | 处理 | 新语义修订 |
| --- | --- | --- |
| timeout/429/5xx | FIFO 内有界 transport retry | 否 |
| JSON/schema/shape | 当前 Behavior 节点 typed output feedback | 是 |
| capability/argument/reference invalid | 当前 Item IR feedback | 是 |
| Adapter unsupported/invalid | typed local failure | 否 |
| compiler/tool/storage/publication | typed local failure | 否 |

每个已规划 Behavior node 拥有一次 baseline。共享 allowance 只限制额外反馈；
`semanticRequestCount` 记录实际调用，`semanticFeedbackCount` 记录额外修订。普通用户不配置或
理解这些内部计数。

用户单 Item 调整词只进入目标 Item 的新 Behavior 请求，绑定 `itemId +
expectedDefinitionHash`；其他 Item 的 Prompt、Behavior 和 Render checkpoint 不重建。

## 8. Provider Queue 与 Output Budget

Desktop composition root 只持有一个 FIFO `ModelRequestQueue`。一个逻辑请求在所有 HTTP retry
期间占用一个席位；排队取消不发送请求。

`llm.max_output_tokens` 是可选 Provider 上限。有效值在创建 snapshot 前计算：

```text
effectiveMaxOutputTokens = min(recipeMaxOutputTokens, configuredProviderCap?)
```

有效值同时进入 ModelRequestSnapshot/hash、Runtime request 和 HTTP body。系统不自动探测模型、
切换 endpoint/model/response format 或根据错误偷偷改写配置。

## 9. 安全与复验

Checkpoint、Run、Graph、Artifact、IPC 和 verification 不保存 Prompt、raw completion、Provider
body、credential 或未分类绝对路径。反馈 envelope 只保存 versioned typed issue、hash 和有界安全
字段。

至少验证：

- Recipe bytes/SHA、slot、typed output schema 和 snapshot hash；
- Pack/Catalog/Adapter/Truth exact readiness；
- Behavior schema 拒绝 native-authoring 字段和未知 capability；
- 同一 pinned context + IR 产生字节级相同 RenderedItemBundle；
- Adapter/compiler failure 的 ModelClient 调用数保持不变；
- 21 个 baseline 节点不会被 20 次 shared feedback allowance 饿死；
- adjustment 只改变目标 Behavior/Render，随后完整闭包复验；
- Prompt、raw completion、secret 和绝对路径不进入持久化证据。
