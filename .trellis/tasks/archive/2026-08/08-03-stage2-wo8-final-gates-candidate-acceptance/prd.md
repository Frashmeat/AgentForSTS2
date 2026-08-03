# Stage 2 Work Order 8: Final Gates, Candidate And Human Acceptance

## Goal

Independently verify the committed Stage 2 production cutover, create a new Windows candidate bound
to that exact clean commit after explicit user authorization, and collect installed-app acceptance.
No old candidate, renamed artifact, fixture-only output, or uncommitted code may represent Stage 2.

## Bound Commit

Initial cutover commit: `b8587cd6` (`feat(stage2): cut shells over to feature runtime`). The exact
40-character commit recorded by a future candidate must be read from Git at candidate preflight.

## Phase 1: Independent Machine Gates

- Confirm branch `rust`, clean tracked worktree, no active product dependency on deleted Core, and
  exactly 9 shared Features.
- Re-run DAG self-test/real gate, workspace check/test/clippy, frontend test/type/build, desktop
  baseline/`ml-rembg`/`e2e` feature checks, legacy Prompt/handler guards, rustfmt and diff check.
- Re-run deterministic desktop facade E2E and verify RunRecord v3, ArtifactManifest v3/hash, no
  staging, and project lock reacquisition.
- Do not call an external LLM/media provider as a machine gate; Adapter protocol and redaction tests
  are deterministic, while provider connectivity/quality is environment acceptance.

## Phase 2: Candidate Authorization And Build

- A new candidate is required because the Shell/runtime code differs materially from the accepted
  pre-Stage-2 candidate.
- Before invoking `scripts/release-candidate.ps1`, obtain explicit user authorization as required by
  the existing release constraint. Machine gates may complete before this authorization point.
- Candidate preflight requires a clean tracked worktree and one new build ID. Build baseline and ML
  through the only release-candidate entry; do not manually invoke bundle collection or rename old
  artifacts.
- Do not delete or overwrite any existing release, verification, `.tmp`, user project, Run,
  Artifact, or real acceptance evidence.
- Verify `release-verification.json`, final executable BuildInfo, variant manifests, installer path
  containment, sizes, SHA-256, temporary residue, and signature status.

## Phase 3: Human Installed-App Acceptance

The user operates installers and desktop UI. The Agent may inspect files, logs, Run records,
Artifact manifests/directories, hashes, and lock state only.

Recommended order:

1. Install and launch the ML NSIS candidate; confirm BuildInfo and health reflect the candidate.
2. Open an existing STS2 project and confirm verified Truth is ready/importable.
3. Generate one new uniquely identified test Mod through the Stage 2 Feature UI.
4. Confirm the persisted v3 Run reaches `succeeded`; no fixture or prior Run may substitute.
5. Recompute the new final ArtifactManifest and every file hash; confirm no `.staging-*` for this run.
6. Exercise AI resource generation when a real provider configuration is available; distinguish
   Adapter registration from provider connectivity and generated-image quality.
7. Exit normally, then prove the same project OS lock can be acquired again.
8. Record any real game load/behavior check separately from compile/package success.

## Completion Rule

- Machine gates and candidate verification do not complete human acceptance.
- Do not mark the task complete/archive until the user explicitly confirms the installed-app result.
- After explicit confirmation, update this PRD/task state, archive with `--no-commit`, and commit the
  WO8 evidence. Push and rebase remain separately authorized operations.

## Required Commands

```text
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p agentthespire-desktop --features ml-rembg
cargo check -p agentthespire-desktop --features e2e
npm run test:frontend
npx tsc -b --pretty false
npm run build
cargo run -p ats-cli -- features
targeted deterministic desktop facade E2E
changed/all production Rust: rustfmt --edition 2024 --check
git diff --check
```

The complete release candidate is intentionally not listed as an automatically authorized command;
it begins only after the explicit authorization gate above.

## Phase 1 Evidence

Completed on 2026-08-03 against clean commit
`b8587cd608c3f8b96afd9e98ac34fc203d8aa5ae`.

