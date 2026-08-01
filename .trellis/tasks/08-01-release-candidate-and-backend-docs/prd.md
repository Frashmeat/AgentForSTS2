# Windows 发布候选与文档规范收口

> 状态：in_progress
>
> 优先级：P0
>
> 开发类型：fullstack/build/docs
>
> 方案来源：[`桌面后端运行时加固与发布收口方案`](../../../docs/03-当前方案/2026-07-31-桌面后端运行时加固与发布收口方案.md) Work Order 5

## 1. 目标

建立唯一、受控、可复验的 Windows release candidate 入口，把前端、Rust workspace、baseline/ML 安装包、产物身份与验证结论收敛为机器可读的 `release-verification.json`；同时把架构总览、当前进度和后端 code-spec 更新为当前 Rust/Tauri + Platform DDD + Game Pack 架构事实。

## 2. 已确认边界

- 唯一候选入口预期为 `scripts/release-candidate.ps1 -Variant All`，复用 `build.ps1` 已建立的 BuildInfo、variant 隔离和 release manifest，不复制构建身份逻辑。
- 验证失败必须返回非零退出码并写入明确的失败结论；不得生成或保留可被误认作成功的摘要。
- `release-verification.json` 是本次候选验证汇总，不替代 ArtifactManifest、RunRecord 或每个 variant 的 `release-manifest.json`。
- GitHub 普通 Windows CI 只做 default/ML compile/check 门禁；installer 构建保留在显式发布流程，不让每次 push 承担完整出包成本。
- 当前任务允许破坏性删除失效的 Python/FastAPI 主架构描述，不保留双主路径兼容叙述。
- 不修改已通过真实游戏验收的 STS2 Game Pack 资源、模板、规则和 package layout，除非新门禁证明存在直接不一致。
- 不引入新的发布平台、签名服务、自动部署或依赖升级。
- workspace 全量 test/clippy、前端生产构建、baseline/ML 完整 Tauri bundle 和安装验收属于高成本最终门禁；实施脚本和定向验证后，执行前再次向用户说明成本并取得确认。

## 3. 功能需求

### 3.1 Release candidate 总入口

- 支持 `Baseline`、`Ml`、`All` 三种 variant 选择；默认行为必须明确，不能从陈旧目录推断 variant。
- 在执行前记录候选 identity 和计划；commit、variant、features、build_id 必须来自受控构建入口。
- 按固定顺序执行前端检查、Rust workspace 门禁、variant build、release manifest/installer 哈希复验和汇总。
- 子步骤失败后停止后续依赖步骤，同时保留已执行步骤的事实和失败分类。
- 支持不执行昂贵构建的 plan/dry-run 或可注入 fixture 路径，供脚本定向测试验证控制流与文件边界。

### 3.2 `release-verification.json`

- 至少包含 schema version、candidate identity、仓库 commit、请求的 variants、开始/完成时间、总体状态和逐步骤状态。
- 每个 variant 关联其 release manifest、BuildInfo identity、installer 相对路径/大小/SHA-256 和复验结论。
- 失败记录稳定 step id、exit code 和可行动摘要；不得写入密钥、完整本机配置或未经界定的原始日志。
- 只有全部必需步骤通过时总体状态才允许为 `succeeded`。
- 写入使用临时文件加原子替换；中断或异常不得留下结构合法但状态虚假的成功文件。

### 3.3 Windows CI

- 现有 Rust CI 增加 Windows default 与 `ml-rembg` 的 compile/check 覆盖。
- 普通 push/PR 不构建 installer，不下载或运行真实游戏，不要求签名证书。
- CI 命令与本地 release candidate 入口使用同一 Cargo feature/variant 约束，不复制相互矛盾的 feature 组合。

### 3.4 架构与 code-spec 文档

