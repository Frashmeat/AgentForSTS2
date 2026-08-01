# Logging And Diagnostic Guidelines

> Current Rust/Tauri/Web/CLI logging and release-output contract.

## Runtime Surfaces

- Core, `ats-web`, and `ats-cli` use `tracing` for structured lifecycle and adapter events.
- The desktop shell currently uses bounded `eprintln!` messages for composition/startup and IPC event-delivery failures where no tracing subscriber is guaranteed.
- Product-visible failures use `ActionableFailure`; logs are diagnostic context and never become a second user-facing error schema.
- Run lifecycle facts live in `RunRecord.timeline`; Artifact provenance lives in `ArtifactManifest`. Logs must not be treated as either authority.

## What To Record

Prefer stable structured fields when available:

```text
run_id, project-relative artifact id, stage, attempt,
provider id, model id, processor, variant, build_id,
bytes transferred, stable reason code
```

- Log expected lifecycle transitions at `info` only when useful to operators.
- Use `warn` for classified recoverable degradation, retry, fallback, or event delivery failure.
- Use `error` for an unexpected boundary failure that cannot be represented only by the returned `ActionableFailure`.
- High-volume progress belongs at `debug` and must be bounded.

## Redaction

Never log API keys, GitHub tokens, authorization headers, cookies, full prompts/outputs, provider response bodies, URL queries/fragments, or full workstation configuration.

Absolute paths may be printed only to a local operator console when the path itself is required to diagnose workstation setup. They must not enter IPC, RunRecord, ArtifactManifest, `release-verification.json`, or a shareable bug-report summary. Prefer project-relative paths and stable diagnostic IDs.

Unknown error text must not cross a serialized/product boundary. `tracing` may retain a local stack/error chain for operator diagnosis, while `ActionableFailure` uses a fixed safe message and diagnostic ID.

## Release And CI Output

- `scripts/release-candidate.ps1` streams native command stdout/stderr to the invoking console.
- `release-verification.json` stores only stable step IDs, exit codes, bounded summaries, build identity, manifest references, and hashes. It never captures raw command output.
- CI logs can contain compiler/test output but cannot print environment secrets or configuration files.
- A failed command must keep its real nonzero process result; scripts must not print `PASSED` after failure.

## Good / Base / Bad

- Good: a failed release step prints its compiler output locally while verification stores `workspace-clippy`, exit code, and `Rust workspace clippy failed` only.
- Base: an ML failure logs a stable fallback category and the ArtifactManifest records the processor/fallback fact separately.
- Bad: a provider body, token, complete prompt, absolute private path, or exception canary is copied into verification, Run history, or UI.
