# Error Handling

> Executable error and redaction contract for the current Rust/Tauri/React implementation.

## Ownership And Signatures

Core owns the only serialized product failure schema in `crates/ats-core/src/failure.rs`:

```rust
FailureNormalizer::{llm,image,project,run,artifact,toolchain,local_props,image_proc,package}(...)
    -> ActionableFailure

#[serde(transparent)]
pub struct CommandFailure(pub Box<ActionableFailure>);
pub type CommandResult<T> = Result<T, CommandFailure>;
```

`ActionableFailure` uses camel-case JSON fields:

```text
schemaVersion
code
category
stage
message
action
retryable
retryAfterMs?
context?
diagnostic? { id, summary, ioKind? }
```

- `RunRecord.failure` stores the Core type directly.
- `src-tauri/src/commands/failure.rs::CommandFailure` is a boxed transparent IPC wrapper. Boxing keeps the Rust `Result` error variant bounded; Serde still emits the same `ActionableFailure` JSON object. Tauri must not copy the fields into another transport DTO.
- `src/services/actionableFailure.ts::toActionableFailure` is the React runtime guard. Invalid reject values become a fixed local `core.unclassified` failure.
- `ActionableErrorNotice` renders the safe message, recovery action, and optional diagnostic ID. Pages do not render `String(error)`.

## Failure Versus Cancellation

- A failed Run has one `ActionableFailure`, no result, and one `failed` terminal timeline event.
- A cancelled Run has neither failure nor result. The first `CancellationReason` wins and the Run receives one `cancelled` terminal event only after work has stopped and rollback has finished.
- Tauri command rejection is not a Run terminal transition. The handler/repository remains the authority for persisted Run state.
- A panic or handler return without a terminal state is converted by the task supervisor into a classified failed Run unless another terminal CAS already won.

## Redaction Boundary

`FailureContext` is a fixed whitelist, not a free-form map. It may contain bounded provider IDs, setting keys, dependency names, Run IDs, project-relative paths, attempt counts, and byte progress when the matching field exists.

The following values never enter IPC, Run history, ArtifactManifest, release verification, diagnostics summaries, or user-visible UI:

- API keys, GitHub tokens, authorization headers, cookies, or decrypted credentials.
- Provider response bodies, full prompts, generated output, or request/response payloads.
- URL query/fragment values.
- Unnormalized absolute project, app-data, model, runtime, or temporary paths.
- Unknown exception `Display`/`Debug` text.

Unknown errors use the fixed `core.unclassified` code/message/action and a generated diagnostic ID. Stable normalizers classify expected domain errors before that fallback.

## Validation And Error Matrix

| Source fact | Stable result | Safety requirement |
| --- | --- | --- |
| LLM/Image 401 or equivalent | `*.authentication_failed` | No provider body or credential |
| LLM/Image 429 | `*.rate_limited` plus bounded `retryAfterMs` | No raw headers/body |
| Transport failure | `*.network_failed` | No URL query or raw client error |
| Package path escape/missing file | stable `package.*` | Only safe project-relative path context |
| Artifact source/path/manifest is invalid | `artifact.path_invalid`, `artifact.snapshot_exists`, or `artifact.manifest_invalid` | No absolute path or raw manifest error |
| Artifact publish I/O fails after bounded retry | `artifact.publish_failed` plus `diagnostic.ioKind` | No raw path/OS message; Windows sharing conflicts remain retryable |
| Generated asset fails `dotnet build` | `artifact.compile_failed` plus Run diagnostic ID | Generated bundle and bounded redacted compile output stay only under `.ats/diagnostics/<run-id>/` |
| Asset compile process/output is unavailable | `artifact.compile_unavailable` plus `diagnostic.ioKind` when known | No command error text or absolute path crosses Run/IPC |
| Truth Snapshot runtime is locked or not writable | `truth_snapshot.storage_locked` plus `diagnostic.ioKind=permission_denied` | Persistent watcher conflicts are non-retryable until the runtime owner is released |
| Filesystem failure | domain code plus optional `diagnostic.ioKind` | No absolute user path |
| Project closing/drain timeout | stable `project.*` | At most one bounded blocking Run ID |
| Unknown backend error | `core.unclassified` | Fixed text; no unknown `Display` |
| Invalid Tauri reject payload | client-local `core.unclassified` | Rejected value is not rendered |
| User/project/shutdown cancellation | Run `cancelled` | No fake failure payload |

## Good / Base / Bad

- Good: a typed authentication error reaches React with the same schema and recovery action while a provider-body canary is absent from every serialized boundary.
- Good: a Windows artifact directory rename exhausts its bounded sharing-conflict retry and reaches the Run as retryable `artifact.publish_failed` with `ioKind=permission_denied`, without the absolute path or OS text.
- Base: an IO error preserves only a stable `ioKind` and safe relative path; the diagnostic ID can be used to correlate local logs.
- Base: a user cancels during a child process; cleanup finishes and the Run becomes `cancelled` with no failure.
- Bad: `anyhow`, SDK, reqwest, IO, or serde error text is returned through `CommandFailure` or persisted directly.
- Bad: an artifact handler discards `ArtifactError` and substitutes `core.unclassified`.
- Bad: a compile failure discards stdout/stderr and generated source, or copies either into RunRecord/IPC.
- Bad: React converts a rejected object with `String(error)` and displays its token/path canary.

## Required Tests

```text
cargo test -p ats-core failure::
cargo test -p ats-core platform::application::handlers::
cargo test -p agentthespire-desktop commands::failure::tests
npm run test:frontend
npx tsc -b --pretty false
```

Assertions must include token, provider-body, URL-query, absolute-path, malformed-payload, unknown-error, typed-error, artifact I/O/path normalization, and cancellation canaries.
