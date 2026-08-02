# Stage 2 Work Order 3：Game Context 与 Resource Workspace

## Goal

在不切换当前 `ats-core` v1 Game Pack/Truth 生产链的前提下，建立 Stage 2 的可验证
Game Context、Contribution Resolver、Truth Evidence 和 Resource Workspace 合同。STS2 与最小
synthetic Pack 必须使用同一 Loader/Resolver；用户上传、AI 生成和 Pack 默认资源必须进入同一
版本、选择和 provenance 流程。后续 WO4-WO6 只消费本 Order 的 verified handles，不重新解释原始
JSON 或资源文件。

## Ownership

- `ats-game-context`：Game Pack schema v2、pinned content identity、Contribution Registry/Resolver、
  Truth Snapshot schema v2、Evidence query、`VerifiedGameContext` 和存储 ports。
- `ats-workspace`：`ResourceInput`、`ResourceAsset`、immutable version、selection、provenance 和 repository port。
- `ats-adapters`：文件 Game Pack/Truth/Resource repository；只实现 IO、hash、路径与原子文件行为。
- `ats-kernel`：继续只提供跨域 ID/schema/hash 值对象；不加入 Pack/Resource 工作流。

Game Context/Workspace/Adapters 不得依赖 `ats-features` 或旧 `ats-core`。Game Context 不读取工程资源，
Workspace 不解释游戏尺寸、角色或 Prompt。

## Game Pack And Contribution Contract

```text
GamePackManifest schemaVersion=2
  id / displayName
  contributions[]
    slotId / featureId / schema / requiredPrimitives[] / payload(object)

LoadedGamePack
  validated manifest
  sha256 of exact accepted bytes
```

- Loader 接收调用方 pinned SHA；exact bytes 不匹配时在注册前失败。
- Pack ID、display name、slot、Feature/schema/Primitive ID、object payload 和重复项全部校验。
- Resolver 输入 `featureId + required slot/schema list + available Primitive set`；缺 slot、Feature 不匹配、
  schema 不兼容或 Primitive 不可用时在 Feature 执行前失败。
- `VerifiedContributionSet` 只能按已解析 slot 和 exact schema typed decode；不公开 unchecked payload 入口。
- 新 `game_packs/sts2/stage2-game-pack.json` 是迁移目标 manifest；旧 `game-pack.json` 在 WO7 前继续服务生产链，
  两者不得互相冒充 schema 或 hash。

## Truth And Evidence Contract

```text
TruthSnapshotManifest schemaVersion=2
  snapshotId / gamePack identity
  sources[]: id / relativePath / bytes / sha256
  indexes[]: id / provider / relativePath / records / sha256
  toolVersions / createdAt

TruthEvidenceRecord
  source / symbol / purpose / boundedExcerpt / relativePath
```

- `snapshotId` 是不含 `createdAt` 和自身 ID 的规范 identity SHA-256。
- 文件 adapter 从 current pointer 打开固定 Snapshot，验证 Pack identity、manifest identity、source/index hash、
  byte/record count、路径和 symlink 后才构造 `VerifiedTruthSnapshot`。
- Evidence query 只在该 verified snapshot 的 bounded records 上执行，limit 有界，结果顺序确定；一次
  `VerifiedGameContext` 固定同一 Pack/Snapshot identity。
- 当前旧 Truth Snapshot v1 不原地升级，也不复制成 v2 冒充新证据。

## Resource Workspace Contract

```text
ResourceInput
  logicalRole
  origin: userUpload | aiGenerated | packDefault
  mediaType
  source file

ResourceAsset schemaVersion=1
  resourceId / logicalRole / origin
  originalVersion / selectedVersion
  versions[]: content identity / parent? / blob / provenance
```

- 原件与派生版本按内容 SHA-256 标识且不可覆盖；相同 bytes 可幂等识别，不得伪造新内容。
- AI provenance 记录 registered provider、model 和 request hash；Pack default 记录 Pack identity 和
  contribution slot；用户上传只记录稳定来源种类，不持久化原绝对路径。
