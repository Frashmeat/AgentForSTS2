# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting rules do you enforce?
- What are your testing requirements?
- What code review standards apply?
-->

(To be filled by the team)

---

## Forbidden Patterns

<!-- Patterns that should never be used and why -->

(To be filled by the team)

---

## Required Patterns

- 桌面工程的 `.ats/lock` 必须通过 `std::fs::File::try_lock()` 持有操作系统独占锁。
- 锁文件可以常驻并记录 PID；不得通过无条件删除锁文件来判断或接管陈旧锁。
- `TryLockError::WouldBlock` 映射为 `ProjectError::Locked`，其他 IO 错误保留为 `ProjectError::Io`。

---

## Testing Requirements

工程锁改动至少覆盖三类用例：

- Good：工程未锁定时可以打开。
- Base：锁持有者退出、锁文件仍存在时可以重新打开。
- Bad：另一 handle 或进程仍持锁时返回 `ProjectError::Locked`。

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)

## Scenario: Structured Asset Codegen

### 1. Scope / Trigger

This contract applies to `SubmitCodeGenerateRequest::Asset` and `submit_asset_generate`. It covers model output, runtime image delivery, image quality, localization, compile validation, rollback, and job finalization. It does not cover product configuration of `GodotPath`.

### 2. Signatures

```rust
JobApplicationService::submit_code_generate(...) -> JobResult<JobId>
JobApplicationService::submit_asset_generate(...) -> JobResult<JobId>
```

Compile gate:

```text
dotnet build --nologo -p:ModsPath=<project>/.ats/compile-gate/<job-id>/
```

