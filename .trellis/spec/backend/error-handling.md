# Error Handling

> Active Stage 2 failure, cancellation, shell mapping, and redaction contract.
>
> This file owns failure classification and safe serialization only. Persisted graph/Draft/Run recovery invariants belong to [`stage2-contracts.md`](./stage2-contracts.md); test breadth and forbidden implementation patterns belong to [`quality-guidelines.md`](./quality-guidelines.md). Task-specific Provider bodies, paths and candidate timelines stay in the task evidence ledger.

## Serialized Product Failure

`ats-kernel::ActionableFailure` is the only backend failure object serialized to a Shell:

```text
schemaVersion, code, category, stage, message, action, retryable
```

The constructor validates schema version, stable identifiers, bounded message text, and recovery action. Tauri's `CommandFailure` is a transparent newtype; it must not duplicate or extend the Kernel fields.

`ats-runtime::RunFailure` is the persisted execution failure envelope:

```text
code, stage, optional versioned safe details
```

It is not an arbitrary error string. The composition boundary maps internal Feature/Adapter errors to stable `RunFailure`; Shell commands map product rejection to `ActionableFailure`.

Single generation must retain the originating class across direct and Batch execution. Current
stable families include `truth.evidence_missing`, `pack.contribution_invalid`, `resource.*`,
`model.*`, `validation.*`, `artifact.*`, and `run.*`; Batch child Runs reuse the Single
classification. Do not collapse these known errors to `feature.execution_failed`.

Planning follows the same rule. Invalid or truncated typed model output, provider failures,
Pack/Recipe rejection, cancellation, and invalid input persist distinct `model.*`, `pack.*`,
`feature.*`, or `run.*` failures from `ModPlanError`; the Shell must not replace them with a
generic execution failure.

The HTTP Model Adapter owns one bounded retry policy for retryable provider failures: at most three
attempts, cancellation-aware waits, numeric `Retry-After` clamped to one through 120 seconds, and
120/300-second defaults when the provider gives no delay. The fallback values are configurable as
`llm.retry_initial_delay_ms` and `llm.retry_followup_delay_ms`, must be within one millisecond
through one hour, and old configuration files receive the defaults through `serde(default)`.
Features must not layer another retry loop or turn an exhausted transport/rate-limit result into
success.

OpenAI-compatible structured-output compatibility is selected before a Run through the typed
`llm.openai_response_format` value (`json_schema` by default, or explicit `json_object`). A
provider or output failure never changes that value for the in-flight Run and never triggers a
second weaker-format request. Both modes retain the same Feature-owned typed decode; invalid JSON
or shape remains `model.output_invalid` without raw Provider content in persisted details.

Direct `mod.generate.single` returns that failure after one logical model call. Inside
`composition.generate` request v5, repairable output-contract failures may enter the Feature-owned
semantic feedback controller without changing model, endpoint, response format or Provider retry
rules. Before the next model call, ExecutionGraph v4 CAS-persists a hashed
`feature.generation-feedback` v1 envelope plus the complete candidate SHA-256. The envelope contains
only closed diagnostic codes, a Pack-trusted optional role ID, expected/observed shape enums and no
raw completion, parser text or model-authored unknown role.

`mod.generate.single` model-output failures persist optional
`feature.mod-generate-single-failure-details` v1. Its payload contains exactly one closed
`reasonCode`: `output_truncated`, `json_decode`, `file_count`, `acceptance_notes`, `file_role`,
`file_content`, `merge_content`, `generated_file_count_overflow`, `checkpoint_provenance` or
`checkpoint_result`. The code/stage remains `model.output_invalid` or `model.output_truncated` at
`mod.generate.single.model`. Details never include completion fragments, parser messages, paths,
Provider metadata or unvalidated model-authored role names.

`merge_content` means a Pack `compositionMerge=json_object` role was not the directly typed flat
object required by the run-scoped output contract, contained a non-string value, or could not be
normalized within the bounded content limit. A JSON-encoded object inside a string is invalid; the
Feature never repairs or heuristically extracts it.

Output feedback stops without another model request when the graph-wide policy is exhausted or the
same typed diagnostic fingerprint and same complete candidate SHA-256 recur. The same diagnostic
with different candidate bytes is not by itself no progress. Generated-content repair campaigns
retain same-fingerprint plus same-checkpoint and unchanged-replacement stops per target. The
semantic request count is graph-total across output feedback, all campaign targets and one operator
adjustment; it is never multiplied by Item count. Provider/configuration,
storage, Truth, checkpoint, publication and cancellation failures never enter semantic feedback.
Cancellation observed during validation feedback must use the graph's common cancellation
transition. `CancellationReason::User` makes the graph terminal `cancelled`; Pause, project
close/switch and shutdown produce `paused` with the claim released. `pause_interrupted` therefore
accepts `running`, `validating`, `repairing` and `pause_requested`; a feedback branch must not turn a
User cancellation into `pause_after_graph_failure("run.cancelled")`.

