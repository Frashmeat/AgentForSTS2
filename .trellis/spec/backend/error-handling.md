# Error Handling

> Active Stage 2 failure, cancellation, Shell mapping and redaction contract.
>
> Graph/Draft/Run recovery invariants belong to [`stage2-contracts.md`](./stage2-contracts.md);
> test breadth belongs to [`quality-guidelines.md`](./quality-guidelines.md).
>
> Last updated: 2026-08-19

## 1. Serialized Product Failure

`ats-kernel::ActionableFailure` is the only backend failure serialized to a Shell:

```text
schemaVersion, code, category, stage, message, action, retryable
```

`ats-runtime::RunFailure` is the persisted execution envelope:

```text
code, stage, optional versioned safe details
```

Neither shape accepts arbitrary error strings. Tauri `CommandFailure` is a transparent newtype.
Known `truth.*`, `pack.*`, `resource.*`, `model.*`, `behavior.*`, `game.*`,
`validation.*`, `composition.*`, `artifact.*` and `run.*` failures keep their originating
family and stage; they must not collapse to `feature.execution_failed` or `core.unclassified`.

## 2. Model Transport And Output

The HTTP Model Adapter owns one bounded retry policy for timeout, 429 and retryable 5xx failures:
at most three attempts, cancellation-aware waits, numeric `Retry-After` clamped to 1..=120 seconds,
and configured bounded fallback delays. Features must not layer another transport retry loop.

OpenAI-compatible response format is pinned before Run creation by
`llm.openai_response_format = json_schema | json_object`. A failure never changes model, endpoint,
response format or Provider during the Run. Feature-owned strict decode remains authoritative.

`llm.max_output_tokens` is an optional user-declared Provider upper bound:

```text
effective = min(recipe.max_output_tokens, configured_provider_cap?)
```

The effective value enters ModelRequestSnapshot/hash, Runtime request and HTTP body. Invalid bounds
are `model.configuration` before queue/HTTP work. Rejection does not trigger automatic probing,
budget reduction, model switching, file splitting or weaker parsing.

- HTTP 401 -> `model.authentication`.
- HTTP 403 -> `model.request_rejected`, unless a future typed Provider contract supplies explicit
  trusted invalid-key evidence.
- truncated output -> `model.output_truncated`.
- invalid JSON/schema/domain shape -> `model.output_invalid`.

Chat Completions are consumed as SSE. Header and inter-chunk idle budgets are independent; malformed
events, invalid UTF-8/delta or `[DONE]` without output are invalid response; stream failure, idle
timeout or EOF before `[DONE]` are retryable transport failures. Cancellation drops the response.
The Adapter concatenates `choices[0].delta.content`; `reasoning_content` is used only when the
entire content channel is empty. It never extracts JSON fragments or branches on model identity.

Every HTTP-backed task enters the single FIFO `ModelRequestQueue` owned by the desktop composition
root. The task holds its slot through attempts, waits, parsing and terminal return. Queue cancellation
sends no HTTP request. Private per-Run queues and releasing the slot between retries are forbidden.

## 3. Standalone Single Boundary

`mod.generate.single` remains an independent Feature used by standalone Single, Batch and Complex.
It retains its role-keyed output errors and one logical semantic request contract. Its failure details
must contain closed reason codes and trusted role/shape metadata only.

Composition does not consume Single contribution, `composition_staged` checkpoint or native-source
repair output. A Single failure must never be used as a fallback for Composition Behavior/Render.

## 4. Composition Behavior Feedback

### 4.1 Scope / Trigger

This contract applies after a resolved Composition closure has passed exact Pack v5, Truth v2,
Capability Catalog and registered Adapter readiness.

### 4.2 Contracts

A Behavior node may request semantic feedback only for failures attributable to its typed proposal:

```text
JSON/schema/shape invalid
unknown or disallowed capability
argument type/bound/required-field invalid
typed reference invalid for the current Item
```