On Windows, cwd may retain `\\?\`, but the `ModsPath` MSBuild property must use a normal drive or UNC absolute path. The template appends child paths, and mixed verbatim/forward-slash paths are invalid for the MSBuild `Copy` task.

### 3. Contracts

Supported `asset_type` values are `card`, `card_fullscreen`, `relic`, `power`, and `character`; parsing is case-insensitive and ignores `_` / `-`. `skip_build` only controls later publish orchestration; asset generation always runs the compile gate.

The model returns one strict JSON object, optionally in one JSON fence:

```json
{
  "csharp": "complete C# source",
  "localization": {
    "eng": {"MODID-ASSET_NAME.title": "..."},
    "zhs": {"MODID-ASSET_NAME.title": "..."}
  }
}
```

Rust owns all paths. The ModId is `project.json.csharp_name`; prompt and validator share `asset_localization_key_segment()`. It follows analyzer acronym boundaries: `E2EEnergySeedRelic -> E2_E_ENERGY_SEED_RELIC`, `XMLParser -> XML_PARSER`.

Committed writes are `Generated/<name>.cs`, both `<ModId>/localization/<locale>/<table>.json`, and generated runtime images under `<ModId>/images/`. Relics receive normal/outline/big paths; cards and powers receive normal/big paths. Diagnostic copies under `artifacts/` survive compile failures.

### 4. Validation & Error Matrix

| Condition | Job result | File behavior |
| --- | --- | --- |
| Empty or invalid JSON | retry once, then `Failed` | no project writes |
| Unsupported type or project scope mismatch | `Failed` | no out-of-scope write |
| Missing/mismatched locale keys, wrong prefix, empty value | retry once, then `Failed` | no project writes |
| Existing localization is not a flat string map | `Failed` | existing files preserved |
| Compile failure | `Failed` with `asset compile gate` | C#, localization, and runtime image writes rolled back |
| Cancellation during stream/compile | `Cancelled` | writes rolled back; success cannot overwrite cancellation |
| Compile success | `Completed` | transaction committed |

### 5. Good / Base / Bad Cases

- Good: strict relic bundle has matching `eng` / `zhs` `.title/.description/.flavor`; compile succeeds and C#, localization, and images remain.
- Base: first stream is empty and second is valid; job completes and usage accumulates.
- Bad: JSON is valid but C# does not compile; prior files are restored and new runtime files are removed.

### 6. Tests Required

```text
cargo test -p ats-core
cargo clippy -p ats-core --all-targets -- -D warnings
cargo check -p agentthespire-desktop
```

Assertions must cover type/acronym normalization, inline project context, strict bundle parsing, locale merge, empty-output retry, Windows MSBuild path normalization, compile rollback including images, cancellation finalization, and scaffold PCK include/exclude rules.

### 7. Wrong vs Correct

Wrong: ask for C# and two JSON files, accept one C# fence, and mark completed without compilation.

Correct: require `{csharp, localization.eng, localization.zhs}`, calculate paths in Rust, transactionally write C#/localization/images, run isolated `dotnet build`, then commit or roll back before job finalization.

## Scenario: Current Source Evidence and Semantic Regression Gate

### 1. Scope / Trigger

This contract applies to generated STS2 C# behavior code. It supplements compile validation; it does not claim that compilation proves runtime semantics or replace user-run real-game acceptance.

### 2. Contracts

- Stable asset templates contain engineering structure only. They must not persist timing-sensitive behavior recipes.
- `KnowledgeQuery.requirements` selects bounded excerpts from the current decompiled game source. Timing-sensitive evidence includes an official similar implementation and the lifecycle caller for the selected override.
- A successful structured asset generation preserves `artifacts/<asset>/evidence.md` with the actual knowledge manifest snapshot, source mode, requirement, symbol/path/line evidence, purpose, and injected facts.
- The reusable validator implements `forbidden_call_in_method`; STS2 supplies the known rule data that forbids `PlayerCmd.GainEnergy` inside `BeforeCombatStart`.
- Semantic rules execute before artifact/project writes and before the compile validator. Comments, string literals, unrelated methods, and method invocations must not create false positives.

### 3. Validation Matrix

| Condition | Expected behavior |
| --- | --- |
| Current official similar implementation + lifecycle caller found | Both bounded excerpts enter Code Facts and Evidence Record |
| `BeforeCombatStart` body calls `PlayerCmd.GainEnergy` | Retry/reject as invalid model output before writes and compile |
| `AfterSideTurnStart` with Owner-side and first-round guards | Semantic rule passes; compile gate remains required |
| Forbidden text appears only in comments/strings/another method | No semantic false positive |
| Current source or manifest unavailable | Prompt exposes the knowledge warning; do not claim verified evidence |

### 4. Targeted Tests

```text
cargo test -p ats-core knowledge::sts2_code_facts_provider::tests
cargo test -p ats-core codegen::prompt_assembler::tests
cargo test -p ats-core codegen::validation::tests
cargo test -p ats-core platform::application::handlers::asset_generate::tests
cargo check -p ats-core
```

## Scenario: Runtime Image Quality and STS2 Relic Role Derivation

### 1. Scope / Trigger

This contract applies when `submit_asset_generate` receives generated image bytes. Generic PNG analysis and role transforms live in `crates/ats-core/src/image_proc/`; STS2 target declarations and runtime paths live in `platform/application/handlers/asset_bundle.rs`.

### 2. Signatures and Result Fields

```rust
analyze_png_quality(bytes, ImageQualitySpec) -> Result<ImageQualityReport, ImageProcError>
derive_png_variants(bytes, &[ImageVariantSpec]) -> Result<Vec<DerivedImageVariant>, ImageProcError>
```

Successful image jobs expose `imageQualityPath`, `imageQuality`, and `runtimeImagePaths`. Diagnostics are `artifacts/<asset>/<asset>.png`, `<asset>.rembg.png`, and `image-quality.json`.

### 3. Contracts

- `ImageProcClient::remove_background` returning `Ok` is not sufficient for delivery. The processed PNG must pass the quality report.
- Default thresholds require at least 5% transparent pixels, 1% foreground pixels, and 60% of foreground pixels in the largest connected component. At most 30% of border pixels may contain foreground, and at most 15% may contain light neutral foreground residue.
- Decode/removal failure keeps the raw diagnostic and fails before code generation, project writes, and compile validation. It must not silently deliver the raw image.
- A rejected processed PNG keeps raw, rembg, and JSON diagnostics. The error names the failed quality rule.
- The current STS2 relic Resource Specification is: normal `128x128` cover resize; outline `128x128` alpha-derived hollow white ring with radius 4; big `1024x1024` cover resize.
- Relic normal/outline/big bytes must be pairwise different. Cards, powers, and characters retain their existing preserve behavior until their own role specifications are verified.
- Before transactional writes, normal and big must pass the standard quality rules at their declared dimensions; outline must contain both non-transparent outline pixels and transparent pixels.
- Runtime image writes remain in the existing file transaction and roll back on compile failure.
- `ml-rembg` remains an explicit desktop build feature. Its prewarm downloads ONNX Runtime 1.22.0 and u2netp through the shared cache before installing the ML primary in `BgRemoverChain`.
- Model and Runtime downloads stream to disk with connect, stall, and total timeouts. An interrupted file is resumed only when the server returns a matching `Content-Range`; a full `200` response truncates the partial file instead of appending it.
- The u2netp model, ONNX Runtime archive, and extracted DLL use exact SHA-256 baselines. A mismatched cache is never loaded or published.
- Native ONNX loader panics are converted into `MlBgRemoverError::OrtInit`. On Windows the prewarm failure tells the user to install or repair Microsoft Visual C++ 2015-2022 Redistributable (x64); it must not leave the state stuck at `Loading`.

### 4. Validation Matrix

| Condition | Expected behavior |
| --- | --- |
| White background with centered subject | heuristic removal passes and writes an explainable accepted report |
| Invalid PNG or remover failure | fail before LLM/compile; keep raw diagnostic only |
| Checker/neutral residue on image border | fail with `likely_background_residue`; keep raw/rembg/report |
| Fully opaque output | fail with `invalid_alpha` |
| Interrupted model/Runtime download | keep resumable bytes; continue only from a matching HTTP range |
| Wrong model/archive/DLL checksum | remove the invalid download or extracted target and fail before session creation |
| Missing/incompatible Windows VC++ Runtime | prewarm becomes `Failed` with an actionable repair message; heuristic fallback remains available |
| Valid relic master | derive 128 normal, independent 128 outline, and 1024 big |
| Compile failure after derivation | roll back formal runtime images; diagnostic inputs/report survive |

### 5. Targeted Tests

```text
cargo test -p ats-core image_proc::
cargo test -p ats-core --features ml-rembg init_missing_runtime_returns_error_without_panicking
cargo test -p ats-core --features ml-rembg loader_panic_returns_actionable_vc_runtime_error
cargo test -p ats-core platform::application::handlers::asset_generate::tests
cargo test -p ats-core platform::application::handlers::asset_bundle::tests
cargo check -p ats-core
cargo check -p agentthespire-desktop --features ml-rembg
```

## Scenario: Godot Toolchain and local.props Synchronization

### 1. Scope / Trigger

This contract applies when changing desktop toolchain settings, project creation/open, `local.props`, asset/code submission, or `build_project`. It prevents GUI/config/MSBuild drift and prevents E2E from touching real app-data or game Mods.

### 2. Signatures

```text
Tauri command: save_settings_patch(SettingsPatch) -> SettingsSnapshot
Tauri commands: create_project, open_project
Tauri commands: submit_code_generate_job, submit_asset_generate_job, submit_build_project_job
```

```rust
validate_godot_executable(path, timeout) -> Result<GodotInstallation, GodotValidationError>