Every HTTP-backed model task also enters the one FIFO `ModelRequestQueue` owned by the desktop
composition root. A task holds its slot through all attempts, retry waits, parsing and terminal
return; the next task cannot start an HTTP attempt first. Cancellation while waiting for the queue
returns `ModelError::Cancelled` without acquiring a slot. Constructing one private queue per Run or
releasing the slot between retries is forbidden because either pattern recreates provider bursts.

Resource Prepare maps decoded media/shape/alpha rejection to `resource.media_invalid`, unsupported
Pack roles to `resource.unsupported`, wrong source entry to `resource.source_invalid`, repository
failure to `resource.storage_failed`, and graph/Primitive/version drift to
`pack.contribution_invalid`. An unavailable built-in asset for the exact Pack identity is
`resource.pack_asset_missing`; bytes that differ from Pack `defaultAsset.sha256` are
`resource.pack_asset_invalid`. These known failures must never collapse to
`feature.execution_failed`.

Composition Draft and graph failures use stable `composition.*` families. Invalid Draft/selection,
stale current conflicts, storage failure, missing exact Item, hash/type/version mismatch, not-ready
nodes, pinned cycles and node-limit failure remain distinct. Repository/OS text and absolute paths
must not enter these failures. A confirmation storage failure returns only after all current pointer
replacements have rolled back or a recovery journal remains for the next repository access.
At the Tauri confirmation boundary, both `CompositionConfirmationError::NotReady` and
`CompositionGraphError::Readiness` map to `composition.confirm.not_ready` at
`composition.draft.confirm`, category `validation`, action `replace_resource`, and
`retryable=false`. They must not fall through to `composition.draft.invalid`; the caller needs the
resource-repair action to make the same Draft ready before another confirmation attempt.
Composition Plan output with duplicate slot targets or nodes outside the root pinned closure is
`model.output_invalid`; a Pack-declared binding count or total-quantity mismatch is
`composition.profile.count_mismatch`. Both fail before Draft or Item persistence and are never
reclassified as `core.unclassified`.

These two Composition Plan families persist `feature.composition-plan-failure-details` v1 when a
typed model response reaches validation. Details contain only stable reason codes, bounded counts
and already-validated Item/type/slot IDs. JSON parser text, complete model output, Provider bodies,
URLs and paths remain forbidden. A failure without valid versioned details is still represented by
its stable code/stage; React must reject malformed optional details and fall back to code/stage.

The exact camelCase payload is `reasonCode` plus optional `expectedCount`, `actualCount`, `itemId`,
`itemType` and `slotId`. Rust authors `reasonCode` only through the closed enum in
`crates/ats-features/src/composition_plan.rs`; current wire values are `json_decode`,
`root_profile_missing`, `draft_graph_invalid`, `node_count_overflow`, `node_total`,
`item_type_unsupported`, `item_id_duplicate`, `root_missing_or_wrong_type`, `item_type_count`,
`reference_target_duplicate`, `reference_count_overflow`, `reference_quantity_invalid`,
`reference_binding_count`, `reference_total_quantity`, `pinned_target_missing`,
`root_pinned_closure`, `reference_target_missing`, `identity_target_type`,
`resolved_node_missing`, `item_definition_invalid` and `item_definition_hash_invalid`.

Whole-closure generation preserves the originating Plan/Single/Build/Package failure family.
Staging uses `composition.staging.*`; final project writes use `composition.publication.*`; the one
final Artifact uses `artifact.*`. Cleanup attempts are independent: an Artifact cleanup error must
not skip a still-available project rollback, and a package commit error must not drop a pending
real-project transaction.

Recoverable whole-closure generation additionally uses `composition.execution.conflict` for a lost
graph revision/claim, `composition.execution.storage_failed` for graph repository failure and
`composition.execution.invalid` for a checkpoint/blueprint/publication-intent mismatch. A known
Plan or Single `model.*` failure keeps its original code and stage while the graph pauses the exact
node; it must not be reclassified as an execution failure. Child Run create-or-match failure is
`run.storage_failed`. Resume is explicit and never converts transport, decode or typed-output
failure into an automatic semantic retry. Before `commit_prepared`, a registered validator may
produce bounded `generated_content` issues for the Feature-owned complete-role repair loop; local,
unsafe, ambiguous, unchanged, repeated or policy-exhausted outcomes pause through graph-level safe
failure without another model request. Failure after `commit_prepared` releases only the active
claim and preserves the roll-forward publication intent plus every successful model checkpoint.

Invalid configured `Sts2AssemblyPath`/`GodotPath` or managed `local.props` XML maps to the one Shell
code `project.local_environment_invalid`, category `configuration`, action `retry`, and
`retryable=false`. Create requires both configured paths as valid plain files. Existing-project
Open/Generate/Build/Package use `sync_or_validate_project_local_props`: both configured paths valid
means atomically update the two managed fields; otherwise preserve the file byte-for-byte and
structurally read/validate both project-local fields. Failure uses stage `project.local_props` for
Open and `feature.local_props` before model/tool work. It must not include absolute paths or
XML/parser/IO text. Create rolls back only its newly created exact project root; Open and execution
preserve every existing project byte.

