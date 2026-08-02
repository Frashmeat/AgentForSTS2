# Logging And Diagnostic Guidelines

> Active Stage 2 runtime, desktop, Web/CLI, and release-output contract.

## Authorities

- `RunRecord.timeline` is Run lifecycle authority.
- `ArtifactManifest` is artifact identity/provenance authority.
- `ActionableFailure` is the product-visible error authority.
- Logs are local operator diagnostics only and never replace these contracts.

## Runtime Surfaces

Adapters, `ats-web`, and `ats-cli` should use structured lifecycle logging when a subscriber exists. The desktop composition/startup path may use bounded `eprintln!` where no subscriber is guaranteed. Shell event delivery failure must not mutate persisted Run state.

Prefer stable fields:

```text
run_id, feature_id, stage, attempt, provider_id, model_id,
artifact_id, project_relative_path, variant, build_id, byte_count, reason_code
```

Use `info` for useful lifecycle milestones, `warn` for classified retry/degradation, `error` for unexpected local boundary failures, and `debug` for bounded high-volume progress.

## Redaction

Never log API keys, GitHub tokens, Authorization/cookies, full prompts/model output, provider response bodies, complete configuration, or URL query/fragment.

Absolute paths may appear only in a local operator console when required for workstation setup. They must not enter IPC, RunRecord, ArtifactManifest, frontend state, `release-verification.json`, or shareable summaries. Prefer safe relative paths and stable IDs.

## Release And CI

- Release scripts may stream native stdout/stderr to the invoking console.
- `release-verification.json` stores stable step IDs, exit code, bounded summary, commit/build identity, manifest reference and hashes only.
- A failed native command retains its nonzero result. Scripts must not print or persist `PASSED` after failure.
- CI may show compiler/test output but must not print secrets or configuration files.

## Good And Bad

Good: log `feature_id=mod.generate.single stage=artifact.publish attempt=2 reason=sharing_conflict`; persist only the typed Run outcome.

Bad: copy a complete model request, Windows absolute path, provider error body or exception chain into Run history, UI, artifact provenance or release verification.
