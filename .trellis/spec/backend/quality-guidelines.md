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

This contract applies to `SubmitCodeGenerateRequest::Asset` and `submit_asset_generate`. It covers model output, runtime image delivery, localization, compile validation, rollback, and job finalization. It does not cover product configuration of `GodotPath` or image alpha quality.

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