sync_local_props(project_root, LocalBuildPaths {
    sts2_dll_path,
    godot_exe_path,
}) -> Result<LocalPropsSync, LocalPropsError>
```

Implementations live in `crates/ats-core/src/toolchain.rs`, `crates/ats-core/src/project/local_props.rs`, `src-tauri/src/commands/settings.rs`, `project.rs`, and `platform.rs`.

### 3. Contracts

```text
config JSON: toolchain.godot_exe_path: string
Tauri snapshot: toolchain.godotExePath: string
Tauri patch: toolchain.godot_exe_path?: string | null
MSBuild: <GodotPath>...</GodotPath>
```

- Omitted/null patch keeps the current value; `""` explicitly clears it.
- Non-empty values must be files whose `--version` first line is exactly `4.5.1` or starts with `4.5.1.`. Drain stdout/stderr concurrently while waiting so Godot cannot block on full pipes.
- Managed XML fields are `SteamLibraryPath` and `GodotPath`. Preserve unknown nodes, attributes, self-closing nodes, and custom `ModsPath`; writes are atomic.
- Asset/code/build requests must canonicalize to the active project before synchronization.
- E2E-only env keys are `SPIREFORGE_CONFIG_PATH`, `SPIREFORGE_APP_DATA_ROOT`, `ATS_E2E_GODOT_PATH`, `ATS_E2E_STS2_DLL_PATH`, and `ATS_E2E_BASELIB_RELEASE_URL`. E2E WDIO plugins/capabilities and endpoint overrides must remain behind the Cargo `e2e` feature and E2E Tauri config.

### 4. Validation & Error Matrix

| Condition | Expected behavior |
| --- | --- |
| Valid Godot 4.5.1 file | Save, hot-reload, and synchronize active project |
| Empty Godot path | Save empty value; build submission fails before job creation |
| Missing file | Reject save and keep config/memory unchanged |
| Godot 4.5.10 or another executable | Reject as unsupported/non-Godot |
| STS2 DLL outside `steamapps` | Return `MissingSteamapps`; do not write partial XML |
| Existing custom XML / `ModsPath` | Update only managed fields and preserve custom content |
| Requested project differs from active project | Reject before asset/build submission |

### 5. Good / Base / Bad Cases

- Good: GUI saves real 4.5.1, creates a project, completes asset compile gate, `dotnet publish`, Godot PCK export, and package in an isolated Mods directory.
- Base: GUI explicitly clears Godot; config and `local.props` contain an empty value, and build returns a user-actionable not-configured error before creating a job.
- Bad: a missing file, non-Godot executable, or `4.5.10` is rejected without changing persisted settings.

### 6. Tests Required

```text
npm run test:frontend
npx tsc --noEmit
cargo test -p ats-core --test local_props --test toolchain_config --test godot_toolchain
cargo check -p agentthespire-desktop
cargo check -p agentthespire-desktop --features e2e
npm run test:e2e:gui
```

Assertions must cover exact version boundaries, large-output pipe draining, explicit clear, self-closing managed XML without duplicates, custom XML preservation, isolated config/app-data/recent projects/Mods, page-switch recovery, DLL/PCK/zip output, and build rejection after clear.

### 7. Wrong vs Correct

Wrong: replace `local.props` as a string template, accept any version starting with `4.5.1`, register WDIO permissions in production, or hard-code a developer's tool path in the runner.

Correct: validate at the settings boundary, synchronize through the shared XML module before project work, inject machine paths through local env/config, and prove Good/Base/Bad through the real Tauri IPC and filesystem chain.

## Scenario: Game Pack Kernel and Project Identity Binding

### 1. Scope / Trigger

This contract applies when changing `crates/ats-core/src/game_pack/`, `ProjectMeta`, project create/open, or the Tauri/frontend project creation payload. Slice 1 establishes identity and truth-source declarations only; Truth Snapshot, resource specifications, templates, validation, build, and package cutovers remain separate slices.

### 2. Signatures

```rust
GamePackLoader::load_str(source_name, text) -> GamePackResult<LoadedGamePack>
GamePackLoader::load_from_dir(pack_root) -> GamePackResult<LoadedGamePack>
resolve_pack_relative_path(pack_root, relative) -> GamePackResult<PathBuf>
GamePackRegistry::built_in() -> GamePackResult<GamePackRegistry>
ProjectFolder::create(parent_dir, name, game_id) -> ProjectResult<ProjectFolder>
ProjectFolder::open(path) -> ProjectResult<ProjectFolder>
```

```text
Tauri command: create_project(parentDir, name, gameId) -> ProjectSnapshot
project.json: game_id: string
.ats/version: 2
```

### 3. Contracts

- Game Pack schema v1 requires `schema_version`, `id`, and `display_name`; `capabilities` and `truth_sources` default to empty collections.
- Loader capability/indexer/provider support comes from `GamePackLoadPolicy`. Unknown identifiers and source kinds are rejected; a Pack cannot declare an arbitrary command or shell runner.
- IDs use lowercase ASCII letters, digits, `_`, and `-`. Duplicate capability, truth-source ID, or Pack ID is rejected.
- `local_file` requires `input_key`. `github_release_asset` requires `repository`, `pinned_release`, and a path-free `asset` file name.
- Existing Pack-relative files must resolve through `resolve_pack_relative_path`; canonical paths outside the Pack root are rejected.
- The built-in STS2 Pack declares the current `v3.3.8` BaseLib release. Every `github_release_asset` requires an exact 64-character SHA-256; the verified STS2 `BaseLib.dll` digest is part of the Pack declaration.
- `ProjectMeta.game_id` is required and `PROJECT_SCHEMA_VERSION` is 2. Create, open, and save validate the ID through the loaded registry.
- Missing `game_id`, unknown Pack IDs, and old schema versions fail before acquiring the project lock. There is no implicit `sts2` default or legacy fallback.
- The current GUI explicitly sends the single installed Pack ID. Tauri accepts Rust `game_id` through the JavaScript `gameId` argument and persists snake-case `game_id` in `project.json`.

### 4. Validation & Error Matrix

| Condition | Expected behavior |
| --- | --- |
| Valid schema v1 Pack | Load and register; lookup by exact ID succeeds |
| Optional collections omitted | Load with empty capability/source collections |
| Unknown schema/capability/kind/indexer/provider | Reject with source name and field path |
| Missing kind-specific field | Reject with `truth_sources[n].<field>` |
| Duplicate source or Pack ID | Reject deterministically; do not replace the first entry |
| Pack-relative path escapes root | Return `GamePackError::PathOutsideRoot` |
| Create with known `game_id` | Write `project.json.game_id` and `.ats/version = 2` |
| Missing `game_id` in legacy project | Return `ProjectError::MissingGameId` with explicit recreate/migrate guidance |
| Unknown `game_id` | Return `ProjectError::UnknownGameId`; create must not leave a project directory |
| `.ats/version = 1` | Return `ProjectError::UnsupportedSchemaVersion` |

### 5. Good / Base / Bad Cases

- Good: the embedded STS2 Pack loads with two truth sources, and a new STS2 project survives create/drop/open with `game_id = "sts2"`.
- Base: a generic fixture with only schema, ID, and display name loads with empty optional collections and does not consult STS2 constants.
- Bad: a Pack with a shell capability, duplicate source ID, or `../` path is rejected; a legacy project never becomes STS2 by default.

### 6. Tests Required

```text
cargo test -p ats-core game_pack::
cargo test -p ats-core project::folder::tests
cargo check -p ats-core
cargo check -p agentthespire-desktop
npx tsc --noEmit
```

Assertions must cover built-in STS2 lookup and pinned BaseLib release, optional defaults, all unknown identifier categories, missing kind fields, duplicates, containment, project round-trip, no-directory-on-rejection, missing/unknown game ID, old schema rejection, and matching Tauri/TypeScript payload signatures.

### 7. Wrong vs Correct

Wrong: deserialize `game_id` with `#[serde(default)]`, treat an empty value as STS2, accept unknown declaration strings, or let the UI omit game identity because only one game currently ships.

