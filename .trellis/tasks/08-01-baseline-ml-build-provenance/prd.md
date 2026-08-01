# baseline/ML 安装版与 provenance

> 状态：completed
>
> 优先级：P0
>
> 开发类型：backend/fullstack/build
>
> 方案来源：[`桌面后端运行时加固与发布收口方案`](../../../docs/03-当前方案/2026-07-31-桌面后端运行时加固与发布收口方案.md) Work Order 4

## 1. 目标

让 baseline 与 ML 两种 Windows 安装候选具有互不污染的构建目录和可核验身份，并把图片处理实际使用的 processor/model/runtime/fallback 事实写入唯一 `ArtifactManifest`。运行时 `BuildInfo`、图片 provenance 与构建产物 manifest 必须能唯一关联同一 commit、variant、feature set、build_id 和产物 SHA-256。

## 2. 已确认边界

- baseline 与 ML 保持同一个 Tauri application identifier，不支持并排安装。
- 两种 variant 使用独立 target/bundle 输出目录，不从共享目录枚举产物。
- 全局 ML prewarm 不访问活动工程，也不注册为 ProjectSession Run。
- ML 失败可以让 `BgRemoverChain` 使用 heuristic fallback，但不能绕过图片质量门禁、静默交付原图或把 fallback 结果记作 ML 验收通过。
- Run result 继续只引用 Work Order 1 的 `ArtifactManifest`；不新增平行 provenance 文件。
- 不修改已验证的 STS2 Game Pack 资源、规则、模板、build recipe 或 package layout，除非新门禁证明存在直接不一致。
- 完整 Tauri build、installer、workspace 全量测试/clippy 和人工安装启动验收必须另行说明成本并取得用户确认。

## 3. 需求

### 3.1 图片处理 provenance

- `ImageProcClient::remove_background` 返回结构化 `ImageProcOutcome`，至少包含处理后 PNG、实际 processor、可选 model SHA、可选 runtime 版本、是否 fallback 及稳定 fallback 原因。
- `SimpleBgRemover`、`MlBgRemover` 和 `BgRemoverChain` 都生成真实 outcome，不由调用方根据配置猜测 processor。
- `ArtifactManifest.imageProcessing` 保存 outcome provenance，并与已有图片质量报告/结论分离。
- ML primary 失败后 simple fallback 成功时，manifest 明确记录实际 processor 为 simple 和 fallback 原因；不得宣称 ML 处理成功。

### 3.2 ML prewarm 单飞与重试

- prewarm 状态增加 attempt、最近一次结构化失败和当前加载状态。
- 新增单飞 retry command；并发 retry 只能共享/拒绝同一次初始化，不能并发下载、解压或构造 ORT session。
- 成功重试原子替换 image processor；失败保持 heuristic fallback 可用，并保留可行动失败。
- 工程关闭、切换和 AppShutdown 不等待全局 prewarm，但 prewarm 不得写工程目录或发布工程 artifact。

### 3.3 BuildInfo

- 编译时嵌入 `BuildInfo { commit, variant, features, build_id }`，开发构建使用明确、可识别且不冒充候选的默认值。
- health/capabilities 暴露同一个 BuildInfo；图片处理 provenance 引用相同运行时身份。
- variant 与 feature 组合受验证：baseline 不得声明 ML feature，ML 候选必须实际启用 `ml-rembg`。

### 3.4 baseline/ML 构建与 release manifest

- `build.ps1`（或其已有等价入口）为 baseline/ML 设置独立 Cargo target 和 Tauri bundle 目录。
- 每个 variant 只收集本次隔离目录内的产物，计算 SHA-256，并生成机器可读 manifest。
- manifest 至少包含 schema version、commit、variant、features、build_id、构建时间和每个产物的相对路径/大小/SHA-256。
- 构建参数把同一 commit/variant/features/build_id 注入运行时；禁止在构建完成后猜测或重写身份。
- 两种 variant 的产物与 manifest 不互相覆盖，不从共享旧目录拾取陈旧 installer。

## 4. 停止条件

- ML 图片质量通过但 provenance 显示 simple/fallback 时，不得记为 ML 验收通过。
- ML prewarm 失败时不得绕过质量门禁或把原图作为正式资源。
- runtime BuildInfo、ArtifactManifest provenance 与 release manifest 不能唯一绑定同一 build 身份时，不得进入安装验收。
- baseline/ML 任一构建仍从共享 target/bundle 目录枚举产物时，不得进入安装验收。
- release manifest 中任何产物哈希无法现场重算一致时，不得标记候选成功。

## 5. 验收标准