- 重写 `docs/01-总览/项目架构总览.md`，以 Rust/Tauri 入口壳、`ats-core`、Platform DDD、Game Pack、Truth Snapshot、ProjectSession、外部 adapter 和 ArtifactManifest 为主路径。
- 同步 `docs/02-现状/当前进度说明.md`、统一任务清单和当前总方案，删除已经通过验收的过期叙述。
- `.trellis/spec/backend/` 必须包含可执行的错误契约、Run CAS/timeline、ProjectSession 生命周期、文件事务、ArtifactManifest/provenance、脱敏边界和 release candidate 命令契约。
- code-spec 必须指向真实文件/API/字段，包含校验与错误矩阵、Good/Base/Bad 和必需测试，不写只有原则的空泛描述。

## 4. 停止条件

- 任一必需步骤失败、缺失或未执行时，不得将总体状态记为 `succeeded`。
- runtime BuildInfo、release manifest、installer SHA-256 或当前 commit 无法唯一关联时，停止候选流程。
- baseline/ML 从共享或非本次 build_id 目录收集产物时，停止候选流程。
- ML variant 未实际启用 `ml-rembg`，或 baseline 声明 ML feature 时，停止候选流程。
- release verification 中出现密钥、本机完整配置或未经脱敏的原始错误时，停止交付。
- 架构总览仍把 Python/FastAPI 描述为桌面主路径时，不得完成文档收口。

## 5. 验收标准

- [x] `scripts/release-candidate.ps1` 成为唯一受控总入口，并复用 `build.ps1` 的真实构建身份。
- [x] fixture/dry-run 测试覆盖 Baseline、Ml、All、步骤失败、陈旧产物、identity/hash 不一致和原子汇总写入。
- [x] `release-verification.json` 准确表达每个步骤及每个 variant 的成功/失败事实，失败时不会出现虚假成功摘要。
- [x] Windows CI 覆盖 default/ML compile/check，普通 push/PR 不构建 installer。
- [x] 架构总览、当前进度、统一任务清单和总方案与 Work Order 1-5 的当前事实一致。
- [x] 后端 code-spec 达到可执行契约深度，并删除失效的 Python/FastAPI 主路径描述。
- [x] PowerShell、文档链接/格式及相关定向测试通过。
- [x] 获得单独授权后，前端/workspace/Windows/ML/installer 最终门禁通过并生成可复验候选；未获授权时明确保持未执行。
- [ ] 用户对当前候选执行最终安装/启动冒烟；Agent 不操作安装器或桌面应用 UI。

## 6. Good / Base / Bad

- Good：`-Variant All` 在同一 commit 上依次生成 baseline/ML 隔离候选，所有 installer 哈希重算一致，verification 总体为 `succeeded`。
- Base：只请求 `Baseline`，verification 只包含 baseline，不读取或报告旧 ML 目录。
- Base：以 dry-run/fixture 验证完整控制流，不启动 Tauri bundle；结果明确标记为测试计划而非正式候选。
- Bad：ML 构建失败；verification 记录失败步骤并返回非零，不继续生成成功摘要。
- Bad：release manifest 指向陈旧 installer 或 BuildInfo 不一致；候选流程立即失败。
- Bad：CI 为普通 push 构建两个 installer，造成不必要的发布成本。
- Bad：文档同时保留 Python/FastAPI 与 Rust/Tauri 两套“主架构”。

## 7. 预计影响范围

- `scripts/release-candidate.ps1`（新增）
- `scripts/test-release-candidate.ps1` 或同等 PowerShell fixture 测试（新增）
- `build.ps1`（仅在总入口复用契约需要最小扩展时修改）
- `scripts/ci-local.ps1`
- `.github/workflows/rust-ci.yml`
- `.trellis/spec/backend/`
- `docs/01-总览/项目架构总览.md`
- `docs/02-现状/当前进度说明.md`
- `docs/03-当前方案/当前方案.md`
- `docs/03-当前方案/2026-07-31-桌面后端运行时加固与发布收口方案.md`

## 8. 调查与实施顺序

1. 调查 `build.ps1`、现有 PowerShell 测试、`ci-local.ps1`、Windows CI 和 release manifest 契约。
2. 固化 release candidate CLI、verification schema、step DAG、错误矩阵和 fixture 注入边界。
3. 实现总入口、原子 verification 和定向 PowerShell 测试。
4. 增加 Windows default/ML CI 门禁，验证普通 push 不出 installer。
5. 重写架构/现状/code-spec 文档并执行链接、格式和契约审查。
6. 完成定向门禁后，汇报全量验证预计成本并申请执行最终 release candidate。