Correct: validate declarations against an explicit Core capability catalog, persist a required registry-backed `game_id`, reject legacy ambiguity before locking, and add new schema dimensions only with their vertical cutover and evidence.

## Scenario: Content-Addressed Truth Snapshot Store

### 1. Scope / Trigger

This contract applies when changing `crates/ats-core/src/game_pack/truth_snapshot/`, Pack truth-source checksums, snapshot storage, refresh, or provider/Prompt consumers. The immutable verified input boundary is the only production truth-source path; the old `runtime/knowledge` layout is test-only where needed for committed equivalence fixtures.

### 2. Signatures and Storage

```rust
TruthSnapshotStore::new(runtime_dir, pack) -> TruthSnapshotStore
TruthSnapshotStore::begin(&self, pack) -> TruthSnapshotResult<TruthSnapshotDraft>
TruthSnapshotDraft::stage_source(&mut self, source_id, source_path) -> TruthSnapshotResult<PathBuf>
TruthSnapshotDraft::index_output_dir(&self, source_id) -> TruthSnapshotResult<PathBuf>
TruthSnapshotDraft::finalize(self, tool_versions) -> TruthSnapshotResult<VerifiedTruthSnapshot>
TruthSnapshotStore::open_current(&self, pack) -> TruthSnapshotResult<Option<VerifiedTruthSnapshot>>
TruthSnapshotStore::open_snapshot(&self, pack, snapshot_id) -> TruthSnapshotResult<VerifiedTruthSnapshot>
```