Before another model call, ExecutionGraph v6 CAS-persists a bounded, versioned, hashed safe feedback
envelope and candidate/Behavior hash. It never persists raw completion, parser text, Prompt,
Provider body, model-authored unknown path or role.

Each planned Behavior node owns one baseline request. `semanticRequestCount` records actual calls
and never blocks another node's baseline. `semanticFeedbackCount` alone consumes the Graph shared
feedback allowance. Each node also has bounded output/IR feedback rounds.

Feedback stops without another call on:

- same diagnostic fingerprint plus same candidate/Behavior hash;
- replacement with unchanged `behaviorSha256`;
- per-node or shared feedback limit exhaustion;
- unsupported Adapter capability;
- cancellation, storage conflict or pinned context drift.

### 4.3 Local Failures Never Sent To AI

The following are local defects or environment failures:

```text
registered Adapter cannot render a declared valid capability
deterministic native output does not compile
Pipeline validate/build/package fails
tool path, lock, filesystem or storage fails
publication/transaction/Artifact fails
checkpoint, Pack, Truth, Catalog or Adapter identity drifts
```

Use `game.adapter_unsupported`, `game.adapter_invalid`, `game.pipeline.*`,
`project.local_environment_invalid`, `composition.execution.*`, `artifact.*` or the exact
owning typed family. These failures do not increment semantic feedback and do not create a
compiler-to-model repair campaign.

### 4.4 Adjustment

`submit_composition_item_feedback` binds:

```text
source succeeded executionGraphId + expectedRevision
itemId + expectedDefinitionHash + expectedBehaviorSha256
bounded instruction (transient IPC/worker input only; never persisted)
```

`composition.adjustment.stale`, `.invalid` and `.requires_replan` fail before model work.
Feedback details may include safe Item identity/hash fingerprints but not instruction text; a paused
human target cannot be resumed without submitting the feedback again from its succeeded source.
A valid adjustment replaces only target Behavior/Render checkpoints and revalidates the closure.

## 5. Validation & Error Matrix

| Failure | Stable owner/result | AI feedback |
| --- | --- | --- |
| timeout/429/retryable 5xx | Model Adapter bounded retry | no semantic revision |
| Behavior JSON/schema/shape | `model.output_invalid` at Behavior stage | current Item only |
| capability/argument/reference invalid | `behavior.*` typed issue | current Item only |
| Adapter cannot render declared capability | `game.adapter_unsupported` | no |
| deterministic output cannot compile | `game.adapter_invalid` | no |
| Pipeline identity/DAG/input drift | `game.pipeline.*` before external work | no |
| tool/config/lock/storage | owning typed local family | no |
| graph claim/revision/checkpoint drift | `composition.execution.*` | no |
| publication/Artifact failure | `composition.publication.*` / `artifact.*` | no |
| User cancel | terminal cancelled after cleanup | no |
| Pause/close/switch/shutdown | paused, claim released after cleanup | no |

## 6. Other Domain Failures

Resource Prepare keeps `resource.media_invalid`, `resource.unsupported`,
`resource.source_invalid`, `resource.storage_failed`, `resource.pack_asset_missing` and
`resource.pack_asset_invalid`. Pack default bytes must match the exact Pack asset hash.

Composition Draft keeps distinct invalid selection, stale current, storage, not-ready, cycle and
node-limit failures. `CompositionConfirmationError::NotReady` and graph readiness map to
`composition.confirm.not_ready` at `composition.draft.confirm`, category `validation`, action
`replace_resource`, `retryable=false`.

Composition Plan typed details use `feature.composition-plan-failure-details` v1 with bounded closed
reason/count/Item/type/slot fields. Parser text, completion, URL and path are forbidden.

Game Pipeline resolution maps unknown/duplicate Provider, unsupported profile, Pack/Truth mismatch,
invalid DAG/digest/barrier and unavailable Primitive to `game.pipeline.*` before model, process or
project mutation. There is no fixed STS2 fallback graph.