```text
DAG self-test and real metadata gate                         passed
legacy production Prompt/handler guard                       clean
cargo fmt --all --check                                      passed
cargo check --workspace --all-targets                        passed
cargo test --workspace --all-targets                         94 passed
cargo clippy --workspace --all-targets -- -D warnings        passed
desktop ml-rembg check                                       passed
desktop e2e check                                            passed
deterministic desktop facade E2E                             passed
ats-cli features                                             exactly 9
frontend tests                                               8 passed
TypeScript build mode check                                  passed
Vite production build                                       passed
git diff/status                                              clean
```

`release-candidate.ps1 -Variant All -PlanOnly` passed without writing candidate files. It selected:

```text
build ID: rc-20260802T200726Z-b8587cd608c3
steps: preflight, frontend-tests, frontend-build, workspace-check, workspace-test,
       workspace-clippy, build-baseline, verify-baseline, build-ml, verify-ml
baseline features: []
ML features: [ml-rembg]
```

Phase 2 is waiting only for explicit authorization to run this real complete candidate.

## Phase 2 Attempt Evidence

The user explicitly authorized the complete baseline + ML candidate. The first real invocation used
the PlanOnly-selected ID `rc-20260802T200726Z-b8587cd608c3`, but the controlling process was
interrupted while `workspace-test` was running. The candidate atomically preserved this failure:

```text
verification: artifacts/release/rc-20260802T200726Z-b8587cd608c3/release-verification.json
status: failed
failed step: workspace-test
exit code: 1073807364 (0x40010004 process interruption/control exit)
later steps: skipped
variants built: none
```

This is valid interruption evidence, not a product test failure. Preserve the candidate directory,
never reuse its build ID, and retry the complete authorized workflow with a newly generated ID.

The authorized retry completed successfully with a new identity:

```text
build ID: rc-20260803T005017Z-b8587cd608c3
commit: b8587cd608c3f8b96afd9e98ac34fc203d8aa5ae
verification: artifacts/release/rc-20260803T005017Z-b8587cd608c3/release-verification.json
steps: 10/10 succeeded
variants: baseline verified; ml [ml-rembg] verified
runtime BuildInfo: verified for both variants
installer size/SHA-256 independent recomputation: 4/4 matched
candidate temporary/staging residue: 0
installer signatures: 4/4 NotSigned
```

Phase 2 is complete. Phase 3 is waiting for the user's installed ML candidate acceptance and the
Agent's read-only inspection of the newly produced Run/Artifact/lock evidence.

## Phase 3 Installed E2E Finding And Repair

The user authorized the Agent to operate the installed UI for E2E. The ML NSIS candidate launched
with the expected BuildInfo, opened the existing `ATSReleaseSmoke` project, imported verified Truth,
and reached a real provider. A normal Plan succeeded but emitted descriptive `requiredEvidence`
strings. Single generation treated those strings as Truth symbol substrings, found no records, and
failed before the model call with a flattened `feature.execution_failed`.

A controlled retry using the real `ICustomModel` symbol proved the downstream chain: succeeded v3
Run, real dotnet validation, immutable ArtifactManifest v3, independently matching manifest/file
hashes, zero staging for that Artifact, normal process exit, and project lock reacquisition. This is
diagnostic evidence, not acceptance of the normal user path.

The authorized repair is intentionally breaking:

```text
mod.plan result v2: evidenceRequirements (descriptive intent)
pack.mod-generate-single v2: per-item evidenceQueries { symbols, terms }
mod.generate.single request v2
mod.generate.batch request v2
```

Every Pack query group must match current verified Truth before model execution; matched records are
bounded and deduplicated. Single and Batch child Runs retain typed Truth/Pack/Resource/Model/
Validation/Artifact/Run failure codes instead of flattening known failures.

This changes the built-in Pack hash, so the successful candidate
`rc-20260803T005017Z-b8587cd608c3` cannot represent the repaired code. Preserve it and all E2E
evidence. After the repair is committed, repeat affected machine gates, regenerate Truth from the
preserved Stage 1 source, obtain explicit authorization for a new complete candidate, and repeat
installed-app acceptance with an ordinary natural-language Plan.

Repair implementation and targeted verification completed before commit on 2026-08-03:

```text
ats-features all-target tests                              18 passed
desktop composition unit + single/composition integration 8 passed
ats-features + desktop targeted clippy                     passed
frontend tests                                             10 passed
TypeScript build mode check                                passed
Stage 2 dependency DAG                                     passed
CLI catalog                                                exactly 9 Features
rustfmt and git diff check                                 passed
```