```text
runtime/game-packs/<game-id>/
  .staging/<draft-id>/
  snapshots/<snapshot-id>/
    snapshot.json
    sources/<source-id>/source.bin
    indexes/<source-id>/...
  current.json
```

### 3. Contracts

- `LoadedGamePack.content_sha256` is the SHA-256 of the exact manifest bytes accepted by `GamePackLoader`. A remote `github_release_asset.sha256` is required, validated as exactly 64 hexadecimal characters, and normalized to lowercase.
- A draft must stage every Pack-declared source exactly once. Source bytes are copied into the draft while streaming SHA-256 and size; a pinned remote checksum mismatch fails before index activation.
- Indexers consume only the staged source copy and write to the source-specific index directory. Every declared source requires a non-empty index containing at least one `.cs` file.
- Index tree identity sorts UTF-8 relative paths and binds each path, byte size, and file SHA-256. Symbolic links and non-regular index entries are rejected.
- Snapshot identity binds snapshot schema, Pack ID/schema/content SHA-256, sorted source identities, indexer/provider/tree summaries, and non-empty tool versions. `created_at` and draft names are excluded, so identical verified content reuses one snapshot ID and directory.
- Drafts and final snapshots are on the same volume. A verified draft is renamed into `snapshots/<snapshot-id>` before the atomic `current.json` pointer is replaced. A failed draft never changes the previous pointer.
- Opening a snapshot recomputes its identity and verifies Pack binding, manifest pointer checksum, every source hash/size, and every index tree. Snapshot IDs and manifest-relative paths cannot escape the store.
- `VerifiedTruthSnapshot` can only be constructed by store verification. A job holds one handle for its lifetime; activating a new current snapshot does not retarget that handle.
- Snapshot acquisition and provider execution are implemented by the Pack-driven refresh and verified context paths. Snapshot import/export and garbage collection remain unsupported and must not bypass store verification.

### 4. Validation & Error Matrix

| Condition | Expected behavior |
| --- | --- |
| All declared sources and indexes match | Rename to immutable snapshot, atomically activate pointer, return verified handle |
| Identical Pack/source/index/tool identity | Reuse the existing snapshot ID and directory |
| Missing/duplicate/unknown source | Reject the draft; leave current unchanged |
| Remote source checksum mismatch | Return `SourceChecksumMismatch`; do not activate |
| Missing index or zero `.cs` files | Return `MissingIndex` / `EmptyIndex`; do not activate |
| Empty tool map or blank name/version | Return `InvalidToolVersion` |
| Pointer traversal or Pack mismatch | Reject before reading an out-of-scope snapshot |
| Source or index bytes changed after activation | Reopen fails with checksum/integrity mismatch |
| Index contains a symlink | Return `InvalidIndexEntry`; do not activate |
| A newer snapshot becomes current | Existing job handle remains bound to its original root and ID |

### 5. Good / Base / Bad Cases

- Good: current game and pinned BaseLib copies produce complete indexes, activate one verified snapshot, and reopen with the same ID.
- Base: rebuilding identical content at a later time discards the duplicate draft and reuses the immutable snapshot while refreshing only the pointer.
- Bad: BaseLib bytes do not match the Pack checksum, one provider emits an empty index, or stored bytes are tampered; activation/reopen fails without silently falling back to legacy knowledge.

### 6. Tests Required

```text
cargo test -p ats-core game_pack::truth_snapshot::
cargo test -p ats-core game_pack::loader::tests
cargo test -p ats-core game_pack::registry::tests
cargo check -p ats-core
```

Assertions must cover staging/activation/reopen, content deduplication, pinned checksum mismatch, missing source/index, empty C# index, failed-draft pointer preservation, source/index tampering, fixed job handles, pointer traversal, store/Pack mismatch, and index symlink rejection.

### 7. Wrong vs Correct

Wrong: mark a directory current because it contains `.cs` files, let a caller supply `SourceMode::RuntimeDecompiled`, or update `current.json` before hashing all source and index bytes.

Correct: bind the exact Pack/source/index/tool identity, finish all work in same-volume staging, verify immutable contents, atomically activate a pointer, and pass the resulting verified handle through the complete job.

## Scenario: Legacy-to-Snapshot Fact Selection Equivalence

### 1. Scope / Trigger

This contract applies when changing `Sts2CodeFactsProvider`, Snapshot provider grouping, provider IDs in `game_packs/sts2/game-pack.json`, or the future `VerifiedGameContext` cutover. It proves semantic fact selection before production Prompt/Evidence APIs are switched.

