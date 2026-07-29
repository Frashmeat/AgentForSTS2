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

## Scenario: Complete Knowledge Refresh and Desktop Job Routing

### 1. Scope / Trigger

This contract applies when changing STS2 source scanning, BaseLib GitHub errors, `knowledge_refresh`, desktop job retrieval, or knowledge GUI E2E. It covers complete fact indexing, optional BaseLib warning semantics, global knowledge job visibility, and E2E isolation. It does not authorize writing the game installation, real Mods, or the repository's formal `runtime/knowledge` during E2E.

### 2. Signatures and Storage

```rust
Sts2CodeFactsProvider::build_facts(
    query: &KnowledgeQuery,
    paths: &KnowledgePaths,
    game_mode: SourceMode,
) -> (Vec<KnowledgeFactItem>, Vec<String>)

BaselibSource::fetch_baselib_dll(dest_dir: &Path)
    -> Result<FetchedBaselib, BaselibError>
```

```text
Tauri write: submit_knowledge_refresh_job(request) -> <runtime>/knowledge/jobs
Tauri read:  get_job(id), list_jobs(), cancel_job(id)
Project jobs: <project>/.ats/history
Knowledge jobs: <runtime>/knowledge/jobs
```

The read commands route across the active project repository and the global knowledge repository. `list_jobs` merges both and sorts by `createdAt` descending. A knowledge refresh must remain observable even when an active project is open.

### 3. Contracts

- Collect `.cs` files first, sort paths deterministically, then scan. The production safety limit is 10,000 files, which exceeds the verified 3,425-file STS2 source while retaining an IO bound.
- If discovered files exceed the limit, return a warning containing discovered count, scanned limit, and explicit truncation language. Never report a truncated index as complete.
- A BaseLib failure is optional: preserve the successful game manifest, complete the job with `baselibStatus = "warning"`, and place the actionable message in `baselibError`.
- GitHub 401 says the token is invalid or expired and names `runtime.workstation.github_token`. Rate-limited 403/429 says to configure a valid token or wait. Other 403 responses say to check permissions.
- Do not return the raw upstream body for these statuses and never echo tokens, credentials, IP-specific rate-limit text, or unbounded response content.
- `ATS_E2E_BASELIB_RELEASE_URL` is read only when the desktop and core `e2e` Cargo features are enabled. Production builds always use the official GitHub Releases URL.
- GUI E2E uses temporary config, runtime, app-data, projects, and Mods. The real STS2 DLL and `ilspycmd` are read-only inputs.

### 4. Validation and Error Matrix

| Condition | Expected behavior |
| --- | --- |
| 3,425-file current game source | all files indexed; no truncation warning; later symbols resolve from `knowledge/game` |
| More than 10,000 `.cs` files | scan first 10,000 sorted paths and emit explicit truncation warning |
| Empty game directory | no facts and the existing no-types warning |
| BaseLib 401 | game refresh preserved; completed job with actionable invalid/expired-token warning |
| BaseLib rate-limited 403/429 | game refresh preserved; completed job with valid-token/wait guidance |
| Other BaseLib 403 | completed warning that directs the user to token permissions |
| Active project plus knowledge job | merged list and ID lookup expose both repositories |
| Missing job ID | `JobError::NotFound`; do not silently select another repository |

### 5. Good / Base / Bad Cases

- Good: a forced GUI refresh of the current DLL writes a matching manifest with 3,425 files; `NSettingsScreen` and the core command/model symbols resolve from game sources.
- Base: repeated scan order is stable; a game cache hit plus `include_baselib = false` completes without network access.
- Bad: deterministic local 401 and rate-limited 403 responses produce sanitized, actionable `baselibError` values visible in the GUI job detail.

### 6. Tests Required

```text
cargo test -p ats-core knowledge::
cargo check -p agentthespire-desktop
cargo check -p agentthespire-desktop --features e2e
npm run test:frontend
npx tsc --noEmit
npm run test:e2e:gui
```

Assertions must cover more than the legacy 2,000-file threshold, deterministic ordering, explicit small-limit truncation, empty directory behavior, sanitized 401/403/429 classification, merged job visibility, manifest/source/count consistency, a real later source such as `NSettingsScreen.cs`, and GUI job details. The E2E runner must delete its temporary root only after success and retain it on failure.

### 7. Wrong vs Correct

Wrong: stop during unsorted `WalkDir` enumeration, surface GitHub's raw body, or write a knowledge job to a repository that the GUI never queries.

Correct: sort then bound with an explicit warning, classify external errors into safe actions, route reads across project/global job stores, and prove the complete flow with isolated real-DLL GUI E2E.

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
