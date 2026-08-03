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
cargo test -p agentthespire-desktop --lib commands::failure
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