## 9. 不纳入

- 代码签名证书申请、SmartScreen 信誉、自动发布或部署。
- 修改 Tauri application identifier 或支持 baseline/ML 并排安装。
- 真实游戏 UI 自动化。
- Game Pack Stage 2+ 新能力或 STS2 内容扩展。
- 修复本任务调查中发现但与候选正确性无直接关系的 UI 易用性问题；这些问题单独记录。

## 10. 调查结论与可执行契约

### 10.1 现有复用边界

- `build.ps1 -Variant Baseline|Ml [-PlanOnly] [-BuildId]` 已经负责 Git commit、variant/feature 组合、隔离 target/bundle/release 路径、最终 GUI BuildInfo 握手和 release manifest 发布；总入口不得复制这些规则。
- baseline 与 ML 可以共用同一个 build ID，因为现有路径分别为 `artifacts/build/<variant>/<build-id>/target` 和 `artifacts/release/<build-id>/<variant>`。总 verification 因而固定写到 `artifacts/release/<build-id>/release-verification.json`。
- `scripts/release-build-lib.ps1::Publish-IsolatedBundle` 已提供 installer 白名单、目录 containment、复制后 SHA-256 和 manifest 原子写入模式；verification 复验应遵循相同路径与 UTF-8 no-BOM 约定。
- `scripts/test-build-plan.ps1` 展示了 PowerShell fixture、临时目录 containment 和安全清理方式；新测试复用这一模式，不运行 Tauri build。
- `scripts/ci-inner.sh full` 与 GitHub Linux jobs 已包含 frontend、workspace check/test/clippy。Windows 新 job 只补 desktop baseline/ML compile，不复制 installer 流程。

### 10.2 Release candidate CLI

```text
scripts/release-candidate.ps1
  -Variant Baseline|Ml|All   # default: All
  [-BuildId <bounded-id>]
  [-PlanOnly]
```

- 未显式提供 build ID 时，总入口生成一次 `rc-<UTC>-<commit12>`，并把同一个 ID 传给所有请求的 variant。
- 总入口先调用每个 variant 的 `build.ps1 -PlanOnly -BuildId <id>`；commit、features 和输出路径以这些 plan 为准。
- 非 PlanOnly 模式要求所有 plan 的 `workingTreeClean=true`，并拒绝已存在的 candidate root。
- 固定步骤顺序为 `preflight`、`frontend-tests`、`frontend-build`、`workspace-check`、`workspace-test`、`workspace-clippy`，然后按请求执行 `build-<variant>` 与 `verify-<variant>`。
- 每个真实命令由封闭 step ID 映射选择；CLI 不接受任意 command、scriptblock 或 caller-supplied Cargo feature。

### 10.3 Verification schema v1

```text
ReleaseVerification {
  schemaVersion: 1,
  candidate: {
    commit: string,
    buildId: string,
    requestedVariants: ["baseline" | "ml"]
  },
  status: "running" | "succeeded" | "failed",
  startedAt: UTC timestamp,
  completedAt?: UTC timestamp,
  steps: [{
    id: stable string,
    status: "pending" | "running" | "succeeded" | "failed" | "skipped",
    startedAt?: UTC timestamp,
    completedAt?: UTC timestamp,
    exitCode?: integer,
    summary?: bounded stable string
  }],
  variants: [{
    variant: "baseline" | "ml",
    features: string[],
    buildId: string,
    releaseManifest: repo-relative string,
    runtimeBuildInfoVerified: boolean,
    artifacts: [{ relativePath, byteLength, sha256 }],
    verified: boolean
  }],
  failure?: { stepId: string, exitCode?: integer, summary: bounded stable string }
}
```