- [x] Simple、ML、ML→Simple fallback 三条路径产生准确、可序列化的 `ImageProcOutcome`。
- [x] 图片 ArtifactManifest 区分质量结论和 processor provenance，并记录当前 BuildInfo。
- [x] prewarm retry 单飞；并发调用不会重复下载/初始化，attempt 与 last failure 可观察。
- [x] baseline/ML 的 BuildInfo 与实际 Cargo feature 组合一致，health/capabilities 返回同一身份。
- [x] baseline/ML 使用独立 target/bundle 目录，release manifest 只枚举对应 variant 的新产物。
- [x] manifest 的 commit、variant、features、build_id 和 SHA-256 可与运行时/产物交叉核对。
- [x] Core/Desktop/TypeScript/PowerShell 定向测试及相关 code-spec、架构/现状文档同步。

### Good / Base / Bad

- Good：ML variant 使用 u2netp 成功处理图片；manifest 同时记录 processor=ML、model SHA、ORT 版本和与 release manifest 一致的 BuildInfo。
- Base：baseline variant 使用 simple processor；没有伪造 model/runtime 字段，质量门禁仍通过。
- Base：ML primary 初始化失败后 simple fallback 成功；正式产物允许发布，但 provenance 明确是 fallback，不能作为 ML 验收证据。
- Bad：两个 retry 同时触发；只能发生一次下载/初始化 attempt。
- Bad：baseline manifest 或 runtime 声明了 `ml-rembg`，构建立刻失败。
- Bad：构建目录残留旧 installer；release manifest 不能收集非本次 build_id 产生的文件。

## 6. 预计影响范围

- `crates/ats-core/src/image_proc/`
- `crates/ats-core/src/platform/artifact/` 与 asset handler
- `src-tauri/src/commands/image_proc_state.rs`
- health/capabilities 与 Tauri API 类型边界
- `build.ps1`、Tauri/Cargo 构建配置和必要的 PowerShell 定向测试
- `.trellis/spec/backend/quality-guidelines.md`
- `docs/01-总览/项目架构总览.md`、`docs/02-现状/当前进度说明.md`、当前方案

## 7. 调查与实施顺序

1. 调查当前 ImageProc trait、ArtifactManifest schema、prewarm 状态机、health/capabilities 和构建脚本。
2. 固化 BuildInfo、ImageProcOutcome 与 release manifest 的可执行 schema/错误矩阵。
3. 实现 Core 图片 provenance 和 ArtifactManifest 写入，添加 Simple/ML/fallback 定向测试。
4. 实现 prewarm attempt/last failure/singleflight retry 和 Desktop/API 定向测试。
5. 实现 BuildInfo 注入与 health/capabilities 暴露，验证 feature/variant 不一致会失败。
6. 实现 baseline/ML 隔离构建和 release manifest 的脚本级 dry-run/fixture 测试。
7. 同步 code-spec 和稳定文档；完成定向门禁后再提出完整 Tauri installer 构建申请。

## 8. 调查结论与可执行契约

### 8.1 当前缺口

- `ArtifactManifest.imageProcessing` 已存在占位结构，但 `publish_generated_artifact` 固定传 `None`，生产图片产物没有 processor provenance。
- `ImageProcClient` 当前只返回 `Vec<u8>`；`BgRemoverChain` 把 ML 失败降为一条日志，fallback 原因无法进入 ArtifactManifest。
- `MlBgRemover` 没有保存已验证的 u2netp SHA 和 ONNX Runtime 版本；prewarm 已经持有这两个真相源，但加载时丢失。
- `ImageProcState` 只有 primary/status 两把同步锁；没有 attempt、singleflight、last failure 或 retry command。
- `HealthReport` 和 `LocalCapabilities` 没有 BuildInfo，TypeScript 类型也没有运行时构建身份。
- 根 `build.ps1` 通过 `-MlRembg` 切 feature，但两种 variant 都从 `src-tauri/target/release/bundle` 枚举产物。

### 8.2 Core schema

```text
BuildVariant = development | baseline | ml

BuildInfo {
  commit: String,
  variant: BuildVariant,
  features: Vec<String>,
  build_id: String,
}

ImageProcessor = simple | ml_u2netp

ImageProcFallback {
  from: ImageProcessor,
  reason: not_ready | model | runtime | decode | encode | unsupported | quality | unclassified,
}

ImageProcessingProvenance {
  processor: ImageProcessor,
  model_sha256?: String,
  runtime_version?: String,
  fallback?: ImageProcFallback,
  build: BuildInfo,
}

ImageProcOutcome {
  png: Vec<u8>,
  provenance: ImageProcessingProvenance,
}
```