| Boundary | Good | Bad |
| --- | --- | --- |
| Create | both configured paths are valid plain files -> managed fields synchronize before session exposure | missing/invalid configured path or XML -> `project.local_environment_invalid`, no half-created root |
| Existing Open/Generate/Build/Package | valid configured pair updates managed fields; otherwise valid existing project-local pair is preserved and used | invalid configured pair overwrites a valid project file, or both sources are invalid but work continues |
| Local failure | one stable typed failure before model/tool invocation | failure enters repair input, leaks a path/parser message or collapses to `core.unclassified` |

Required canaries: `commands::failure::tests::invalid_project_local_environment_has_one_stable_shell_contract`, Workspace configured-Create rollback plus sync/fallback/no-mutation tests, and the GUI Build case that clears global Godot while retaining a valid project-local input.

## Ownership

- Kernel owns serialized shape and validation.
- Runtime owns Run terminal invariants and `RunFailure`.
- Features classify domain stages without provider/OS text.
- Adapters return typed local errors and never decide UI messages.
- Composition/Shell maps known failures to fixed codes/actions.
- React validates the exact object at runtime and replaces malformed rejects with a fixed local fallback; it never displays `String(error)`.

## Run Terminal Contract

- `succeeded`: one versioned result, no failure.
- `failed`: one `RunFailure`, no result.
- `cancelled`: cancellation reason, no failure and no result.
- Pending/Running records have neither result nor failure.
- A Feature returns only after rollback/cleanup; `ProjectSession` supervisor performs the sole persisted terminal transition.
- Panic or non-terminal worker return becomes fixed `run.task_panic` or `run.incomplete` unless another terminal CAS already won.

## Cancellation

The first cancellation reason wins. User cancel, project close/switch, and shutdown publish through `CancellationToken`; network/process/file work stops and cleanup completes before terminal persistence. Close timeout retains task handles and the OS lock. There is no force-success or force-unlock path.

## Redaction

Never serialize or persist:

- API keys, Authorization, cookies, provider bodies, complete prompts or outputs;
- absolute private paths, URL query/fragment, SDK/reqwest/IO `Display` text;
- unbounded compiler output or arbitrary JSON supplied by a reject value.

Use stable code/stage, safe relative identifiers, bounded classified details, and local-only diagnostics. Unknown values map to `core.unclassified`; they are not interpolated into its message.

## Artifact Failures

Artifact validation, path, IO, publish, rollback, codegen, batch and package failures must remain typed `artifact.*`/Feature stage failures. Only Interrupted, Windows PermissionDenied, and OS errors 5/32/33 are eligible for bounded atomic rename retry. Exhaustion fails, cleans this run's staging, and never synthesizes success.

## Registered Validation And Repair

- Validators return at most 256 bounded `ValidationIssue` values with validator/code/severity,
  normalized relative path, optional line/column/symbol, repairability and stable fingerprint.
- Adapter logs may retain sanitized stdout/stderr tails locally; graph failure, Run details, IPC and
  repair prompts must not contain arbitrary command tails or absolute machine paths.
- Missing game/tool paths are `local_environment`, never generated-content repair input.
- Only errors classified `generated_content` and individually owned by exactly one non-merge
  generated file may enter semantic repair. Multiple valid owners form one deterministic campaign;
  they do not make the validation globally ambiguous. Any individual shared/ambiguous issue, or any
  non-repairable, local, storage/configuration issue, rejects the whole campaign before model work.
- Campaign diagnostics are bounded, redacted, fingerprinted and persisted before target execution.
  Repair prompts contain only the active Item's issues and current complete role files. Completed
  targets are not replayed after restart; stale diagnostics are discarded before full revalidation.
- `composition.adjustment.stale`, `.invalid` and `.requires_replan` are stable pre-model failures.
  Adjustment details may contain safe Item identity and expected/current hash fingerprints, but no
  instruction text. Structural changes return to Draft planning and never masquerade as source repair.
- Repeated fingerprint against the same checkpoint, unchanged replacement bytes, policy exhaustion,
  cancellation and Truth insufficiency are stable stop conditions, not retryable Provider errors.

## Required Tests

```powershell
cargo test -p ats-kernel product
cargo test -p ats-runtime --all-targets
cargo test -p ats-adapters model_client -- --nocapture
cargo test -p agentthespire-desktop --lib commands::failure
cargo test -p agentthespire-desktop --lib commands::stage2::tests::confirmation_readiness_maps_to_a_resource_action -- --exact
npm run test:frontend
npx tsc -b --pretty false
```

Tests must include malformed/unknown reject canaries, terminal invariants, cancellation cleanup, storage conflict, and absence of raw path/token/provider text.

## Forbidden Patterns

- `Result<T, String>` or `anyhow::Error` at a serialized boundary.
- Persisting `error.to_string()` in Run/Artifact.
- Replacing a known Adapter error with `core.unclassified`.
- Marking cancellation before cleanup or dropping a `JoinHandle` as proof of completion.
- React rendering an unvalidated reject.
