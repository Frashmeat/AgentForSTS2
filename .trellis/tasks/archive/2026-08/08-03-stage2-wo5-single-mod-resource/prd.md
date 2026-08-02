# Stage 2 Work Order 5: Single Mod And Resource Vertical Slice

## Goal

Migrate `mod.plan`, `mod.generate.single`, and `resource.prepare` into the Stage 2 modules without
switching the current Shell. Prove one STS2 single-item path from typed request and verified context
through model assembly, selected resources, rollback-capable project writes, compile validation,
immutable Artifact publication, and a typed Run result.

## Ownership

- `ats-runtime`: game-neutral cancellation, media request/response port, validation-runner port, and
  rollback-capable project-file transaction port.
- `ats-features`: typed plan/single/resource contracts, Feature Recipes, output decoding, workflow
  ordering, resource selection requirements, Artifact context/provenance/extension payloads.
- `ats-game-context`: verified Pack contributions and Truth Evidence only.
- `ats-workspace`: immutable Resource versions, selection, and origin provenance.
- `ats-adapters`: atomic project writes/rollback, registered validation processes, file Artifact and
  Resource repositories. Adapters do not select product workflow.
- `game_packs/sts2`: mod types, stable engineering guidance, output path templates, resource roles,
  validation Primitive IDs, and finite constraints. It does not contain an executable workflow.

Legacy `ats-core` planning/codegen/resource handlers and current Shells remain unchanged until WO7.

## Contracts

### `mod.plan`

- Pinned Recipe owns the generic planning task and typed output schema.
- `mod.plan.guidance` owns game-supported item types and stable planning guidance.
- The result is strict versioned `PlanItem`; missing fields do not silently default.
- Planning does not invent API/hook facts. Requirements needing API behavior explicitly request Truth
  retrieval during generation.

### `resource.prepare`

- User upload, AI-generated, and Pack-default inputs all enter the existing Resource v1 repository.
- The Feature validates logical role/media type against `resource.prepare.specs` before ingest.
- AI generation records provider, model, and request SHA-256; Pack default records exact Pack identity
  and contribution slot. Selection points to one immutable version.
- Resource bytes/absolute input paths are transient and never enter Run result or model snapshot.

### `mod.generate.single`

```text
typed request + pinned Recipe + verified Pack contribution + Truth Evidence
+ selected Resource identities + sanitized project context + Custom Instructions
-> ModelRequestSnapshot
-> strict GeneratedModBundle
-> Pack-owned path/rule validation
-> rollback-capable project writes
-> registered compile validation
-> ArtifactManifest v3 publication
-> RunRecord v3 success
-> commit project transaction
```

- Generated paths are normalized project-relative paths and must match Pack-declared target templates.
- Model output contains text files only. Binary resources come from selected Resource versions.
- Formal writes remain rollback-capable through validation, Artifact publication and terminal Run
  transition. Validation/publish/terminal failure restores prior bytes and removes a newly published
  Artifact when needed.
- Artifact contexts record Pack and Truth identities; provenance records model request and selected
  Resource identities; Feature extension records generated file count, validation Primitive and
  acceptance items.
- Runtime/Adapters never decode Feature payloads or branch on game ID.

## Failure And Cancellation

- Invalid request, Pack contribution, Truth identity, resource role/selection, model output, path,
  validation rejection/unavailability, project write, Artifact publication and Run transition are
  distinct errors at their owning boundary.
- Cancellation is checked before model invocation and after every external/transactional stage.
  Cancellation after project writes rolls back; cancellation after Artifact publication removes that
  run-scoped final Artifact before rollback.
- Compile output is bounded and sanitized by the Adapter. Provider bodies, prompts, credentials and
  absolute paths never enter typed Run/Artifact results.

## Machine Acceptance

- STS2 and synthetic Pack use the same plan/generation/resource contracts.
- `mod.plan` Recipe contains no game terms; different Pack guidance changes request identity.
- All three Resource origins produce the same immutable selected-resource contract with distinct
  provenance; unsupported role/media type fails before repository mutation.
- An isolated STS2 fixture completes model decode, formal writes, real registered compile gate,
  Artifact publication, manifest/hash recomputation, Run success and transaction commit.
- Compile rejection, Artifact failure and cancellation restore previous project files and leave no
  final Artifact or transaction staging residue.
- Generated traversal, undeclared path/type, missing Truth/Pack/resource identity, malformed output,
  unknown validation Primitive and duplicate file are rejected before success.
- Runtime and generic Recipes contain no STS2/Godot/BaseLib/C#/.NET terms; Feature source has no long
  production Prompt constants.

## Required Gates

```text
cargo test -p ats-runtime
cargo test -p ats-game-context
cargo test -p ats-workspace
cargo test -p ats-features
cargo test -p ats-adapters
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
rustfmt --edition 2024 --check <changed Rust files>
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

## Out Of Scope

- Batch/complex composition, project build/package Feature migration (WO6).
- Tauri/Web/CLI/React cutover and legacy Core deletion (WO7).
- A new installer/release candidate (WO8 after explicit confirmation if required).
- Mutation of old Run/Artifact/release evidence or deletion of user data.

## Completion Evidence

- Added game-neutral cancellation, media, validation and rollback-capable project-write ports in
  `ats-runtime`, with filesystem/process implementations in `ats-adapters`.
- Added typed/pinned `mod.plan`, `resource.prepare` and `mod.generate.single` Features. Generic
  Recipes contain no STS2/toolchain terms; the pinned STS2 Pack owns item types, guidance, file
  roles/templates, resource specs and the registered validation Primitive.
- Resource bytes from upload, AI and Pack-default origins use Resource v1. Generation reads only the
  exact manifest-selected version; stale selection and tampered blob reads fail.
- `src-tauri/tests/stage2_single_mod.rs` proves real `dotnet build`, six-file ArtifactManifest v3
  publication, manifest SHA-256 recomputation, RunRecord v3 success, Custom Instructions appearing
  once and no staging/transaction residue. Compile rejection, Artifact failure and cancellation
  restore an existing source byte-for-byte and remove newly written resource files.
- A synthetic Pack resolves the same generation/resource schemas and supports a different generated
  file template without a Feature game branch.

Machine gates passed:

```text
cargo test -p ats-runtime
cargo test -p ats-game-context
cargo test -p ats-workspace
cargo test -p ats-features
cargo test -p ats-adapters
cargo test -p agentthespire-desktop --test stage2_single_mod
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
rustfmt --edition 2024 --check <all WO5 changed Rust files>
generic Runtime/Recipe game-term guard
pinned Recipe/Pack SHA-256 recomputation
git diff --check
```

`cargo fmt --all -- --check` additionally reports formatting differences only in the untouched,
already committed `crates/ats-core/src/image_gen/chat_image.rs`. WO5 does not rewrite that unrelated
legacy file; all Rust files changed by this Work Order pass explicit `rustfmt --check`.