- `SimpleBgRemover` 返回 `processor=simple`，没有 model/runtime/fallback。
- `MlBgRemover` 构造时接收已校验的 model SHA 和 runtime version，成功返回 `processor=ml_u2netp`。
- `BgRemoverChain` 的 primary 失败只把稳定错误分类写入 fallback；不得把原始错误文本写进 manifest。
- `ImageProcOutcome` 在处理发生时从 `BuildInfo::current()` 记录运行时身份；`ArtifactManifest.imageProcessing` 直接复用同一 provenance 类型，不复制一套 processor schema。
- ImageProc/BuildInfo schema 变化将 ArtifactManifest schema version 从 1 提升到 2；项目未上线，不兼容读取旧 schema。

### 8.3 BuildInfo 注入

- `ats-core/build.rs` 是构建身份唯一注入点，读取受控 `ATS_BUILD_COMMIT`、`ATS_BUILD_VARIANT`、`ATS_BUILD_ID`，并结合实际 `CARGO_FEATURE_ML_REMBG` 生成编译期值。
- 未提供环境变量时生成 `development/unknown/dev`，明确不冒充候选。
- `baseline + ml-rembg` 或 `ml + feature off` 在 build script 阶段失败；features 由实际 Cargo cfg 计算，不信任脚本声明字符串。
- Core `health` 和 `capabilities` 引用同一个 `BuildInfo::current()`；Desktop 与 Web 不各自复制常量。

### 8.4 Prewarm singleflight

```text
retry call captures (attempt, was_loading)
  -> acquire one async flight mutex
  -> if another attempt advanced, or call observed Loading: return its terminal snapshot
  -> increment attempt exactly once
  -> download/verify/init/load
  -> atomically publish primary + Ready metadata, or structured Failed(lastFailure)
```

- startup prewarm 与 retry command 复用同一入口。
- `PrewarmStatus` 的 Loading/Ready/Failed 都携带 attempt；Ready 携带 model/modelSha/runtimeVersion，Failed 携带唯一 `ActionableFailure`。
- retry 返回最终 `PrewarmStatus`；feature off 产生稳定的不可重试失败，不并发下载。

### 8.5 Build isolation 与 release manifest

- `CARGO_TARGET_DIR=<repo>/artifacts/build/<variant>/<build-id>/target` 是该 variant 唯一 bundle 根。
- release 输出落到 `<repo>/artifacts/release/<build-id>/<variant>/`；只复制/哈希本次 target 下的允许 installer 扩展名。
- `build.ps1 -Variant Baseline|Ml` 负责生成一次 build identity 并注入环境；保留 `-MlRembg` 仅会形成双入口漂移，因此破坏性替换为 `-Variant`。
- 提供不调用 Tauri build 的 plan/dry-run 路径，用 fixture 验证目录隔离、feature 参数和 manifest 枚举边界；真实 installer 构建另行授权。

## 9. 不纳入

- Work Order 5 的完整 Windows release-candidate 总入口与 GitHub Actions 发布工作流。
- 自动运行 workspace 全量 test/clippy、完整 baseline/ML Tauri bundle 或安装版验收。
- 两 variant 并排安装、修改 Tauri identifier 或迁移用户应用数据。
- 真实游戏 UI 操作。

## 10. 自动验证证据（2026-08-01）

已通过：

```text
cargo test -p ats-core build_info::tests --no-default-features                    # 2 passed
cargo test -p ats-core image_proc:: --no-default-features                         # 26 passed
cargo test -p ats-core image_proc:: --features ml-rembg                           # 33 passed
cargo test -p ats-core platform::artifact::tests --no-default-features             # 4 passed
cargo test -p ats-core happy_path_writes_png_and_cs --no-default-features          # 1 passed
cargo test -p agentthespire-desktop commands::image_proc_state::tests --no-default-features # 3 passed
cargo test -p agentthespire-desktop commands::image_proc_state::tests --features ml-rembg    # 2 passed
cargo check -p ats-core --no-default-features
cargo check -p ats-core --features ml-rembg
cargo check -p agentthespire-desktop --no-default-features
cargo check -p agentthespire-desktop --features ml-rembg
npx tsc -b --pretty false
pwsh -NoProfile -File scripts/test-build-plan.ps1
git diff --check
```

`scripts/test-build-plan.ps1` 已覆盖 variant/feature 组合、拒绝调用方额外 feature、baseline/ML 目录隔离、忽略共享目录陈旧 installer、只收集允许的当前 bundle 文件以及复制后 SHA-256 重算。

