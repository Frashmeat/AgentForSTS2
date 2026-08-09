# Error Handling

> Active Stage 2 failure, cancellation, shell mapping, and redaction contract.

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