### 2. Signatures

```rust
VerifiedTruthSnapshot::provider_index_roots(provider)
    -> Vec<(&TruthSnapshotIndex, PathBuf)>

Sts2CodeFactsProvider::build_facts_from_snapshot(query, snapshot, provider)
    -> Result<(Vec<KnowledgeFactItem>, Vec<String>), SnapshotCodeFactsError>
```

### 3. Contracts

- A provider query groups every verified index carrying the same provider ID before scanning, scoring, truncation, and evidence selection. It must not query each source independently and concatenate results.
- The STS2 game and BaseLib truth sources both select `sts2_code_facts`, preserving the legacy single `CodeFactsIndex` and shared `MAX_FACTS` budget.
- Snapshot facts use `snapshot://<snapshot-id>/<source-id>/<relative-path>` coordinates. Equivalence comparison may normalize only the legacy absolute root and Snapshot URI prefix; source-relative paths, excerpts, order, and all other fields remain semantic.
- A provider ID with no verified indexes returns `SnapshotCodeFactsError::MissingProvider`. It does not return empty facts, consult `KnowledgePaths`, or fall back to `SourceMode`.
- The Snapshot provider remains a side path until Slice 3. Passing this fixture does not authorize claiming that production Prompt/Evidence uses current Snapshot evidence.

### 4. Validation & Error Matrix

| Condition | Expected behavior |
| --- | --- |
| game and BaseLib share one provider | Aggregate both roots, query once, preserve global ranking and cap |
| Behavior query selects Lantern and lifecycle caller | Legacy and Snapshot normalized facts/excerpts/order are identical |
| BaseLib symbol query | Same BaseLib fact and source-relative evidence path |
| Normal relic query | Same facts, ordering, keywords, asset types, and warnings |
| Provider absent from verified manifest | Return `MissingProvider`; do not produce an empty success |

### 5. Good / Base / Bad Cases

- Good: identical game/BaseLib C# trees in legacy and Snapshot layouts produce byte-equivalent serialized facts after root-only normalization.
- Base: paths differ by legacy absolute root versus Snapshot ID URI, while the source ID and relative path remain equal.
- Bad: game and BaseLib are queried separately and concatenated, creating two ranking budgets or different evidence selection; the equivalence fixture must fail.

### 6. Tests Required

```text
cargo test -p ats-core knowledge::sts2_code_facts_provider::tests
cargo test -p ats-core game_pack::
cargo check -p ats-core
```

Assertions must cover behavior evidence, a BaseLib symbol, a normal asset query, exact warnings, normalized full fact payloads, deterministic provider grouping, and missing-provider rejection.

### 7. Wrong vs Correct

Wrong: call the provider once per source, take up to 12 facts from each, concatenate them, and treat different ranking as an acceptable migration detail.

Correct: select all Snapshot indexes declared for one provider, build one corpus, execute the existing selection algorithm once, and require root-normalized output equality before cutover.

## Scenario: Verified Game Context Generation Cutover

### 1. Scope / Trigger

This contract applies to production code, asset, and batch generation; Prompt previews; and Evidence Records. Production refresh and status use the active project's Pack and verified Truth Snapshot; legacy knowledge refresh/status is not available.

### 2. Signatures and Payload

```rust
VerifiedGameContext::open_current(runtime_dir, registry, game_pack_id)
    -> Result<VerifiedGameContext, GameContextError>

PromptAssembler::assemble_asset_prompt(request, context)
    -> Result<String, PromptAssemblyError>
PromptAssembler::assemble_asset_prompt_with_evidence(request, context)
    -> Result<AssetPromptAssembly, PromptAssemblyError>
PromptAssembler::assemble_custom_code_prompt(request, context)
    -> Result<String, PromptAssemblyError>
PromptAssembler::assemble_asset_group_prompt(request, context)
    -> Result<String, PromptAssemblyError>

JobApplicationService::submit_code_generate(request, context, artifacts_dir, sink)
    -> JobResult<JobId>
JobApplicationService::submit_asset_generate(request, context, artifacts_dir, image_gen, image_proc, sink)
    -> JobResult<JobId>
JobApplicationService::submit_batch_custom_code(request, context, artifacts_dir, sink)
    -> JobResult<JobId>
```

Generation Job payloads preserve the request fields and add `_gameContext` with camel-case `gamePackId`, `gamePackDisplayName`, `gamePackSchemaVersion`, `gamePackSha256`, `snapshotSchemaVersion`, `snapshotId`, `sources`, `indexes`, `toolVersions`, and `createdAt`.

### 3. Contracts

- `VerifiedGameContext` is created only from a loaded registry Pack, the project's explicit `game_id`, and `TruthSnapshotStore::open_current`.
- Production Prompt and Job APIs accept `VerifiedGameContext`; they do not accept `KnowledgePaths`, `SourceMode`, or caller-asserted freshness.
- A missing current Snapshot fails before Job creation, image generation, or LLM calls. There is no legacy cache or ilspy fallback.
- Resolver facts come from all verified indexes assigned to the Pack provider. Lookup coordinates use `snapshot://<snapshot-id>/<source-id>/` only.
- Evidence and Job payload `_gameContext` record Pack ID/display/schema/SHA, Snapshot ID/schema, sources, indexes, tool versions, and creation time.
- The context is moved into the spawned Job so a later current-pointer change cannot alter in-flight evidence.
- The legacy fact builder is test-only and exists solely for the committed equivalence fixture.