完整 baseline/ML Tauri bundle 已在干净提交 `a2570e2606ef1c21a589d9c8ec3abea37ebdfbce` 上完成。`build.ps1` 会在 bundle 后用最终 GUI EXE 的 `--write-build-info` 文件握手核对 commit/variant/features/buildId，不一致时不发布 release 目录。

最终候选：

| Variant | Build ID | Installer | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| baseline | `wo4-a2570e26-baseline` | `msi/AgentTheSpire_0.1.0_x64_en-US.msi` | 8622080 | `3634ad392b18086c76867d6b65478bd3e81c6e180e5effb17a97529b362b67cb` |
| baseline | `wo4-a2570e26-baseline` | `nsis/AgentTheSpire_0.1.0_x64-setup.exe` | 6256029 | `05627782c35a3fd42b2a8ef144f8855d03855ebfef8c607a3802820360bd1fa4` |
| ml | `wo4-a2570e26-ml` | `msi/AgentTheSpire_0.1.0_x64_en-US.msi` | 18092032 | `d9d287c11c65ede33717fad2836eea941cf940981a4cae3e092cc1c456267e71` |
| ml | `wo4-a2570e26-ml` | `nsis/AgentTheSpire_0.1.0_x64-setup.exe` | 6364765 | `0912545b7f512b8da65d07ebe91030f7d80c8a741aa71b8541db7b2592e73531` |

四个文件的 manifest/release/bundle 哈希均已现场复算一致，两个 variant 的哈希互不相同，且身份临时文件已清理。候选位于 `artifacts/release/<build-id>/<variant>/`，均为 `NotSigned`。

## 11. 安装候选人工 E2E 证据（2026-08-01）

已使用两种 NSIS 候选完成隔离安装验收，随后正常关闭并卸载：

- baseline：安装器 SHA-256 为 `05627782c35a3fd42b2a8ef144f8855d03855ebfef8c607a3802820360bd1fa4`；运行时 BuildInfo 为 commit `a2570e2606ef1c21a589d9c8ec3abea37ebdfbce`、variant `baseline`、features `[]`、buildId `wo4-a2570e26-baseline`，与 release manifest 完全一致。应用可启动、加载隔离配置、通过 UI 创建工程，并通过窗口关闭流程正常退出。
- ML：安装器 SHA-256 为 `0912545b7f512b8da65d07ebe91030f7d80c8a741aa71b8541db7b2592e73531`；运行时 BuildInfo 为同一 commit、variant `ml`、features `["ml-rembg"]`、buildId `wo4-a2570e26-ml`，与 release manifest 完全一致。
- ML 从空隔离目录完成首次 prewarm；u2netp SHA-256 为 `309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8`，ONNX Runtime 版本为 `1.22.0`，下载的 `onnxruntime.dll` SHA-256 为 `579b636403983254346a5c1d80bd28f1519cd1e284cd204f8d4ff41f8d711559`。
- UI 完成 `Generate Plan` 与 `Generate Code`。Run `run-000000000000000018c7adebadbcc894-00000004` 状态为 `succeeded`；ArtifactManifest schema version 为 2，processor 为 `ml_u2netp`，model/runtime/BuildInfo 与上述候选一致，且不存在 fallback。
- ArtifactManifest SHA-256 为 `68172455af59f180c09543761e25fd8047d8e8e1b9e89ad12381e12779f143f5`，与 RunRecord 的 `manifestSha256` 一致；manifest 内所有快照文件和已发布文件均已逐项重算哈希一致。
- 图片质量门禁通过：透明像素比例 `0.537109375`、最大连通组件比例 `0.9852320675105485`、边缘前景比例 `0.21774193548387097`，`accepted=true`、`issues=[]`。
- 本次生成引用 STS2 Game Pack SHA-256 `81238b4f679c9bc994d596021903446aa0091edaa576c5c604f1e4ed2985dd39` 和 Truth Snapshot `5bd4e6ffd7e667bfac6613d6c6a7c3185e9fd9fafe7a07974c9a3faef03c4b6d`；`current.json`、snapshot manifest 与 ArtifactManifest 引用一致。
- ML 应用通过窗口关闭正常退出，本地 E2E stub 已停止，候选卸载器以退出码 0 完成；安装目录和卸载注册表项均已移除。隔离 E2E 证据保留在 `artifacts/e2e/wo4-installed-candidates/`，不纳入版本控制。

由此，第 6 项验收标准已完成。该 E2E 使用确定性本地 image/LLM stub 验证安装版的真实桌面调用链、ML 处理和 provenance，不用于评价外部模型的提示词语义一致性，也未操作真实游戏 UI。
