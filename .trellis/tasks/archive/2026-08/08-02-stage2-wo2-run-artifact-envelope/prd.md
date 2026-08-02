# Stage 2 Work Order 2：Run 与 Artifact 解耦

## Goal

建立不包含产品 Feature 枚举或游戏领域类型的新 Run/Artifact 合同，并用 typed Feature Registry 和文件 Artifact adapter 证明新增 Feature 无需修改 Runtime 中心类型。当前 `ats-core` RunRecord v2/ArtifactManifest v2 在本 Order 继续服务旧生产链，后续纵切面逐项迁移，WO7 切换 Shell 后删除。

## Ownership

- `ats-runtime`：Run lifecycle、versioned payload、failure summary、Artifact 基础 manifest、publisher port。
- `ats-features`：FeatureSpec、typed request/result decode、Feature Registry。
- `ats-adapters`：文件 Artifact Store、hash、staging 和同目录原子发布。
- `ats-kernel`：Feature/Schema/Failure ID、SchemaVersion、Sha256Digest。

Runtime 和 Adapter 不得 import `ats-features`、Game Pack、Truth、codegen、image processing 或旧 `ats-core` 类型。

## Run Contract

```text
RunRecord schemaVersion=3
  id
  featureId
  request: VersionedPayload
  status / timestamps / attempts / timeline / progress
  failure?: RunFailure
  result?: VersionedPayload
```

- `VersionedPayload::from_typed(schema, value)` 是运行时构造入口，只接受序列化为 JSON object 的 typed value。
- 读取持久化 payload 后必须由 `FeatureRegistry` 按 `featureId + SchemaRef` 解码；未知 Feature、schema 不匹配和 payload 字段错误全部失败。
- Runtime terminal transition 只维护状态/CAS 不变量，不判断产品结果枚举。
- 成功必须有 result 且无 failure；失败必须有 failure 且无 result；取消两者都没有。

## Artifact Contract

```text
ArtifactManifest schemaVersion=3
  artifactId / artifactKind / featureId / producingRunId / createdAt
  contexts[]: VersionedPayload
  provenance[]: VersionedPayload
  featureExtension: VersionedPayload
  files[]: role / snapshotRelativePath / publishedRelativePath? / byteLength / sha256
```

- Runtime 只验证 ID、schema envelope、文件记录和基础结构。
- Feature Registry 在发布前验证 Feature extension；具体 Context/Provenance 由后续 owning registry 验证。
- File Artifact Store 只接收基础 request 和 staged files，不 import 上层领域类型。
- 发布继续使用 `artifacts/<artifact-id>/runs/<run-id>/.staging-* -> final` 同目录原子 rename。
- 只重试 Interrupted、Windows PermissionDenied 和 OS 5/32/33；有界退避后失败并清理 staging，不复制、不伪造成功。

## Validation And Error Matrix

| 条件 | 结果 |
| --- | --- |
| 注册 Feature 的 typed request/result | Registry decode 成功 |
| 新增第二个 Feature fixture | 不修改 `ats-runtime` enum 或 match |
| 未注册 Feature | `UnknownFeature` |
| request/result schema 不匹配 | `SchemaMismatch` |
| payload 非 object 或字段错误 | 构造/Decode 失败 |
| 非法 Run transition | 保持原状态并返回 typed lifecycle error |
| Artifact 路径逃逸、symlink、空文件集、坏 hash | 发布前失败 |
| 原子 rename 瞬态冲突 | 有界重试后成功或 typed I/O failure |
| 确定性 rename 错误 | 不重试，清理 staging |
| final 已存在 | 拒绝覆盖不可变 Artifact |

## Good / Base / Bad

- Good：独立 fixture Feature 注册自己的 request/result schema，完成 Run 成功 round-trip 和 Artifact extension 发布，Runtime 不增加分支。
- Base：旧 `ats-core` v2 生产链继续编译运行，新合同尚不伪装成 Shell 当前格式。
- Bad：Runtime 定义 `RunKind`/`RunResult` 产品枚举，Artifact Store import Game Pack/Image/Codegen，或直接把任意 `serde_json::Value` 送入业务逻辑。

## Required Machine Gates

```text
cargo test -p ats-runtime
cargo test -p ats-features
cargo test -p ats-adapters
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
rustfmt --edition 2024 --check <changed Rust files>
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

## Acceptance Criteria

- [x] Runtime Run contract 不含产品枚举，Feature Registry 是 typed decode 唯一入口。
- [x] 新 Feature fixture 不修改 Runtime 即可完成 request/result/Artifact extension 验证和 lifecycle。
- [x] ArtifactManifest 只有基础字段、versioned records 和 Feature extension。
- [x] 文件 Artifact Store 位于 Adapters 且不 import 上层领域类型。
- [x] 原子发布、重试、失败清理和不可变 final 由机器测试证明。
- [x] 当前文档明确区分新合同已落地与旧生产链尚未切换。
- [x] 全部机器门禁通过后提交并自动进入 Work Order 3。

## Machine Evidence

2026-08-02 最终代码通过：

- `cargo test -p ats-runtime`：11 passed。
- `cargo test -p ats-features`：2 passed。
- `cargo test -p ats-adapters`：5 passed；当前 Windows 环境实际覆盖 symlink source 拒绝。
- Stage 2 dependency DAG self-test 与真实 workspace check 均通过。
- 变更 Rust 文件 `rustfmt --check` 和 `git diff --check` 通过。
- `cargo check --workspace --all-targets` 通过。
- `cargo test --workspace --all-targets` 通过；包含旧 Core 370 项、桌面 20 项及全部新 crate/集成测试。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。

未执行新的 release candidate、安装器或真实 Mod 验收；WO2 没有切换当前生产 Shell。

## Out Of Scope

- 迁移 Game Pack/Truth/Resource Workspace。
- 迁移任何生产 Feature handler 或 Prompt。
- 修改 Tauri/React DTO 和历史 v2 文件。
- 删除旧 Run/Artifact 实现。
- 构建安装器或修改现有验证证据。