- 选择只能指向当前 Asset 的已验证版本。重新生成/派生产生新 version；选择变化不修改旧 blob。
- 文件 repository 使用工程内 `.ats/resources/`、同目录临时文件和 atomic manifest replace；拒绝 symlink、
  路径逃逸、坏 hash、重复/篡改版本和越权选择。

## Validation And Error Matrix

| 条件 | 结果 |
| --- | --- |
| pinned STS2 与 synthetic Pack | 同一 Loader/Registry/Resolver 成功 |
| Pack exact bytes hash 错误或 schema 不兼容 | 注册前 typed failure |
| required contribution 缺失/错 schema/缺 Primitive | Resolver 在 Feature side effect 前失败 |
| Snapshot Pack identity、snapshotId、source/index hash 错误 | 不构造 verified handle |
| Evidence query 为空、超 limit 或含越权路径 | typed validation failure |
| user/AI/Pack default ResourceInput | 同一 repository 产生可复验 Asset/version/provenance |
| 相同 bytes 再摄取 | 内容 identity 稳定，不覆盖原 blob |
| 派生版本或 selection 不属于 Asset | 拒绝且 manifest 不变 |
| symlink、外部 manifest path、坏 hash | 拒绝，不产生成功状态 |

## Good / Base / Bad

- Good：STS2 和 synthetic manifest 都以 pinned bytes 加载；同一 Feature 的 required slots 解析为
  typed contribution，固定 Snapshot query 返回 bounded evidence，三类资源来源都能版本化和选择。
- Base：旧 `ats-core` v1 Pack/Truth 和当前 Shell 保持编译运行；新 v2 fixture 不宣称是当前真实 Run 产物。
- Bad：Feature 直接读取 Pack JSON、按 `game_id` 分支；Workspace 硬编码 STS2 图片角色；adapter 接收
  未校验 hash 后构造 verified handle；新版本覆盖旧 blob 或 selection 指向外部文件。

## Required Machine Gates

```text
cargo test -p ats-kernel
cargo test -p ats-game-context
cargo test -p ats-workspace
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

- [x] STS2 与 synthetic Pack 通过同一 pinned Loader 和 Registry。
- [x] Contribution Resolver 对缺 slot、错 schema 和缺 Primitive 给出 typed failure。
- [x] Truth Snapshot 的 Pack/content identity、source/index hash 和 Evidence query 可复验。
- [x] `VerifiedGameContext` 在一次使用中固定 Pack/Snapshot，不暴露未验证构造器。
- [x] 用户上传、AI 生成和 Pack 默认资源进入同一 version/provenance/selection 合同。
- [x] 原件/版本 immutable，symlink/path/hash/selection 反例不会留下伪成功。
- [x] 新 crate 不依赖 Feature/旧 Core，当前文档明确旧生产链尚未切换。
- [x] 全部机器门禁通过后提交并自动进入 Work Order 4。

## Machine Evidence

2026-08-02 最终代码通过：

- `cargo test -p ats-kernel`：4 passed。
- `cargo test -p ats-game-context`：8 passed。
- `cargo test -p ats-workspace`：3 passed。
- `cargo test -p ats-adapters`：12 passed；当前 Windows 环境覆盖 Truth/Resource symlink 拒绝。
- Stage 2 dependency DAG self-test 与真实 workspace check 均通过；目标 crate 无 Feature/旧 Core 依赖。
- 变更 Rust 文件 `rustfmt --check`、pinned manifest `eol=lf` 属性和 `git diff --check` 通过。
- `cargo check --workspace --all-targets` 通过。
- `cargo test --workspace --all-targets` 通过；包含旧 Core 370 项、桌面 20 项及全部新 crate/集成测试。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。

未转换旧 Truth Snapshot、未切换当前 Shell、未执行网络 refresh、release candidate 或真实游戏验收。

## Out Of Scope

- 切换当前 Tauri/Web/CLI、工程 schema 或旧 Pack/Truth runtime。
- 执行网络下载、反编译 refresh 或真实游戏 Snapshot v2 转换。
- Prompt Recipe、日志分析、单 Mod 或图片生成纵切面。
- 删除旧 `game-pack.json`、旧 Truth Snapshot、用户工程或任何验证证据。
- 构建新的 Windows release candidate。