Invalid `Sts2AssemblyPath`/`GodotPath` or managed `local.props` maps to
`project.local_environment_invalid`, category `configuration`, action `retry`,
`retryable=false`. Existing-project fallback preserves a valid project-local pair; invalid global
configuration must not overwrite it. Absolute paths and XML/parser/IO text are excluded.

## 7. Run, Cancellation And Recovery

- `succeeded`: one versioned result, no failure.
- `failed`: one RunFailure, no result.
- `cancelled`: cancellation reason, no result/failure.
- Pending/Running: no result/failure.
- A Feature returns only after rollback/cleanup; ProjectSession owns the terminal transition.
- Supervisor claim release is conditional on `activeRunId` still matching the terminal Run.
- Panic/non-terminal return becomes `run.task_panic` or `run.incomplete`.

The first cancellation reason wins. User cancellation becomes terminal cancelled. Pause, project
close/switch and shutdown become paused with claim released after cleanup. Close timeout retains task
handles and the OS lock; no force-success or force-unlock path exists.

After `commit_prepared`, failure preserves the roll-forward intent and successful checkpoints.
Before it, failure must leave no final publication. Cleanup attempts are independent: Artifact
cleanup failure must not skip available project rollback.

## 8. Redaction

Never serialize or persist:

- API keys, Authorization, cookies, Provider bodies, complete Prompts or outputs;
- absolute private paths, URL query/fragment, SDK/reqwest/IO `Display` text;
- unbounded compiler output or arbitrary rejected JSON.

Use stable code/stage, safe relative identifiers, bounded classified details and local-only
diagnostics. Adapter logs may retain sanitized bounded tails locally, but not in Graph, Run, IPC,
feedback or verification evidence.

## 9. Good / Base / Bad Cases

### Good

A Card Behavior uses a declared draw capability with valid arguments. Local validation passes,
Adapter renders deterministic files, compiler succeeds, and no feedback is created.

### Base

The model returns an unknown capability. The current Behavior node persists a safe typed issue and
performs one bounded semantic revision; other Item baselines remain available.

### Bad

The Adapter renders valid IR into uncompilable C# and the compiler text is sent to the model to
rewrite a source bundle. This hides a local Adapter defect, leaks native authorship back to AI and
is forbidden.

## 10. Tests Required

```powershell
cargo test -p ats-kernel product
cargo test -p ats-runtime execution_graph --lib
cargo test -p ats-adapters model_client -- --nocapture
cargo test -p ats-features composition_generate --lib
cargo test -p agentthespire-desktop --lib commands::failure
npm run test:frontend
npx tsc -b --pretty false
```

Assertions must cover:

- malformed/unknown Shell reject fallback and terminal invariants;
- cancellation cleanup, claim release and storage conflict;
- FIFO transport retry and 401/403 classification;
- Behavior feedback versus Adapter/compiler zero-feedback routing;
- baseline allowance not starved by shared feedback;
- adjustment stale/no-change/target-only behavior;
- absence of Prompt, raw completion, token, Provider body and absolute path.

## 11. Wrong vs Correct

Wrong:

```text
compiler error -> persist compiler tail -> ask model to rewrite source -> raise Graph total limit
```

Correct:

```text
Behavior output issue -> current Item typed feedback
Adapter/compiler issue -> local typed failure
transport issue -> same logical request bounded retry
```

## 12. Forbidden Patterns

- `Result<T, String>` or `anyhow::Error` at a serialized boundary.
- Persisting `error.to_string()` in Run/Graph/Artifact.
- Replacing a known Adapter error with `core.unclassified`.
- Compiler-to-model or native-source Composition repair.
- Marking cancellation before cleanup or dropping a JoinHandle as completion.
- React rendering an unvalidated reject.
