# Stage 2 Contracts And Dependency DAG

> Executable current contract for the Stage 2 modular monolith after the Work Order 7 production cutover.

## 1. Kernel Values

`ats-kernel` owns validated IDs (including stable Item/Item Type/field/locale IDs), schema refs/versions, SHA-256, `ActionableFailure`, `BuildInfo`, and project-template value contracts. Constructors and Serde deserialization apply equivalent validation. Kernel imports no IO runtime, provider, game, Feature, or Shell.

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

`ats-game-context` loads pinned Pack schema v3, resolves exact Feature slots, verifies immutable Truth Snapshot v2 and returns bounded Evidence. Pack v3 has one top-level `itemTypes` catalog containing validated localized names, restricted generic field descriptors, required locales, executable Evidence Queries and required Resource roles. STS2 and synthetic fixtures use the same contracts. Missing/duplicate item types or fields, invalid constraints, missing contribution, Pack/Snapshot mismatch, unsafe path, unknown Primitive or hash mismatch fails before product work.

`mod.plan` result v2 owns descriptive `evidenceRequirements`. The Pack v3 item catalog owns
per-item `evidenceQueries` with explicit symbol/term fields. Capability readiness and Single
generation require every query group to match the active Truth Snapshot and never interpret Plan
prose as an index key. A missing current Snapshot blocks every declared type; a partially matching
Snapshot blocks only affected types with typed, query-indexed reasons.

The Pack v3 item catalog also owns each item type's `requiredResourceRoles`.
`pack.mod-plan-guidance` v3 owns only cross-item planning guidance;
`pack.mod-generate-single` v3 owns only generation guidance, validation Primitive and exact
generated-file roles. The Plan model-output contract excludes internal Resource role IDs;
`ModPlanService` attaches catalog values after typed model validation. The Single contribution
must cover exactly the catalog's item type IDs, preventing a declared-but-unexecutable type.

`ats-workspace` owns ItemDefinition schema v1: stable Item identity/type, canonical structured
field values, behavior intent, explicit localization status and exact Resource version bindings.
Validated Serde and deterministic ordered serialization produce the definition SHA-256 used by
future Run/Artifact provenance; Runtime remains unaware of game-specific types.

Pack may contain declarations/templates/resources and registered Primitive IDs. It cannot contain arbitrary script, native plugin, provider credential, or complete workflow implementation.

### Scenario: Pack v3 Item Catalog, Definition Identity, And Readiness

#### 1. Scope / Trigger

Use this contract whenever adding an Item type, generic field, required locale, Truth query or
Resource role, or whenever a Run begins to consume an ItemDefinition. It prevents Plan, Single,
React and Batch from becoming parallel type registries.

#### 2. Signatures

```rust
GamePackLoader::load(bytes: &[u8], expected_sha256: &Sha256Digest)
    -> Result<LoadedGamePack, GamePackLoadError>

LoadedGamePack::item_types(&self)
    -> &BTreeMap<ItemTypeId, ItemTypeDescriptor>
LoadedGamePack::item_type(&self, id: &ItemTypeId)
    -> Option<&ItemTypeDescriptor>

ItemCapabilityCatalog::evaluate(
    pack: &LoadedGamePack,
    truth: Option<&VerifiedTruthSnapshot>,
) -> Result<ItemCapabilityCatalog, CapabilityEvaluationError>

ItemDefinition::definition_hash(&self)
    -> Result<Sha256Digest, ItemDefinitionError>
```

Implementations live in `crates/ats-game-context/src/pack.rs`,
`crates/ats-game-context/src/item.rs` and `crates/ats-workspace/src/item.rs`.

#### 3. Contracts

The Pack payload is schema v3. The top-level shape is:

```json
{
  "schemaVersion": 3,
  "id": "sts2",
  "displayName": "Slay the Spire 2",
  "itemTypes": [{
    "id": "relic",
    "displayNames": {"eng": "Relic", "zhs": "遗物"},
    "requiredLocales": ["eng", "zhs"],
    "fields": [{
      "id": "rarity",
      "displayNames": {"eng": "Rarity", "zhs": "稀有度"},
      "required": true,
      "value": {
        "kind": "choice",
        "options": [{"value": "common", "displayNames": {"eng": "Common"}}]
      }
    }],
    "evidenceQueries": [{"symbols": ["CustomRelicModel"], "terms": []}],
    "requiredResourceRoles": ["relic.normal"]
  }],
  "contributions": []
}
```

Allowed field `value.kind` values are `text`, `integer`, `boolean`, `choice` and `string_list`,
with the bounded fields represented by `ItemFieldValueSpec`. No descriptor contains UI code.

Capability response entries contain `descriptor`, `ready` and `blockers`. Blocker code is exactly
`truth.snapshot_unavailable` or `truth.evidence_missing`; the latter includes zero-based
`queryIndex`. The response also carries `gamePackId`, `gamePackSha256` and optional
`truthSnapshotId`, so clients must replace rather than merge results from another identity.

ItemDefinition schema v1 fields are `itemId`, `itemType`, `canonicalFields`, `behaviorIntent`,
`localizations` and `resourceBindings`. `canonicalFields`, `localizations` and `resourceBindings`
are ordered maps. The definition SHA-256 covers the complete validated wire object including
`schemaVersion`; it does not include repository timestamps or a mutable selected pointer.