### 4. Validation and Error Matrix

| Condition | Expected behavior |
| --- | --- |
| Known project `game_id` and verified current Snapshot | Create context, persist matching `_gameContext`, and resolve Prompt facts from that Snapshot |
| No `current.json` for the project Pack | Return `GameContextError::MissingCurrent` before Job creation, image generation, or LLM calls |
| Unknown project `game_id` | Return `GamePackError::UnknownPackId`; do not select STS2 implicitly |
| Current pointer or Snapshot bytes fail integrity verification | Return `TruthSnapshotError`; do not consult legacy knowledge |
| Requested provider has no verified indexes | Return `SnapshotCodeFactsError::MissingProvider`; do not return empty success |
| Current pointer changes after Job submission | In-flight Job continues with its owned context and original Snapshot ID |
| Web Prompt request has no current Snapshot | Return HTTP 400 with the actionable missing-current message |

### 5. Good / Base / Bad Cases

- Good: an STS2 project with a verified current Snapshot produces Snapshot facts and an Evidence Record whose Pack/Snapshot/source/index/tool identity matches Job payload `_gameContext`.
- Base: facts contain no matching type for a valid query; guidance and an explicit no-matching-facts warning render from the verified Snapshot without falling back to mutable paths.
- Bad: a project has no current Snapshot; Desktop/Web reject the request before creating a Job or invoking image/LLM clients.

### 6. Targeted Tests

```text
cargo test -p ats-core codegen::prompt_assembler::tests
cargo test -p ats-core knowledge::sts2_knowledge_resolver::tests
cargo test -p ats-core knowledge::sts2_lookup_provider::tests
cargo test -p ats-core platform::application::handlers::batch_custom_code::tests
cargo test -p ats-core platform::application::handlers::asset_generate::tests
cargo test -p ats-core --test job_lifecycle code_generate_writes_files_via_public_api
cargo test -p ats-web routes::codegen::tests::missing_current_snapshot_is_rejected_as_bad_request
cargo check -p ats-core
cargo check -p agentthespire-desktop
cargo check -p ats-web
```

## Scenario: Pack-Driven Truth Snapshot Refresh

### 1. Scope / Trigger

This contract applies when changing Pack truth-source acquisition, indexer execution, current Snapshot activation, Desktop refresh/status, health readiness, or refresh Job history. It does not authorize writing the game installation or operating the game UI.

### 2. Signatures and Storage

```rust
TruthSnapshotRefresher::refresh(pack, store, local_inputs, force)
    -> Result<TruthSnapshotRefreshOutcome, TruthSnapshotRefreshError>

inspect_truth_snapshot(pack, store) -> TruthSnapshotStatus

JobApplicationService::submit_truth_snapshot_refresh(
    request, pack, store, local_inputs, refresher, sink,
) -> JobResult<JobId>
```

```text
Tauri write: submit_truth_snapshot_refresh_job({force}) -> <project>/.ats/history
Tauri read:  get_job(id), list_jobs(), cancel_job(id)
Project jobs: <project>/.ats/history
Historical legacy jobs: <runtime>/knowledge/jobs (read-only compatibility)
```

The Tauri submit command derives `game_id` from the active project and local source bindings from workstation configuration. The request cannot provide a Pack ID, source path, release, asset, checksum, indexer, or provider.

### 3. Contracts

- Local inputs are validated before Job creation. Every Pack-declared source is required.
- Refresh-only Job submission does not construct or require an LLM client or API key.
- GitHub sources use `/releases/tags/<pinned-release>` and require an exact asset name, matching response tag, and exact Pack SHA-256. Production never calls `releases/latest`.
- The GitHub API token is sent only to the API request. It is never forwarded to the response-provided browser download URL.
- GitHub asset downloads use 15-second connect, 45-second read-stall, and 10-minute per-request total timeouts with at most four attempts. A retry sends `Range: bytes=<stored>-`; bytes are appended only when `206 Content-Range` starts at that exact offset. A full `200` truncates the partial file, an invalid range is rejected, and retry exhaustion reports attempt and byte progress.
- Indexers are a closed Core capability set. Stage 1 supports `dotnet_project` and `dotnet_file`; unknown values fail deterministically.
- Every source is copied into the draft before indexing. Indexers consume only the staged copy.
- `ilspycmd --version` is part of Snapshot identity. The `toolVersions.ilspycmd` value is the normalized version without a repeated `ilspycmd:` label. A verified current Snapshot is a cache hit only when Pack, local source hashes, pinned remote identities, indexes, and tool versions still match.
- A verified pinned remote source may be reused when a local source or tool changes. `force = true` reacquires remote inputs and reindexes all sources; identical content still deduplicates by Snapshot ID.
- Fetch, checksum, index, or finalize failure never updates `current.json`. A Pack-declared BaseLib failure fails the refresh Job; it is not an optional warning.
- Status is `ready`, `missing`, or `invalid`. `ready` requires reopening and fully verifying the current Snapshot.
- Desktop health uses verified current readiness. Web has no global knowledge status route because it has no active-project identity; Web generation validates the request project's context directly.
- Legacy `KnowledgeRefresh` remains an enum value only for historical Job deserialization. Old refresh/status, manifest v1, import/export, and `releases/latest` clients have no production entry.