- verification 在创建、步骤开始、步骤结束和最终终态时通过同目录临时文件加原子替换更新。
- `running` 文件可以在进程异常中断后保留，明确表示未完成；只有所有必需步骤和请求 variant 复验通过时才写 `succeeded`。
- step failure 使用脚本拥有的稳定 summary；命令 stdout/stderr 只输出到当前控制台或显式日志，不进入 verification。
- variant 复验重新解析 release manifest，逐字段核对 commit/variant/features/buildId，验证每个相对路径 containment、文件大小和 SHA-256。`runtimeBuildInfoVerified=true` 只在 `build.ps1` 成功返回且该 manifest 复验通过后写入。

### 10.4 定向测试矩阵

- Good：All fixture 的 baseline/ML manifest 和 installer 哈希均匹配，最终 status 为 succeeded，两个 variant 使用同一 build ID。
- Base：Baseline-only plan/result 不读取旧 ML 目录；PlanOnly 不创建 candidate root。
- Bad：注入的必需 step 返回非零，当前 step failed、后续 steps skipped、总体 failed，进程返回非零。
- Bad：manifest commit/variant/features/buildId、artifact path/size/hash 任一不一致，`verify-<variant>` 失败。
- Bad：candidate root 已存在、工作区 dirty 或 caller 传入非法 build ID，在昂贵步骤前失败。
- 原子性：每次写入后 JSON 可解析，终态不存在 `.tmp` 残留，失败记录不包含测试 canary secret/path。

## 11. 自动实施与定向验证结果

截至 2026-08-01，已完成以下自动范围：

- 新增 `scripts/release-candidate.ps1`、`release-candidate-lib.ps1` 和 fixture 测试；PlanOnly 复用 `build.ps1` 的 commit、feature、build ID 和隔离路径计划。
- verification 对每个状态变化执行同目录原子写入；失败步骤、后续 skipped、稳定摘要、manifest identity/path/size/hash、空或错误 variant 和最终 requested variants 精确集合均有门禁。
- `.github/workflows/rust-ci.yml` 新增 Windows baseline/ML compile job；fixture 静态断言该 job 包含两种 `cargo check` 且不包含 `tauri build`。
- Rust/Tauri 架构、后端 code-spec、Prompt/Game Pack/Truth Snapshot 资源边界和当前进度已按当前代码事实同步。

已通过：

```text
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/test-release-candidate.ps1
pwsh -NoProfile -File scripts/test-build-plan.ps1
PowerShell parser check（Windows PowerShell 5.1 语法）
git diff --check
Trellis JSON parse
changed Markdown relative-link check
active documentation stale-path search
```

完整候选门禁的两次失败尝试均保留为不可变 verification：

- `rc-20260801T133725Z-40a014d79b19` 在 `workspace-test` 停止。根因是两条 `run_lifecycle` 测试仍断言 Work Order 2 已禁止暴露的内部错误文本；产品结构化失败与回滚行为正确。测试改为断言 `ActionableFailure.code/stage/diagnostic` 和脱敏结果后，定向 6/6 通过。
- `rc-20260801T134159Z-3d31211c5433` 通过 frontend、workspace check/test，在 `workspace-clippy` 停止。Rust 1.94 的严格 lint 暴露 Core 13 项和其后 Desktop 31 项派生警告；按最小根因方案修正显式文件打开语义、机械 lint、RunResult 大 variant 和透明 IPC failure 的内存表示。

修正后 `cargo clippy --workspace --all-targets -- -D warnings`、Core 全目标测试和 Desktop 全目标测试通过；`Box<PlanItem>`、`Box<ActionableFailure>` 的 JSON/IPC 透明性有明确回归断言。

第三次候选 `rc-20260801T142915Z-3c88bf28c2b3` 绑定提交 `3c88bf28c2b3fba182e589cdf24e5c75b8b66f17`，总入口退出码为 0：10/10 steps succeeded，baseline/ML 2/2 variants 的 BuildInfo 与 release manifest 独立复验通过，四个 installer 的大小和 SHA-256 可重新计算一致，原子临时文件残留为 0。机器证据位于：

```text
artifacts/release/rc-20260801T142915Z-3c88bf28c2b3/release-verification.json
```

四个 installer 均为 `NotSigned`，符合本阶段未签名候选边界。尚未执行且不得据此声称通过：该候选的人工安装/启动冒烟。