#### 4. Validation & Error Matrix

| Input/state | Result | Work allowed |
| --- | --- | --- |
| Pack bytes do not match pinned SHA-256 | `GamePackLoadError::ContentHashMismatch` | none |
| `schemaVersion != 3` | `GamePackLoadError::UnsupportedSchema` | none |
| empty/duplicate/more than 64 `itemTypes` | `GamePackLoadError::InvalidItemTypes` | none |
| invalid locale/field/query/role descriptor | `GamePackLoadError::InvalidItemType(ItemCatalogError::*)` | none |
| Single generation types differ from catalog IDs | `SingleGenerateError::InvalidPackContribution` | no model/IO |
| no current verified Truth | every declared type blocked with `truth.snapshot_unavailable` | caller must reject submit/model work |
| one Evidence Query has zero records | affected type blocked with `truth.evidence_missing + queryIndex` | caller admits other ready types only |
| Truth belongs to another Pack/hash | `CapabilityEvaluationError::TruthPackMismatch` | none |
| invalid/tampered ItemDefinition wire | Serde error wrapping `ItemDefinitionError` | no identity/persist/Run |
| valid definition content changes | new definition SHA-256 | old Run/Artifact remains immutable |

#### 5. Good / Base / Bad Cases

- **Good**: Pack declares `relic`; all three query groups match current Truth; readiness returns the
  complete descriptor with `ready=true`; ItemDefinition validates and receives a stable hash.
- **Base**: Pack declares `relic`, but no Truth is imported; the descriptor is still visible with
  `ready=false` and `truth.snapshot_unavailable`; a transport/UI readiness gate can offer Truth
  import without submitting a product Run. That transport gate is connected with the Item UI in
  the following Order; the Order 1 evaluator itself performs no Run or model work.
- **Bad**: React hardcodes `card`, or Plan guidance declares Resource roles that differ from the
  top-level catalog. The undeclared React type has no capability entry; duplicated contribution
  facts are forbidden and Single exact-coverage validation fails before model work.

#### 6. Tests Required

- `ats-game-context::pack::item_catalog_rejects_duplicate_types_and_invalid_field_constraints`:
  assert duplicate IDs and inverted integer bounds fail during Pack load.
- `ats-game-context::item::capability_catalog_is_pack_driven_and_truth_scoped`: assert no-Truth,
  matching and missing-query states plus exact `truth.evidence_missing` serialization.
- `ats-workspace::item::definition_hash_is_stable_and_covers_canonical_content`: assert wire
  round-trip preserves identity and one canonical content change changes the hash.
- `ats-workspace::item::invalid_definition_content_cannot_receive_an_identity`: assert invalid
  content and schema tampering fail before identity.
- `ats-features::mod_plan::built_in_generation_contribution_covers_item_catalog`: assert the generation
  contribution covers exactly the Pack catalog IDs.
- Run `cargo test --workspace --all-targets`, workspace Clippy, DAG self-test/real gate, rustfmt and
  `git diff --check` after any catalog contract change.

#### 7. Wrong vs Correct

Wrong: three owners can drift.

```json
{"plan":{"itemTypes":["relic"]},"single":{"itemTypes":["relic","card"]},"ui":["relic","power"]}
```

Correct: declare once, then reference the selected catalog descriptor.

```json
{"itemTypes":[{"id":"relic","evidenceQueries":[{"symbols":["RelicModel"],"terms":[]}]}]}
```

## 5. Resource Workspace

`ats-workspace` stores immutable Resource versions with origin/provenance and explicit selection. A Feature references only `resourceId + selectedVersion`; stale or tampered bytes fail. User upload, Pack default, and AI media use the same contract. The registered HTTP Media Adapter supports Images/Chat protocols, cancellation, typed status mapping, bounded bytes, and request-hash provenance; health reports registration, not provider connectivity.

## 6. Prompt And Model

Pinned Feature Recipe + verified Pack contribution + bounded Truth Evidence + selected Resource identities + sanitized project context + one runtime custom-instruction slot + typed output contract produce `ModelRequestSnapshot` v1.

Runtime owns provider-neutral `ModelClient`; Adapters own HTTP. Long Mod/game Prompt strings are forbidden in handler/Shell/Adapter code. Protocol roles, schema/slot IDs, JSON contracts, escaping, truncation and redaction remain code contracts.

HTTP Adapters map the Runtime-owned output contract to native strict structured output:
OpenAI-compatible `response_format.json_schema` and Anthropic
`output_config.format.json_schema`. Provider rejection remains typed; prompt-only fallback and
permissive extraction are forbidden.

Recipe output JSON Schema exposes all expressible identifier, length and collection constraints
enforced by the typed Feature validator. The validator remains authoritative and its failures retain
their typed classification through composition and persisted Run state.

For `mod.generate.single`, the selected Pack item type compiles into a run-scoped bundle v2 schema.
`files` is a role-keyed object with exactly the Pack-owned generated-file roles required and no
additional properties. Recipe rendering, `ModelRequestSnapshot`, provider-native structured output,
typed decoding, and final role validation therefore share one contract; Pack roles cannot remain a
hidden post-provider constraint.

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
