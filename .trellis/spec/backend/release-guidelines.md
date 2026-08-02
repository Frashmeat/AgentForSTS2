# Windows Release Candidate Guidelines

> Executable contract for `scripts/release-candidate.ps1`, `build.ps1`, release manifests, and Windows CI.

## Entry Points And Ownership

```text
scripts/release-candidate.ps1 -Variant Baseline|Ml|All [-BuildId <id>] [-PlanOnly]
build.ps1 -Variant Baseline|Ml [-BuildId <id>] [-PlanOnly]
```

- The candidate script owns orchestration and `release-verification.json`.
- `build.ps1` owns commit/variant/features/build-id injection, isolated target/bundle paths, final GUI BuildInfo handshake, and variant `release-manifest.json` publication.
- `src-tauri/build.rs` derives candidate identity inputs from actual Cargo cfg and `ats-kernel::BuildInfo` validates the embedded result. Scripts do not accept caller-supplied feature strings.

One candidate uses one build ID for all requested variants:

```text
artifacts/build/<variant>/<build-id>/target/
artifacts/release/<build-id>/
  release-verification.json
  baseline/release-manifest.json
  ml/release-manifest.json
```

## Candidate Step Contract

Fixed required order:

```text
preflight
frontend-tests
frontend-build
workspace-check
workspace-test
workspace-clippy
build-<variant>
verify-<variant>
```

- PlanOnly reads authoritative variant plans and writes no candidate files.
- A real candidate requires a clean Git worktree and a candidate root that does not already exist.
- Any nonzero/exception marks the current step failed, later pending steps skipped, overall status failed, and the script process nonzero.
- The CLI maps closed step IDs to fixed commands. It has no arbitrary command/scriptblock/feature parameter.

## Verification Schema

`release-verification.json` schema v1 contains:

- `candidate { commit, buildId, requestedVariants }`
- overall `status`, `startedAt`, optional `completedAt`
- `steps[] { id, status, startedAt?, completedAt?, exitCode?, summary? }`
- `variants[] { variant, features, buildId, releaseManifest, runtimeBuildInfoVerified, artifacts[], verified }`
- optional `failure { stepId, exitCode?, summary }`

Writes use a same-directory temporary file and atomic replacement. `running` is a valid interrupted fact; only all required successful steps and verified requested variants allow `succeeded`.

Variant verification recomputes manifest identity, path containment, allowed installer extension, byte length, and SHA-256. `runtimeBuildInfoVerified=true` is written only when the matching `build-<variant>` step succeeded and the manifest independently passed.

## CI Boundary

- Linux CI remains the authority for frontend, workspace check/test, and workspace clippy.
- `windows-desktop-variants` compiles `agentthespire-desktop` with `--no-default-features` and with `--features ml-rembg`.
- Ordinary push/PR CI does not call `tauri build`, create installers, download models/game data, operate the game UI, or require signing credentials.

## Validation Matrix

| Condition | Required result |
| --- | --- |
| Baseline-only | No ML step or stale ML result |
| All variants succeed | Same commit/build ID, exact feature sets, all hashes match, overall succeeded |
| Dirty worktree | Preflight failed before expensive commands |
| Existing candidate root | Reject without overwriting prior evidence |
| Build/manifest identity mismatch | Verify step failed; no success summary |
| Artifact traversal/missing/size/hash mismatch | Verify step failed |
| Raw exception contains a secret/path canary | Verification keeps only the stable step summary |
| Process interrupted | Last atomic JSON remains parseable and not succeeded |

## Good / Base / Bad

- Good: `-Variant All` produces two isolated manifests and one successful verification bound to one commit/build ID.
- Base: `-PlanOnly` proves step/path/feature selection without creating a candidate or claiming release success.
- Bad: a script scans shared `src-tauri/target`, trusts a preexisting hash, accepts `--features=e2e`, or emits success after a failed required step.

## Required Tests

```text
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/test-release-candidate.ps1
pwsh -NoProfile -File scripts/test-build-plan.ps1
git diff --check
```

Full frontend/workspace checks, clippy, both Tauri bundles, and installer E2E remain an explicit high-cost candidate gate.
