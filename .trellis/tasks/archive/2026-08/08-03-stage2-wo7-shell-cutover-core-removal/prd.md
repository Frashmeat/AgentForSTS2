# Stage 2 Work Order 7: Shell Cutover And Core Removal

## Goal

Switch Tauri, Web, CLI and React from the legacy `ats-core` product contracts to the Stage 2
Feature registry, typed Run/Artifact v3 contracts and explicit composition roots. Delete
`crates/ats-core`, its long production Prompt bundle and every replaced v2 handler/API after the
new product path is executable. This is an authorized breaking migration; a renamed or hidden
legacy monolith is not acceptance.

## Scope And Ownership

- `ats-kernel`: serialized actionable failure and build identity value contracts shared by Shells.
- `ats-runtime`: Run repository/executor ports, cancellation ownership and Feature envelope
  lifecycle; no product `RunKind`/`RunResult` enum.
- `ats-game-context`: built-in Pack resources and declarative Truth acquisition/index contracts.
- `ats-workspace`: project identity, OS lock, recents and project-template workspace behavior.
- `ats-features`: typed Feature facade, request/result decoding and orchestration only.
- `ats-adapters`: provider HTTP, filesystem repositories, registered process/media/index/build and
  package implementations.
- Shells: configuration loading, transport DTO conversion, state ownership and dependency wiring.
  Shell handlers do not own Prompt, game rules, generation decoding or filesystem transactions.

## Breaking Contract

- Tauri/React Run transport becomes schema v3: `id`, `featureId`, `status`, versioned `request`,
  optional versioned `result`, typed failure/cancellation and timeline. The v2 `kind`, untyped
  `payload` and center `RunResult` union are removed.
- Product commands submit typed Features (`mod.plan`, `resource.prepare`, `mod.generate.single`,
  `mod.generate.batch`, `mod.generate.complex`, `log.analyze`, `project.build`,
  `project.package`) through one facade. Raw prompt completion, Prompt preview and legacy handler
  submission commands are deleted.
- Web and CLI expose the same Feature catalog/contract identities. They do not keep legacy planning
  or codegen implementations.
- Existing project data and old Run/Artifact files are preserved. Unsupported old schema is read as
  historical/unsupported evidence or rejected with a typed action; it is never rewritten as v3.

## Shell Flow

```text
React / Web / CLI typed input
-> Shell transport validation
-> FeatureFacade
-> Pack contribution + fixed Truth context + selected Resources
-> typed Feature service
-> Runtime ports / registered Adapters
-> RunRecord v3 + ArtifactManifest v3
-> versioned typed result payload
```

## Failure And Cancellation

- Kernel owns the only serialized failure shape; shell-local invalid reject values use a fixed
  `core.unclassified` fallback and never serialize unknown error text.
- One project session owns the OS lock, Run repository, cancellation tokens and task handles.
  close/switch/exit rejects new work, cancels and drains before releasing the lock.
- Feature failure is normalized at the facade boundary and terminalized exactly once. Cancellation
  becomes `cancelled` only after the service and rollback stop; completed child Runs remain terminal.
- Provider bodies, prompts, credentials and absolute project/runtime paths do not enter Run, IPC,
  Web responses or UI errors.

## Machine Acceptance

- Cargo metadata and repository search contain no `ats-core` package, path dependency or Rust import.
- Repository search contains no legacy production Prompt bundle or replaced v2 handler entry names.
- Tauri invokes only the Stage 2 facade for product generation/log/build/package Runs; TypeScript
  guards and UI consume the exact v3 envelope and typed Feature payloads.
- Web and CLI compile against the shared Feature registry and expose matching Feature IDs/schemas.
- Existing project open/close/lock recovery and settings/build identity remain functional.
- A deterministic desktop E2E proves open project -> plan -> single generation -> succeeded v3 Run
  -> ArtifactManifest v3/hash -> no staging -> normal exit/reopen lock.
- Baseline and `ml-rembg` desktop feature sets compile; ML identity remains truthful even if a media
  request is not part of the deterministic cutover fixture.

## Required Gates

```text
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
repository legacy-entry and long-Prompt guards
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p agentthespire-desktop --features ml-rembg
cargo check -p agentthespire-desktop --features e2e
npm run test:frontend
npx tsc -b --pretty false
npm run build
targeted desktop Feature facade/E2E tests
rustfmt --edition 2024 --check <changed Rust files>
git diff --check
```

## Out Of Scope

- Creating the WO8 installer/release candidate or mutating previous release evidence.
- A second real game, dynamic plugins, marketplace or arbitrary Pack code.
- Automatic migration that overwrites historical RunRecord/ArtifactManifest files.
- Push, rebase, deployment, signing or deletion of user/release evidence.

## Completion Evidence

Completed on 2026-08-03.

- Tauri product execution uses `Stage2Composition`; React uses v3 typed transport and persisted Run polling; Web/CLI expose the shared catalog.
- `crates/ats-core`, its Prompt bundle, old v2 handlers/routes/IPC and replaced React stores/components were physically removed.
- Kernel actionable failure deserialization now reuses validated construction and rejects malformed wire contracts.
- A registered HTTP Media Adapter supports OpenAI Images and Chat Completions image protocols; `resource.prepare` sends AI output through the same Resource Workspace and records provider/model/request hash provenance.
- The deterministic desktop facade E2E proves STS2 project/lock, verified Truth, `mod.plan`, `mod.generate.single`, real compile, persisted succeeded RunRecord v3, ArtifactManifest v3/file hashes, no `.staging-*`, drain and lock reacquisition.
- Core search hits only the intentional dependency-DAG negative fixture. Legacy Prompt/handler search is empty. CLI reports exactly 9 shared Features.

Machine gates passed:

```text
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo test --workspace --all-targets                         # 94 passed
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p agentthespire-desktop --features ml-rembg
cargo check -p agentthespire-desktop --features e2e
npm run test:frontend                                        # 8 passed
npx tsc -b --pretty false
npm run build
cargo run -p ats-cli -- features                              # 9 Features
changed Rust files: rustfmt --edition 2024 --check
git diff --check
```

The repository drive reached zero free bytes during the gate. The pre-existing rebuildable root
`target` cache was moved intact to `E:\AgentTheSpire-old-target-before-wo7-complete`, and gates ran
with `CARGO_TARGET_DIR=E:\AgentTheSpire-cargo-target-wo7`. Release, verification, `.tmp`, user
projects and real acceptance evidence were not modified. A partial interrupted move copy remains at
`E:\AgentTheSpire-old-target-before-wo7` and was not deleted without separate cleanup authority.

No release candidate, installer, push or rebase was executed.