### 4. Validation and Error Matrix

| Condition | Expected behavior |
| --- | --- |
| All declared sources fetch, hash, and index | Atomically activate a verified current Snapshot and complete the Job with Pack/Snapshot/source/index/tool identity |
| Inputs and tool versions unchanged | Return `cacheHit = true` without fetch or index work |
| Local game assembly changes | Create a new Snapshot and reuse the still-verified pinned remote source |
| Asset body stalls or ends early | Retry from the stored byte count with a matching HTTP range |
| Asset returns a mismatched `Content-Range` | Reject the download; do not append bytes or activate current |
| Fixed remote bytes do not match Pack SHA | Fail and preserve the previous current pointer |
| Indexer fails or emits no C# | Fail and preserve the previous current pointer |
| Local input key is absent or not a file | Reject before Job creation |
| Current pointer or bytes are corrupt | Status is `invalid`; generation cannot open a context |
| Refresh is already locked | Fail with a deterministic busy error; do not race Snapshot directory activation |

### 5. Good / Base / Bad Cases

- Good: current game assembly and the Pack-pinned BaseLib produce one verified Snapshot whose manifest records both source and index identities.
- Base: a second non-forced refresh is a cache hit; a forced identical refresh reexecutes work but reuses the content-addressed Snapshot ID.
- Bad: remote checksum or index failure leaves the previous verified current usable and records a failed refresh Job.

### 6. Tests Required

```text
cargo test -p ats-core game_pack::truth_snapshot::refresh::tests
cargo test -p ats-core platform::application::handlers::truth_snapshot_refresh::tests
cargo check -p ats-core
cargo check -p agentthespire-desktop
cargo check -p ats-web
npx tsc -b --pretty false
```

Assertions must cover successful activation, cache hit, local source change, pinned release URL, exact remote SHA, interrupted-body resume, mismatched `Content-Range` rejection, normalized tool version, index failure, missing local input, `ready/missing/invalid`, token non-forwarding, and Job result identity/counts.

### 7. Wrong vs Correct

Wrong: fetch `releases/latest`, use one short whole-request timeout without resume, treat BaseLib as optional, index the caller's mutable source path, or update current after only the game source succeeds.

Correct: resolve all inputs from the active Pack, resume interrupted fixed assets only through validated HTTP ranges, stage and hash every source, run only declared Core indexers over staged copies, verify the complete Snapshot, then atomically activate current.

## Scenario: STS2 Mod Manifest Scaffold Contract

### 1. Scope / Trigger

This contract applies when changing `mod_template/ModTemplate.json` or
`project::template::scaffold_from_template`. It describes the STS2 `v0.107.1`
manifest format used by newly scaffolded projects. It does not describe the
unrelated `runtime/knowledge/knowledge-manifest.json` cache format.

### 2. Current Source Evidence

The current game assembly is
`<game>/data_sts2_windows_x86_64/sts2.dll`. Direct decompilation of
`MegaCrit.Sts2.Core.Modding.ModManifest` and `ModDependency` defines:

```json
{
  "min_game_version": "0.107.1",
  "dependencies": [
    {"id": "BaseLib", "min_version": "v3.3.8"}
  ]
}
```

BaseLib `v3.3.8` is the verified runtime and NuGet package baseline. The game
temporarily migrates old string-only dependencies in `ReadFromStream`, but
logs that the compatibility path will be removed.

### 3. Contracts

- `scaffold_from_template` must emit `<CSharpName>.json` with
  `min_game_version = "0.107.1"`.
- Each dependency entry must be an object with `id` and `min_version`.
- The STS2 template must declare BaseLib with `min_version = "v3.3.8"`.
- `dependencies: ["BaseLib"]` is not an accepted scaffold baseline.
- Build and package handlers copy or collect this file; they must not rewrite
  it into another schema.
- Changes to future game or BaseLib baselines require fresh current-source
  evidence and synchronized template assertions.

### 4. Validation Matrix

| Condition | Expected behavior |
| --- | --- |
| Good: current game and BaseLib fields | Scaffolded manifest contains the exact minimum versions and object dependency |
| Base: placeholder replacement | `id` and `name` use the derived C# name while schema fields remain unchanged |
| Bad: string-only BaseLib dependency | The scaffold regression test fails before the template becomes a release baseline |
| Bad: missing dependency minimum version | The scaffold regression test fails |
| Bad: knowledge manifest confused with Mod manifest | No changes are made to `knowledge::manifest` for this contract |

### 5. Targeted Tests

```text
cargo test -p ats-core project::template::tests
cargo check -p ats-core
```