The repair was then committed as
`fcb8f6561d5f513ef2ea37ad2b1f31e69196e987` (`fix(stage2): separate plan evidence from truth
queries`).

## Repair Commit Repeated Phase 1 Evidence

WO8 Phase 1 was independently repeated against the clean repair commit on 2026-08-03:

```text
dependency DAG self-test and real metadata gate             passed
legacy Core/Prompt/handler production guards                clean
cargo fmt --all --check                                     passed
cargo check --workspace --all-targets                       passed
cargo test --workspace --all-targets                        95 passed
cargo clippy --workspace --all-targets -- -D warnings       passed
desktop ml-rembg check                                      passed
desktop e2e feature check                                   passed
frontend tests                                              10 passed
TypeScript build mode check                                 passed
Vite production build                                      passed
ats-cli features                                            exactly 9
exact deterministic desktop facade E2E                     1 passed
git diff/status                                             clean
```

The exact facade E2E again proved verified Truth -> natural-language Plan v2 -> Pack Evidence Query
v2 -> Single Generate -> real compile -> succeeded RunRecord v3 -> ArtifactManifest v3/hash -> no
staging -> drained session -> project lock reacquisition.

This completes the repaired commit's machine gate only. The changed Pack hash still requires a fresh
Truth Import. A new complete candidate remains behind its separate authorization gate, and only that
new candidate may be used for installed-app acceptance.

## Final Candidate And Installed Acceptance

The later run-scoped generation output-contract repair was committed as
`bde2d64969d9b8d0b41357dfc3bb36932e9b1056` (`fix(model): specialize generation output
contracts`). WO8 machine gates were repeated against that clean commit and passed, including 105
workspace tests, full workspace clippy/check, desktop `ml-rembg`/`e2e` checks, Feature 24/24,
Adapter 29/29, desktop integration 7/7, exact deterministic facade E2E, frontend 10/10,
TypeScript/Vite build, the 9-Feature CLI catalog, DAG/legacy guards, rustfmt, diff check, and Recipe
SHA-256 verification.

After explicit user authorization, the complete replacement candidate succeeded:

```text
build ID: rc-20260803T125802Z-bde2d64969d9
commit: bde2d64969d9b8d0b41357dfc3bb36932e9b1056
verification: artifacts/release/rc-20260803T125802Z-bde2d64969d9/release-verification.json
steps: 10/10 succeeded
variants: baseline verified; ml [ml-rembg] verified
runtime BuildInfo: verified for both variants
installer size/SHA-256 independent recomputation: 4/4 matched
candidate temporary/staging residue: 0
installer signatures: 4/4 NotSigned
```

The user installed and launched the ML NSIS candidate. The installed runtime reported the expected
ML variant, build ID, 9 Features, `sts2` Pack, and ready Truth. The existing `ATSReleaseSmoke`
project was opened and a new natural-language `custom_code` request completed through the normal
Plan and Single Generate UI:

```text
artifact ID: run-scoped-contract-gate-20260803-2114
run ID: run-000000000000000018c84e0f201446d8-00000001
run schema/status/attempts: v3 / succeeded / 1
validation primitive: code.dotnet-validate
generated files: 1
manifest schema: v3
manifest SHA-256: 44c2fb9fbbaff10a7cea44581a68c9a63386fa43711c53edb0f795a7cae74448
source size/SHA-256: 146 / 6e1528aacaf101a827a47b52124f6157d8392745f524233a0bb0ccb5feb28bd5
this Artifact staging residue: 0
```

The RunRecord result reference, manifest producing Run ID, snapshot file, and published file all
matched independently. The two preserved `.staging-*` directories remain historical evidence from
2026-08-01 and are unrelated to this Run. A live lock probe was rejected while the app held the
project; after closing through the normal window control, process `60156` exited and the same OS
lock was acquired and released successfully. No real game load/behavior claim is made beyond the
successful generated-code validation.

On 2026-08-03 the user explicitly confirmed this installed-app result and authorized PRD/task
completion and `--no-commit` archival. WO8 is complete; release signing/SmartScreen and Git
push/rebase remain outside this acceptance.
