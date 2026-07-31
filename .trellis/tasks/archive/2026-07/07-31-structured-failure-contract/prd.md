# 结构化错误契约与脱敏边界

> 状态：completed
>
> 优先级：P0
>
> 开发类型：fullstack
>
> 方案来源：[`桌面后端运行时加固与发布收口方案`](../../../docs/03-当前方案/2026-07-31-桌面后端运行时加固与发布收口方案.md) Work Order 2

## 1. 目标

在不建立第二套错误模型的前提下，将 Work Order 1 的最小 `ActionableFailure` 扩展为 Core、RunRecord、Tauri command 和 React 共用的结构化错误契约：稳定分类、稳定错误码、明确恢复动作、有限白名单上下文和统一脱敏诊断。

## 2. 已确认范围

- Core 定义 `FailureCategory`、`RecoveryAction`、完整 `ActionableFailure`、`FailureNormalizer` 和统一脱敏器。
- `CommandFailure` 直接封装或复用同一 `ActionableFailure` serde 形状，不复制字段映射。
- 首批覆盖 LLM、Image、Package、Project、Toolchain、ImageProc 和 Run 错误。
- 所有 Tauri command 使用统一安全兜底；未知错误映射为 `core.unclassified`，不得裸传原始错误字符串。
- React 使用统一 `ActionableErrorNotice` 渲染 message、恢复动作和可选诊断标识，不再由页面直接拼接 `String(error)`。
- Run handler 失败写入与 command 同构的 `ActionableFailure`，不得恢复 `error: String` 第二真相源。
- 同步 backend code-spec、架构总览、当前进度和当前运行时方案。

## 3. 契约基线

```text
ActionableFailure
  schemaVersion = 1
  code
  category = configuration | authentication | rate_limit | network
           | upstream | validation | filesystem | toolchain
           | state | internal
  stage
  message
  action = configure | reauthenticate | retry | check_path
         | install_dependency | open_settings | none
  retryable
  retryAfterMs?
  context?          # 仅允许显式白名单字段和标准化路径
  diagnostic?
    id
    summary         # 已脱敏、有限长度
    ioKind?
```

`CommandFailure` 与 `RunRecord.failure` 必须序列化同一 `ActionableFailure`；若 Tauri IPC 需要外层 envelope，只允许增加传输层标记，不复制领域字段。

## 4. 首批稳定错误码

| 领域 | 稳定码 |
| --- | --- |
| LLM | `llm.config_missing`、`llm.authentication_failed`、`llm.rate_limited`、`llm.network_failed`、`llm.upstream_failed` |
| Image | `image.config_missing`、`image.authentication_failed`、`image.rate_limited`、`image.network_failed`、`image.upstream_failed`、`image.empty_result` |
| Package | `package.source_missing`、`package.output_invalid`、`package.permission_denied`、`package.required_file_missing`、`package.path_escape` |
| Project | `project.not_open`、`project.locked`、`project.closing`、`project.close_timeout`、`project.path_invalid` |
| Toolchain | `toolchain.not_configured`、`toolchain.not_found`、`toolchain.version_failed` |
| ImageProc | `image_proc.not_ready`、`image_proc.model_failed`、`image_proc.runtime_failed` |
| Run | `run.interrupted`、`run.storage_failed`、`run.not_found` |
| Fallback | `core.unclassified` |

## 5. 验收方向

- [x] Rust 与 TypeScript 的 category/action/schema 字段完全一致。
- [x] RunRecord failure 和 Tauri CommandFailure 使用同一序列化结构。
- [x] 首批领域错误具有确定性的 code/category/action/retryable 映射。
- [x] provider body、authorization header、token、URL query/fragment、完整 Prompt 和未标准化用户根路径不会出现在 IPC、RunRecord、diagnostics 或 UI。
- [x] 401、429、网络失败、路径缺失、权限拒绝、工具链缺失和未知错误均有定向测试或 typed 映射覆盖。
- [x] 未知错误只产生有限的 `core.unclassified` 摘要和 diagnostic ID。
- [x] React 页面不再直接使用 `String(error)` 作为用户错误契约。
- [x] Core、Tauri、TypeScript 和相关前端测试通过定向检查。
- [x] 稳定文档和 backend code-spec 同步。

## 6. 不纳入

- Work Order 3 的 `ProjectSession`、关闭工程 cancel-and-drain、跨 command repository 所有权和崩溃 reconciliation。
- Work Order 4 的 BuildInfo、ML provenance、baseline/ML 安装版。
- Web 壳功能补齐；只保留未来将同一失败契约映射为 HTTP 的边界。
- 完整遥测、远程日志上传、错误聚合服务或自动清理 diagnostics。
- 为旧字符串错误前端保留长期 fallback。
- 重要依赖升级或新的错误处理框架依赖。

## 7. 调查结论

### 7.1 当前错误丢失链

```text
typed Core error
  -> handler / Tauri map_err(error.to_string())
  -> Tauri InvokeError JSON string
  -> React String(error)
  -> page-local Notice
```

