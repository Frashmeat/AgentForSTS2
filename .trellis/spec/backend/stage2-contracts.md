# Stage 2 Contracts And Dependency DAG

> Executable current contract for the Stage 2 modular monolith after the Work Order 7 production cutover.

## 1. Kernel Values

`ats-kernel` owns validated IDs, schema refs/versions, SHA-256, `ActionableFailure`, `BuildInfo`, and project-template value contracts. Constructors and Serde deserialization apply equivalent validation. Kernel imports no IO runtime, provider, game, Feature, or Shell.

## 2. Exact Dependency DAG

```text
ats-kernel       -> []
ats-runtime      -> [ats-kernel]
ats-game-context -> [ats-kernel]
ats-workspace    -> [ats-kernel]
ats-features     -> [ats-kernel, ats-runtime, ats-game-context, ats-workspace]
ats-adapters     -> [ats-kernel, ats-runtime, ats-game-context, ats-workspace]
```

No target crate may depend on deleted `ats-core` or a Shell. Runtime, Game Context, Workspace, and Adapters cannot depend on Features.

```powershell
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
```

Any DAG change requires an approved architecture change plus this spec, script fixture, and Cargo manifests in one task.

## 3. Run And Artifact Envelope

`ats-runtime` owns `VersionedPayload`, RunRecord schema v3, and ArtifactManifest schema v3 without product enums or upper-layer types.

- Feature registry is the only typed request/result decoder.
- Invalid transition leaves the previous Run unchanged; deserialization revalidates timeline, timestamps and terminal exclusivity.
- Artifact context/provenance/extension are versioned payloads.
- FileArtifactStore validates contained regular files, stages complete snapshots, and publishes with same-directory atomic rename.
- Existing final is immutable. Retry is bounded to classified transient Windows rename conflicts; failure cleans this run's staging and never copies/fakes success.

## 4. Pack, Truth, And Contributions

`ats-game-context` loads pinned Pack schema v2, resolves exact Feature slots, verifies immutable Truth Snapshot v2 and returns bounded Evidence. STS2 and synthetic fixtures use the same contracts. Missing contribution, Pack/Snapshot mismatch, unsafe path, unknown Primitive or hash mismatch fails before product work.

Pack may contain declarations/templates/resources and registered Primitive IDs. It cannot contain arbitrary script, native plugin, provider credential, or complete workflow implementation.

## 5. Resource Workspace

`ats-workspace` stores immutable Resource versions with origin/provenance and explicit selection. A Feature references only `resourceId + selectedVersion`; stale or tampered bytes fail. User upload, Pack default, and AI media use the same contract. The registered HTTP Media Adapter supports Images/Chat protocols, cancellation, typed status mapping, bounded bytes, and request-hash provenance; health reports registration, not provider connectivity.

## 6. Prompt And Model

Pinned Feature Recipe + verified Pack contribution + bounded Truth Evidence + selected Resource identities + sanitized project context + one runtime custom-instruction slot + typed output contract produce `ModelRequestSnapshot` v1.

Runtime owns provider-neutral `ModelClient`; Adapters own HTTP. Long Mod/game Prompt strings are forbidden in handler/Shell/Adapter code. Protocol roles, schema/slot IDs, JSON contracts, escaping, truncation and redaction remain code contracts.

## 7. Feature Composition

The shared registry contains exactly 9 current Features. Single generation owns the validated model bundle -> rollback-capable project writes -> real validation -> immutable Artifact -> Run success order. Batch invokes Single child Runs. Complex invokes Plan, Batch/Single, Build and Package. Neither composition creates an alternative Prompt, Resource, file transaction, build or package implementation.

## 8. Shell Cutover

- Tauri product Runs execute only through `Stage2Composition` and `ProjectSession`.
- React uses v3 DTO guards and persisted Run polling.
- Web exposes shared health/catalog/static SPA only.
- CLI `features` emits the shared registry.
- Raw LLM, Prompt preview, old planning/codegen APIs, v2 transport and legacy handler submission are removed.

## 9. Project Session

One session owns the OS lock, v3 repository, cancellation tokens, and task handles. Submit and closing are serialized so no untracked Pending Run exists. Close/switch/shutdown cancels, drains and then releases the lock; timeout retains closing state and lock. Reopen reconciles interrupted records before exposure.

## 10. Machine Acceptance

```powershell
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
git diff --check
```

Repository guards must find no production Core/old Prompt/old handler entry. The deterministic desktop facade E2E must prove Truth -> plan -> single generate -> compile -> persisted succeeded Run v3 -> Artifact v3/hash -> no staging -> drain/reopen lock.

Full release candidate and installed-app/real-game behavior remain separate explicit gates.