- `LlmError` 和 `ImageGenError` 已保留 auth、rate limit、HTTP status、transport、config 等可分类 variant；当前 provider message 可能来自上游 body，不能直接进入 IPC 或持久化。
- `ProjectError`、`RunError`、`GodotValidationError` 和 `LocalPropsError` 已保留 variant 及 `io::Error`，可以不解析英文 message 完成分类。
- `ImageProcError` 当前只保留 decode/encode/unsupported/quality；ML model/runtime 错误在部分路径中过早折叠成字符串，需要在本任务内保留稳定 variant。
- `package_project.rs` 的打包、事务和路径错误仍是 `String`，必须先改为 typed `PackageError`，否则无法可靠区分 source missing、permission denied、required file missing 和 path escape。
- Tauri commands 大量返回 `Result<T, String>`；`commands/mod.rs` 还记录了“前端只能拿字符串”的错误旧假设。
- Tauri 2.11 的 `InvokeError` 对任意 `Serialize` 类型实现 `From<T>`，因此可以直接拒绝一个结构化 JSON object；`CommandFailure` 可使用 `#[serde(transparent)]` 包装 `ActionableFailure`，不会增加第二层字段。
- `tauriApi.ts` 当前直接调用 `invoke`，React 多处调用 `String(error)`；`api.ts` 和 `webApi.ts` 也会抛出普通 `Error` 或包含 response body 的字符串。

### 7.2 相关规范与代码模式

- `.trellis/spec/backend/error-handling.md`：错误必须保留类型事实并在边界映射。
- `.trellis/spec/backend/quality-guidelines.md`：RunRecord/ArtifactManifest、图片、工具链和 Game Pack 现行契约。
- `.trellis/spec/frontend/quality-guidelines.md`：前端统一错误入口和纯逻辑测试要求。
- `.trellis/spec/guides/cross-layer-thinking-guide.md`：Core -> Tauri -> TypeScript -> React 字段往返门禁。
- `LlmError::{is_retryable,retry_after_secs}`：已有 retry 元数据模式。
- `RunRepository::transition`：持久化前统一 schema 校验模式。
- Tauri 2.11 `ipc::InvokeError`：自定义 `Serialize` error 会作为 JSON reject value 发送给前端。

### 7.3 预计修改文件

- 新建 `crates/ats-core/src/failure.rs`：唯一 failure schema、校验、脱敏、diagnostic ID 和 normalizer。
- 修改 `platform/domain/models.rs`、`handlers/common.rs` 和各 handler：Run failure 接受结构化错误，不再先转字符串。
- 修改 `llm/client.rs`、`image_gen/client.rs`、`image_proc/{mod,ml}.rs`：补齐稳定映射所需的 typed metadata。
- 修改 `package_project.rs`：引入 `PackageError` 并保留 `io::ErrorKind`、声明相对路径和阶段。
- 新建 `src-tauri/src/commands/failure.rs`，并迁移所有返回 `Result<_, String>` 的 command 到 `CommandResult<T>`。
- 修改 `src/services/tauriApi.ts`：定义同构 TypeScript 类型、统一 `invokeCommand` 和 runtime shape guard。
- 新建 `src/components/ActionableErrorNotice.tsx` 与前端纯函数模块；迁移页面和卡片的错误 state。
- 修改 `webApi.ts` / `api.ts`：desktop-only、HTTP 和缺失导出错误进入同一安全 shape，但不扩展 Web 功能。
- 新增 Core/Tauri/frontend 定向测试并同步稳定文档。

## 8. 技术约束

- 不把 STS2 专属错误规则放入通用 failure domain。
- 不通过匹配完整英文 message 建立核心分类；优先匹配现有 enum variant、HTTP status 和 `io::ErrorKind`。
- diagnostic/context 使用显式白名单，不接受任意 `serde_json::Value` 或 map 透传。
- message、diagnostic summary 和 context value 必须有限长。
- 实施前完成现有错误调用链调查、跨层 schema、错误矩阵和 Good/Base/Bad 门禁。

## 9. 技术方案

### 9.1 唯一 Core 契约

将 `ActionableFailure` 从 platform domain 提升到 Core 根级 `failure` 模块，RunRecord 直接引用该类型。字段使用有限 enum 和显式 context struct：

```text
FailureContext
  provider?
  httpStatus?
  runId?
  projectRelativePath?
  settingKey?
  dependency?

FailureDiagnostic
  id
  summary
  ioKind?
```

不接受任意 map。`FailureDiagnostic.id` 用于关联现有 run-scoped diagnostics 或安全 tracing 记录；本任务不新增 Audit、错误数据库或第二份 lifecycle repository。

### 9.2 Typed normalizer

`FailureNormalizer` 以领域 enum variant、HTTP status 和 `io::ErrorKind` 为输入，输出完整 `ActionableFailure`。上游 message/body 只作为内存内部错误，不进入输出。未知错误走 `core.unclassified`，message 和 diagnostic summary 使用固定安全文案，不能调用未知错误的 `Display` 生成用户内容。

路径脱敏只允许：工程相对路径、固定依赖名、setting key 和安全 provider ID。绝对路径不写入 failure；URL context 必须通过 URL parser移除 query/fragment；已知 secret 在任何允许进入 diagnostic 的文本上先执行精确替换，再执行长度限制。

### 9.3 Tauri 边界

```rust
#[serde(transparent)]
pub struct CommandFailure(pub ActionableFailure);

pub type CommandResult<T> = Result<T, CommandFailure>;
```

所有 fallible Tauri command 返回 `CommandResult<T>`。已知 Core error 使用对应 normalizer；mutex poison、spawn join 和其他无法安全分类的错误只传 stage 进入 `core.unclassified`。无 fallible 返回值的 command 不强制包 Result。

### 9.4 React 边界

`invokeCommand<T>` 捕获 Tauri reject value，使用严格 shape guard 转为 `ActionableFailure`；非法 shape 转为前端本地 `core.unclassified`，不显示 `String(error)`。页面 state 保存 `ActionableFailure | null`，统一通过 `ActionableErrorNotice` 展示：

- message、恢复动作提示和可选 diagnostic ID；
- `none` 不显示无效恢复动作；
- 本 Work Order 不新增跨页面导航或通用重试 callback 协议。

Run timeline/list 直接消费 `RunRecord.failure`，不再把它降级为 string。

## 10. 方案取舍

### 采用：Core 契约 + typed normalizer + 透明 IPC newtype

- 优点：Run、Tauri 和 React 只有一个字段模型；分类依据可测试；未来 Web 可直接映射 HTTP。
- 代价：需要一次性迁移所有 fallible Tauri command 和前端错误 state，改动面较大。

### 不采用：在每个 Tauri command 手工构造 JSON

- 会重复 code/category/action 映射，Run 与 command 很快产生漂移。

### 不采用：保留 String 并由正则/英文 message 推断

- provider body、路径和 secret 已经混入字符串；分类不稳定，也无法证明脱敏。

## 11. 实施顺序

1. Core schema、校验、长度限制、context 白名单、diagnostic 和脱敏单元测试。
2. LLM/Image/Project/Run/Toolchain/ImageProc normalizer；为 Package 与 ML 补齐 typed error。
3. handler 公共 finalize API 改为接收 `ActionableFailure`，逐类迁移 Run 失败路径。
4. Tauri `CommandFailure/CommandResult` 和全部 fallible command 迁移，增加序列化与 canary 测试。
5. TypeScript schema guard、统一 invoke wrapper、`ActionableErrorNotice` 和页面迁移。
6. 旧字符串错误入口扫描、跨层定向验证和稳定文档同步。

每一步必须保持 Core/Tauri/TypeScript 可编译；不保留旧 command 或旧 failure 字段 fallback。

## 12. 验证矩阵

| 场景 | 预期 |
| --- | --- |
| Good：LLM 401 | `llm.authentication_failed`、`authentication`、`reauthenticate`，不包含 provider body |
| Good：Run 持久化失败 | `run.storage_failed`、`filesystem`，保留受控 `ioKind` |
| Base：LLM 429 带 Retry-After | `rate_limit`、`retry`、`retryAfterMs` 精确换算 |
| Base：工程相对文件缺失 | `check_path`，context 只含工程相对路径 |
| Bad：provider body 带 token/URL query/Prompt canary | IPC、RunRecord、diagnostic、UI 序列化均不含 canary |
| Bad：未知错误 Display 含绝对路径和 secret | 返回 `core.unclassified` 固定摘要和 diagnostic ID，不调用 Display 对外 |
| Bad：前端收到字符串或畸形 reject | 转为本地安全 `core.unclassified`，不直接渲染原值 |
| Bad：Package 声明路径逃逸或 symlink | 对应稳定 package code；不暴露绝对根路径 |

## 13. 最低定向验证

```text
cargo test -p ats-core failure::
cargo test -p ats-core platform::application::handlers::package_project::tests
cargo test -p ats-core platform::application::handlers::asset_generate::tests
cargo test -p ats-core --test run_lifecycle
cargo check -p ats-core
cargo check -p agentthespire-desktop
npm run test:frontend
npx tsc -b --pretty false
```

- 不运行 workspace 全量测试、workspace clippy、生产前端构建或 Tauri bundle，除非发现跨 crate 编译风险需要扩大并先取得授权。
- 不执行真实游戏 UI；错误契约不改变已验收的 Game Pack 行为与资产。

## 14. Definition of Done

- PRD 验收项全部有代码、测试或静态扫描证据。
- Core/Tauri/TypeScript 的 schema 和序列化样例一致。
- 生产 Tauri commands 不再返回 `Result<_, String>`。
- 生产 React 调用链不再把未知 reject 通过 `String(error)` 直接展示。
- backend code-spec 包含真实签名、字段、错误矩阵、Good/Base/Bad 和定向测试。
- 架构总览、当前进度和运行时方案与实现一致。

## 15. 扩展边界

- 为未来 Web HTTP 映射保留稳定 category/code，但本任务不新增 Web endpoint。
- 为未来 diagnostics/telemetry 保留 correlation ID，但本任务不新增持久化审计或远程上报。
- Work Order 3 的 `project.closing`、`project.close_timeout` 先进入枚举/映射表，不在本任务伪造尚未存在的生命周期行为。
