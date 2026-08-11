# Stage 2 Contracts And Dependency DAG

> Executable current contract for the Stage 2 modular monolith, including durable composition Plan/Generate execution graphs.
>
> Use this file for cross-layer ownership, persisted schemas and recovery invariants. Failure serialization and redaction belong to [`error-handling.md`](./error-handling.md); implementation quality and required gates belong to [`quality-guidelines.md`](./quality-guidelines.md). Task status and candidate evidence do not belong in stable specs.

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

`ats-game-context` loads pinned Pack schema v4, resolves exact Feature slots, verifies immutable Truth Snapshot v2 and returns bounded Evidence. Pack v4 has one top-level `itemTypes` catalog containing validated localized names, restricted generic field descriptors, Pack-declared localization fields, typed reference slots, conditional Resource profiles, required locales and executable Evidence Queries. Optional top-level `compositionProfiles` declare bounded preset/custom parameter contracts. STS2 and synthetic fixtures use the same contracts. Missing/duplicate types or fields, invalid reference/profile constraints, missing contribution, Pack/Snapshot mismatch, unsafe path, unknown Primitive or hash mismatch fails before product work.

`mod.plan` result v2 owns descriptive `evidenceRequirements`. The Pack v4 item catalog owns
per-item `evidenceQueries` with explicit symbol/term fields. Capability readiness and Single
generation require every query group to match the active Truth Snapshot and never interpret Plan
prose as an index key. A missing current Snapshot blocks every declared type; a partially matching
Snapshot blocks only affected types with typed, query-indexed reasons.

Pack v4 replaces each item's unconditional `requiredResourceRoles` with `resourceProfiles`. A type
without a selector has at most one profile; a type with `resourceProfileField` must point to one
required choice field whose options exactly match the profile IDs. Definition-bound readiness
resolves only the selected profile. Plan may expose the bounded union for legacy leaf-item planning,
but Single/Composition execution always uses the exact definition selector.
The Plan union and the definition-selected exact role set are different contracts and must never be
compared for equality. `validate_plan_roles` validates the union against the Pack descriptor;
`load_resources` independently validates the selected definition profile and immutable bindings.
`pack.mod-plan-guidance` v3 owns only cross-item planning guidance;
`pack.mod-generate-single` v4 owns bounded common guidance, required per-item guidance, validation
Primitive and exact generated-file roles. Single exposes `commonGuidance` plus only the selected
`itemGuidance` to the Prompt. The Plan model-output contract excludes internal Resource role IDs;
`ModPlanService` attaches catalog values after typed model validation. The Single contribution
must cover exactly the catalog's item type IDs, preventing a declared-but-unexecutable type.

`ats-workspace` owns ItemDefinition schema v2: stable Item identity/type, canonical structured
field values, behavior intent, Pack-keyed localization field maps, exact Resource version bindings,
typed reference bindings and optional composition-profile provenance.
Validated Serde and deterministic ordered serialization produce the definition SHA-256 used by
future Run/Artifact provenance; Runtime remains unaware of game-specific types.

Definition-bound Plan execution receives the complete validated `StoredItemDefinition` through
Feature context while retaining the public `mod.plan` request schema. The Recipe exposes that
definition as authoritative context. After typed model decode, Features deterministically rebind
`itemId`, `itemType` and `behaviorIntent` from the definition before persisting the Plan. Plan prose
may add implementation/evidence/acceptance guidance but cannot override canonical fields,
localizations, Resource bindings or reference bindings. Single always resolves a conflict in favor
of the exact definition; free-text fact extraction or heuristic conflict validation is forbidden.

`ats-workspace` also owns CompositionDraft schema v2 and the repository ports for Draft CAS,
payload-hash `createOrMatch`, and atomic multi-definition saves. Drafts are review state, not ready
Items. `ats-adapters` persists them below `.ats/composition-drafts-v2`; v2 records optional exact
`sourceExecutionGraphId` plus `validatedContentDigest`, and the Adapter uses a recoverable pointer
journal when confirming more than one definition. Old v1 files are preserved and not rewritten.
`ats-features` owns closed-subgraph confirmation and `ResolvedItemGraph` v1; Adapters do not import
or reimplement graph rules.

Pack may contain declarations/templates/resources and registered Primitive IDs. It cannot contain arbitrary script, native plugin, provider credential, or complete workflow implementation.

### Scenario: Pack v4 Item Catalog, Definition Identity, And Readiness

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

The Pack payload is schema v4. A leaf type shape is:

```json
{
  "schemaVersion": 4,
  "id": "sts2",
  "displayName": "Slay the Spire 2",
  "itemTypes": [{
    "id": "relic",
    "displayNames": {"eng": "Relic", "zhs": "遗物"},
    "requiredLocales": ["eng", "zhs"],
    "localizationFields": [{
      "id": "name",
      "displayNames": {"eng": "Name"},
      "required": true,
      "multiline": false,
      "minLength": 1,
      "maxLength": 256
    }],
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
    "referenceSlots": [],
    "resourceProfiles": [{
      "id": "default",
      "displayNames": {"eng": "Default"},
      "requiredResourceRoles": ["relic.normal"]
    }]
  }],
  "contributions": []
}
```

Allowed field `value.kind` values are `text`, `integer`, `boolean`, `choice` and `string_list`,
with the bounded fields represented by `ItemFieldValueSpec`. No descriptor contains UI code.

Reference slots are data-only Pack contracts:

```text
identity binding = itemId + expectedItemType
pinned binding   = itemId + definitionHash + quantity
```

- Identity slots express affiliation such as Card -> owner Character. They validate target identity/type in a resolved composition but do not expand a version hash.
- Pinned slots express reproducible composition such as Character -> StartingDeck/StartingRelics. Their exact definitions expand the immutable closure and their edges must be acyclic.
- Slot kind, allowed target types, entry cardinality and quantity bounds are Pack-owned. Definition serde performs structural validation; `ItemDefinitionValidator` applies the Pack slot; `ResolvedItemGraph` later proves target existence/type/hash/closure.
- `pack.composition-plan-guidance` may add generic `referenceBindingRules` keyed by source Item type,
  slot and `bindings` or `total_quantity`. Multiple rules for one key aggregate their base/parameter
  contributions. Every planned node of the source type must match the exact result, duplicate targets
  in one slot are invalid, and every Draft node must be reachable from the declared root through
  pinned edges. Identity edges never make an otherwise disconnected Draft node reachable.
- Composition Plan resolves those Pack rules before model work and compiles a run-scoped output
  contract: exact total nodes, only nonzero profile Item types, Pack field/localization shapes and
  profile-specific reference cardinalities. Per-type counts, quantity sums and root closure remain
  typed Feature validation. Model violations may persist only the bounded
  `feature.composition-plan-failure-details` v1 reason/count/validated-ID payload.

`compositionProfiles[]` groups one composition/root Item type with `defaultProfile`,
`customBaseProfile`, a hard `maxNodes <= 128`, bounded numeric parameter descriptors, immutable
presets and cross-parameter constraints. `nodeWeight` plus `baseNodeCount` provides a generic
pre-model estimate. ItemDefinition v2 records the chosen preset or Custom base and the complete
resolved parameter map; Run/Artifact graph provenance is added by the composition Feature.

Capability response entries contain `descriptor`, `ready` and `blockers`. Blocker code is exactly
`truth.snapshot_unavailable` or `truth.evidence_missing`; the latter includes zero-based
`queryIndex`. The response also carries `gamePackId`, `gamePackSha256` and optional
`truthSnapshotId`, so clients must replace rather than merge results from another identity.

ItemDefinition schema v2 fields are `itemId`, `itemType`, `canonicalFields`, `behaviorIntent`,
`localizations`, `resourceBindings`, `referenceBindings` and optional `compositionProfile`.
`canonicalFields`, locale field maps, bindings and composition parameters
are ordered maps. The definition SHA-256 covers the complete validated wire object including
`schemaVersion`; it does not include repository timestamps or a mutable selected pointer.

#### 4. Validation & Error Matrix

| Input/state | Result | Work allowed |
| --- | --- | --- |
| Pack bytes do not match pinned SHA-256 | `GamePackLoadError::ContentHashMismatch` | none |
| `schemaVersion != 4` | `GamePackLoadError::UnsupportedSchema` | none |
| empty/duplicate/more than 64 `itemTypes` | `GamePackLoadError::InvalidItemTypes` | none |
| invalid locale/field/query/reference/resource-profile descriptor | `GamePackLoadError::InvalidItemType(ItemCatalogError::*)` | none |
| invalid preset/custom bounds, profile constraint, root type or >128 estimated nodes | `GamePackLoadError::InvalidCompositionProfile*` | none |
| duplicate reference target, binding/quantity mismatch or disconnected planned node | `model.output_invalid` or `composition.profile.count_mismatch` | no Draft/Item/project mutation |
| Single generation types differ from catalog IDs | `SingleGenerateError::InvalidPackContribution` | no model/IO |
| missing/invalid per-item generation guidance | `SingleGenerateError::InvalidPackContribution` | no model/IO |
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
- `ats-game-context::pack::pack_v4_validates_reference_resource_localization_and_composition_profiles`:
  assert selector/profile exact coverage, typed reference slots, default Standard/custom base,
  parameter bounds, cross-field constraints and <=128 node estimation.
- `ats-game-context::item::capability_catalog_is_pack_driven_and_truth_scoped`: assert no-Truth,
  matching and missing-query states plus exact `truth.evidence_missing` serialization. The complete
  catalog wire must expose tagged `text` fields as `minLength`/`maxLength` and `string_list` fields
  as `minItems`/`maxItems`/`itemMaxLength`, with no snake_case aliases in serialized output.
- `ats-workspace::item::definition_hash_is_stable_and_covers_canonical_content`: assert wire
  round-trip preserves identity and one canonical content change changes the hash.
- `ats-workspace::item::invalid_definition_content_cannot_receive_an_identity`: assert invalid
  content and schema tampering fail before identity.
- `ats-workspace::item::definition_v2_hash_covers_typed_references_and_composition_provenance`:
  assert exact camelCase wire, reference quantity/profile parameter hash coverage and v1 rejection.
- `ats-features::item_definition::ready_validation_resolves_profile_references_and_composition_parameters`:
  assert selected Resource profile, localization fields, Pack reference cardinality/quantity and
  preset parameter identity are authoritative before execution.
- `ats-features::mod_plan::built_in_generation_contribution_covers_item_catalog`: assert the generation
  contribution covers exactly the Pack catalog IDs.
- `agentthespire-desktop::stage2_single_mod::card_pack_truth_resources_prompt_and_artifact_form_one_vertical_contract`:
  assert Card guidance is selected without Relic leakage, missing Truth/resource stops before
  model work, and complete Card generation publishes five hash-verifiable files.
- `agentthespire-desktop::stage2_character_composition` with STS2 assembly/Godot environment:
  assert the Pack-declared 11-node Placeholder Prototype has three starter Card identities totaling
  ten cards, 23 model requests, real dotnet validation/publish, Godot PCK, ZIP, one recomputable
  composition Artifact, merged Character/Card/Ancients localization and zero staging residue.
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

### Scenario: Persist Item Versions And Gate Desktop Commands Before Run Creation

#### 1. Scope / Trigger

This contract applies whenever the desktop lists, loads or saves an ItemDefinition, and whenever
Plan, Single, Batch or Complex submission names an Item type. It keeps editable drafts versioned
without weakening Pack/Truth readiness or creating a rejected Run.

#### 2. Signatures And Layout

```rust
// crates/ats-workspace/src/item.rs
pub trait ItemRepository {
    fn save(&self, definition: &ItemDefinition) -> Result<StoredItemDefinition, Self::Error>;
    fn load_current(&self, item_id: &ItemId) -> Result<StoredItemDefinition, Self::Error>;
    fn load_version(&self, item_id: &ItemId, hash: &Sha256Digest)
        -> Result<StoredItemDefinition, Self::Error>;
    fn list_current(&self) -> Result<Vec<StoredItemDefinition>, Self::Error>;
}

// src-tauri/src/commands/stage2.rs
get_item_capabilities() -> ItemCapabilityCatalog
list_item_definitions() -> Vec<StoredItemDefinition>
get_item_definition(itemId, definitionHash?) -> StoredItemDefinition
save_item_definition(definition) -> StoredItemDefinition
```

```text
.ats/items-v2/<itemId>/
  current.json
  definitions/<definitionHash>.json
```

`ProjectSession` owns one `Arc<FileItemRepository>` for the open project. Commands must not create
independent per-call repositories because their mutexes would not serialize concurrent current
pointer updates.

#### 3. Contracts

| Operation | Required behavior |
| --- | --- |
| save new content | validate ItemDefinition and Pack Draft mode; write one immutable hash snapshot; atomically replace only `current.json` |
| save same content | reuse the exact snapshot and move the pointer idempotently |
| save existing itemId | itemType must equal every committed or crash-orphaned snapshot for that ID |
| load by hash | recompute and match the path hash; reject changed bytes, symlinks and path drift |
| list | return only validated current pointers, sorted by stable itemId |
| capability wire | use camelCase, including tagged-enum payload fields and blocker `queryIndex`; React runtime guards reject malformed shapes |
| Item save | selected type must be ready for the pinned Pack/current verified Truth before storage |
| Run submit | decode Plan/Single/Batch/Complex typed requests and check every requested type before `RunRecord::new` |

Saving uses `ItemDefinitionValidationMode::Draft`, so incomplete authoring snapshots can be kept.
`Ready` validation is reserved for definition-bound execution. Readiness and structural completeness
are separate facts and must not be collapsed.

#### 4. Validation And Error Matrix

| Condition | Boundary result | Side effect |
| --- | --- | --- |
| blocked/missing Truth for Item save | `truth.evidence_missing`, `item.save` | no snapshot/pointer write |
| blocked type in Plan/Single/Batch/Complex | `truth.evidence_missing`, `run.submit.readiness` | no RunRecord created |
| schema/type/Pack drift | `item.definition_invalid` | no write/Run |
| missing current/hash | `item.not_found` | none |
| IO or in-process repository lock failure | `item.storage_failed` | no fabricated success |
| existing itemId changes itemType | `ItemStoreError::TypeConflict` -> `item.definition_invalid` | old pointer/history unchanged |
| tampered snapshot/hash/path/symlink | typed invalid/storage mapping | no unverified definition returned |
| valid changed content | new definition hash and current pointer | old snapshot remains immutable |

#### 5. Good / Base / Bad Cases

- Good: edit a Pack-declared ready Item, save two hashes, load either hash, and list the second as
  current while the first remains unchanged.
- Base: save an incomplete Draft with valid fields and locale candidates; later Ready validation may
  still block generation without deleting the Draft.
- Bad: create a new `FileItemRepository` inside every command; concurrent saves bypass the intended
  mutex and can race type/pointer checks.
- Bad: write a definition snapshot, fail before the pointer, then allow the same itemId under another
  type because `current.json` is absent. Existing orphaned versions still reserve the type.
- Bad: call `RunRecord::new` before readiness and persist a Pending Run that can never execute.

#### 6. Tests Required

```powershell
cargo test -p ats-adapters item_store -- --nocapture
cargo test -p agentthespire-desktop --lib commands::stage2::tests -- --nocapture
cargo test --workspace --all-targets
npm run test:frontend
npx tsc -b --pretty false
```

Assertions must cover immutable/history/current behavior, same-hash idempotence, tamper/path/type
rejection, orphaned-snapshot type reservation, exact `queryIndex` serialization, all four
Item-bearing request shapes, and readiness rejection before Run creation.

#### 7. Wrong Vs Correct

Wrong - validate readiness only inside the asynchronous worker:

```rust
let run = RunRecord::new(submission.feature_id, submission.request);
session.submit(run, |run, ..| execute_and_discover_blocked_type(run)).await
```

Correct - reject before any Run identity or repository side effect:

```rust
let session = current_session(&active, "run.submit")?;
ensure_submission_ready(composition, &submission)?;
let run = RunRecord::new(submission.feature_id, submission.request);
session.submit(run, worker).await
```

### Scenario: Definition-Driven Batch And Complex Composition

#### 1. Scope / Trigger

This contract applies whenever the desktop submits, displays or retries Batch/Complex generation.
It prevents the Shell from authoring `PlanItem`, keeps every retry pinned to immutable Item input,
and makes child outcomes persistable even when every selected Item fails.

#### 2. Schemas

`feature.mod-generate-batch-request` v4 is:

```text
modId
items[] = artifactId + StoredItemDefinition
failFast
```

Batch deterministically converts `definition.behaviorIntent` to `ModPlanRequest.requirements`, runs
Plan, pins the typed result back to the same `itemId/itemType`, then invokes Single v3. Batch result
v2 records `total/processed/succeeded/failed` plus, per processed Item, `definitionHash`,
`planRunId`, optional `generationRunId`, terminal status, typed failure, Plan and Single result.

`feature.mod-generate-complex-request` v3 contains the exact Batch v4 request plus Package request.
Complex result v2 always contains the Batch result. Build/Package Run IDs and results are present
only when `processed == total && failed == 0`; otherwise delivery is skipped.

#### 3. Contracts

- Every Plan and Single attempt is a terminal persisted child `RunRecord`; no progress event or
  parent summary may replace the repository record.
- A completed Batch parent is `succeeded` even when Item results are failed. Product callers must
  inspect `failed/items`; cancellation and composition contract/infrastructure errors remain parent
  failures or cancellation.
- `failFast=true` stops after the first failed Item. Unprocessed request Items have no child Run and
  are identified by the request/result difference; no skipped Run may be fabricated.
- Retry submits only failed items copied from the previous parent Run request. Loading a newer
  current Item pointer is forbidden because it changes the pinned definition hash.
- Desktop preflight validates every definition, non-empty behavior intent, Pack/Truth readiness and
  selected Resource media before `RunRecord::new` or model work.
- Plan/Single execution remains serial because project writes, rollback and Artifact publication
  share one project transaction boundary.

#### 4. Required Tests

```powershell
cargo test -p agentthespire-desktop --test stage2_composition -- --nocapture
cargo test -p agentthespire-desktop --test stage2_single_mod -- --nocapture
npm run test:frontend
npx tsc -b --pretty false
```

Tests must cover Plan failure without a Single child, generation failure with both children,
fail-fast unprocessed inputs, Complex delivery skip, exact-hash retry, malformed payload rejection,
and a four-type Relic/Card/Potion/Power Batch with real compile and hash-verifiable Artifacts.

### Scenario: Persist And Resolve A Composition Draft

#### 1. Scope / Trigger

Use this contract when a set-level plan creates editable Item nodes, when the user confirms any
closed subset, or before a composition generation Run consumes a root definition. Draft persistence
must survive restart without creating pseudo-ready Items, and graph resolution must finish before
model or project mutation.

#### 2. Signatures

```rust
CompositionDraftRepository::{create, load, compare_and_set, list}

AtomicItemRepository::save_batch(
    request: &AtomicItemSaveRequest,
) -> Result<Vec<StoredItemDefinition>, AtomicItemSaveError<Self::Error>>

CompositionConfirmationService::confirm(pack, draft, selected_item_ids, item_repository)
    -> Result<CompositionConfirmation, CompositionConfirmationError>

ResolvedItemGraph::resolve(
    pack, truth, resource_contributions, item_repository, resource_repository,
    root, draft_ref,
) -> Result<ResolvedItemGraph, CompositionGraphError>
```

#### 3. Contracts

- CompositionDraft schema v1 stores `draftId`, monotonic `revision`, pinned Pack ID/hash, one root
  Item ID, exact profile source/parameters, 1-128 ItemDefinition nodes, per-node expected current
  hash, and creation/update timestamps. The root definition must carry the same profile provenance.
- Draft create requires revision 1. Update uses revision CAS, preserves Pack/root/creation identity,
  increments exactly once and never overwrites a concurrent edit.
- Confirmation validates every Draft node against the Pack, requires the selected subset to be
  closed, validates Ready-mode structure, computes immutable hashes, then submits one atomic batch.
  A selected pinned edge may target another selected node or an already persisted exact version;
  identity affiliation must resolve inside the resulting selected/pinned closure.
- Planning never authors Resource selection. Before confirmation, Shell review may replace only one
  node's `definition.resourceBindings` through `update_composition_draft(draft_id,
  expected_revision, nodes)`. The node's Item ID/type and `expectedCurrentDefinitionHash` remain
  unchanged, and the repository revision CAS remains authoritative.
- Atomic batch input contains 1-128 unique definitions and exactly one expected current value for
  every Item ID. All type and optimistic-current checks complete before any pointer changes.
  Immutable snapshots may remain as history after failure; current pointers change all-or-none.
- Filesystem confirmation writes a `prepared` pointer journal before replacements. Recovery rolls a
  prepared journal back to the expected pointers and rolls a `committed` journal forward to the new
  pointers, then removes the journal. This is recovery, not a second authority.
- ResolvedItemGraph expands only pinned edges. Identity edges do not load a version; after pinned
  closure resolution they must target an Item in that closure with the expected type. Exact target
  hash, Pack allowed type, Truth capability, locale/behavior, Resource selection/version and the
  128-node limit are checked before the graph is returned.
- Nodes and edges are sorted before serialization. Graph provenance binds schema, Pack ID/hash,
  Truth Snapshot ID, optional Draft ID/revision, root ID/hash, profile parameters, exact definition
  snapshots and both edge sets. `graphDigest` is SHA-256 of that canonical material.

#### 4. Validation & Error Matrix

| Condition | Stable result | Mutation |
| --- | --- | --- |
| invalid Draft wire/root/profile/node | `composition.draft.invalid` | none |
| stale Draft revision | Draft repository conflict | existing Draft retained |
| duplicate/unknown/non-closed confirmation selection | `composition.confirm.selection_invalid` or `composition.graph.item_missing` | no current pointer change |
| selected node missing a required, selected, exact-shape Resource binding | internal `NotReady`/`Readiness`; Shell `composition.confirm.not_ready`, action `replace_resource` | Draft retained; no current pointer change |
| one expected current changed | `composition.confirm.conflict` | no current pointer change |
| batch storage failure after one replacement | `composition.confirm.storage_failed` | all pointers rolled back; Draft retained |
| prepared journal after process interruption | recovery rollback | expected pointers restored |
| committed journal after process interruption | recovery roll-forward | new pointers restored |
| missing exact Item/storage failure/hash tamper/wrong type | `composition.graph.item_missing` / `storage_failed` / `hash_mismatch` / `type_mismatch` | no model/project mutation |
| incompatible versions or pinned cycle | `composition.graph.version_conflict` / `cycle` | no model/project mutation |
| Truth/locale/behavior/Resource not ready | `composition.graph.not_ready` | no model/project mutation |
| more than 128 resolved nodes | `composition.graph.node_limit` | no model/project mutation |

#### 5. Good / Base / Bad Cases

- **Good**: Character and selected Cards are confirmed together; Character pinned edges expand exact
  Card hashes, Card identity edges point back to the Character identity, and one stable graph digest
  is produced.
- **Base**: a selected subset references an already confirmed exact version. Confirmation succeeds
  only when that target exists and the combined subgraph is closed.
- **Bad**: one Draft pointer became current after the Draft was created. The entire batch returns a
  typed conflict; no other selected Item pointer moves.
- **Bad**: the process stops after the first pointer replacement. The next repository access reads
  the prepared journal and restores every previous pointer before serving data.

#### 6. Tests Required

```powershell
cargo test -p ats-workspace composition -- --nocapture
cargo test -p ats-adapters item_store -- --nocapture
cargo test -p ats-adapters composition_draft_store -- --nocapture
cargo test -p ats-features composition -- --nocapture
```

Tests must cover Draft wire/CAS, stale current, mid-commit rollback, prepared-journal crash recovery,
closed/partial confirmation, identity plus pinned resolution, missing/type/hash/readiness/cycle
failures and deterministic graph/confirmation digests.

### Scenario: Publish A Resolved Composition As One Unit

`composition.generate` is the eleventh catalog Feature. Request v1 pins the root
`StoredItemDefinition`, optional Draft revision, artifact/mod identity and Package request. The
Feature repeats `ResolvedItemGraph` resolution, then executes sorted nodes as proposed Plan/Single
children without calling Single's project/Artifact publication path.

```text
ResolvedItemGraph
  -> N x (Plan child + composition_staged Single child)
  -> bounded isolated project copy
  -> apply all proposals + one validation
  -> one isolated Build + composition_staged Package child
  -> one rollback-capable real-project write transaction
  -> one composition Artifact
  -> parent succeeded terminal
```

- Single result v2 and Package result v2 distinguish `published` from `composition_staged`; staged
  results contain no fabricated Artifact ref/hash.
- Build request v2 accepts an optional normalized output root. Pack build recipe steps may declare
  `isolatedOutputProperty`; the registered process Adapter permits only `ModsPath` and resolves it
  below the staged project.
- `.ats`, `.git`, `.godot`, `artifacts`, `delivery`, `dist` and `target` are excluded from the bounded
  project copy. Symlinks, more than 50,000 files or more than 2 GiB fail before model output is
  published.
- The final ZIP is streamed from the stage through the same project writer transaction as generated
  source. Artifact input paths reference that pending real-project state and are hash-verified by
  `ArtifactManifest` v3.
- Final project commit uses a same-directory transaction-directory rename as its decision point.
  Pre-decision rename failure rolls back complete backups before return; post-decision cleanup-only
  directories are retried on the next writer access.
- Validation, Build, Package, final-write, Artifact and cancellation failure retain completed child
  Runs but restore real-project files and remove owned stage/Artifact state before returning.
- The composition Artifact extension binds graph/root/Draft/profile provenance, node/file counts,
  every child Run ID and the Package report. No node receives an independent final Artifact.
- Process-stop recovery during final project transaction commit remains O6 scope; ordinary returned
  errors are not allowed to leave an advertised partial publication.

Required deterministic gate:

```powershell
cargo test -p agentthespire-desktop --test composition_generation -- --nocapture
```

It must prove successful whole-closure publication, validation rejection and package-commit failure,
including exact child terminal counts, recomputable composition manifest files and zero real-project,
Artifact or staging residue on failed paths.

## 5. Resource Workspace

`ats-workspace` stores ResourceAsset schema v2 with immutable candidates and explicit selection. A Feature references only `resourceId + selectedVersion`; a new upload, Pack default, AI response, or deterministic derived output has `selectedVersion=null` until an explicit select succeeds. Stale or tampered bytes fail. The registered HTTP Media Adapter supports Images/Chat protocols, cancellation, typed status mapping, bounded bytes, and request-hash provenance; health reports registration, not provider connectivity.

### Scenario: Prepare And Select A Deterministic PNG Resource Graph

#### 1. Scope / Trigger

This contract applies whenever a Pack adds or changes a Resource role, transform Primitive/version,
exact media dimensions, alpha requirement or generated-file target, and whenever Resource Prepare
ingests upload, AI or Pack-default bytes. It also applies before Single generation consumes a
selected Resource version.

#### 2. Signatures And Layout

```rust
// crates/ats-workspace/src/resource.rs
pub trait ResourceMediaProcessor {
    fn prepare_file(&self, source_path: &Path, declared_media_type: &str)
        -> Result<PreparedResourceMedia, Self::Error>;
    fn prepare_bytes(&self, bytes: Vec<u8>, declared_media_type: &str)
        -> Result<PreparedResourceMedia, Self::Error>;
    fn transform(&self, source: &PreparedResourceMedia, operation: &ResourceTransformOperation)
        -> Result<PreparedResourceMedia, Self::Error>;
}

pub trait ResourceRepository {
    fn ingest_batch(&self, requests: Vec<ResourceBytesIngestRequest>)
        -> Result<Vec<ResourceAsset>, Self::Error>;
    fn select(&self, resource_id: &ResourceId, version: &Sha256Digest)
        -> Result<ResourceAsset, Self::Error>;
    fn list(&self) -> Result<Vec<ResourceAsset>, Self::Error>;
    fn read_version_bytes(&self, resource_id: &ResourceId, version: &Sha256Digest)
        -> Result<Vec<u8>, Self::Error>;
}

// crates/ats-features/src/resource_prepare.rs
ResourcePrepareService::prepare_file(...)
ResourcePrepareService::prepare_default(..., resolve_asset, ...)
ResourcePrepareService::prepare_ai(...)
ResourcePrepareService::catalog(...)
ResourcePrepareService::list(...)
ResourcePrepareService::preview(...)
ResourcePrepareService::select(...)
```

```text
.ats/resources/<resourceId>/
  resource-manifest.json
  versions/<candidateSha256>/original.png
```

`ProjectSession` owns one `Arc<FileResourceRepository>` for the open project. Run composition and
future Resource commands reuse it; per-call repositories with independent mutexes are forbidden.

#### 3. Contracts

`feature.resource-prepare-request` and result are schema v2. The request is:

```json
{
  "logicalRole": "relic.master",
  "mediaType": "image/png",
  "source": {"kind": "user_upload"}
}
```

AI source adds `prompt` and optional `model`; it does not carry a user-authored file name because
the Pack role determines the stored candidate name. The result contains `candidates[]` with
`resourceId`, `logicalRole`, `candidateVersion`, optional `selectedVersion`, media type, width,
height, alpha fact and origin.

`pack.resource-specs` is schema v3. Every role declares exact dimensions, supported media types,
alpha requirement and either `master` or one direct `derived` source. A derived role references an
existing master plus a registered transform Primitive at version exactly `1`. Current operations
are deterministic nearest-neighbor `resize` and white-alpha `outline`; Pack payloads cannot supply
code. STS2 `relic.master` is 512x512 and produces `relic.normal` 128x128,
`relic.outline` 128x128 radius 4 and `relic.big` 256x256.

A master may declare `defaultAsset { id, sha256 }`. The caller sends only
`source: {"kind":"pack_default"}` and no `sourcePath`; the Shell resolves bytes from a reviewed
built-in asset registry bound to the exact loaded Pack ID/SHA-256 and asset ID. The Feature verifies
the declared asset SHA-256 before PNG decode or repository mutation. Derived roles cannot declare a
default asset. STS2 `character.identity_master` is a 512x512 embedded PNG and deterministically
produces five Branded Placeholder roles: 85x85 top-panel icon/outline, 132x195 select/locked icon,
and 49x64 map marker. BaseLib 3.3.8 maps these to `CustomIconTexturePath`, `CustomIconPath`,
`CustomCharacterSelectIconPath`, `CustomCharacterSelectLockedIconPath`, and `CustomMapMarkerPath`.

The PNG Adapter validates a regular non-symlink file, `image/png`, complete decode/CRC, maximum
64 MiB decoded RGBA and maximum 16384 per dimension. It records decoded width/height and whether
the source has an alpha channel. All probe and transform work completes before repository mutation.

A master request stages the master and every direct derived candidate as one batch. Every new
asset has `selectedVersion=null`. Derived provenance stores source role/version, Primitive/version,
canonical transform-parameters SHA-256 and pinned Pack ID/SHA-256. Candidate version is the output
byte SHA-256 and must be identical for repeated input and Pack identity. Explicit select atomically
updates only the manifest; Single generation checks the exact selected version and its Pack media
shape before reading bytes.

Desktop exposes typed `get_resource_catalog`, `list_resource_assets`, `get_resource_preview` and
`select_resource` commands. They all reuse the open `ProjectSession` repository. Preview returns a
bounded `data:image/png;base64,...` value and never a filesystem path. List and preview may expose
intrinsically valid historical candidates so the Shell can mark stale bindings; only select and
generation enforce the current Pack shape.

#### 4. Validation And Error Matrix

| Condition | Boundary result | Side effect |
| --- | --- | --- |
| malformed/oversized PNG, media mismatch, wrong dimensions or missing alpha channel | `resource.media_invalid` | no Resource final or selection change |
| unknown role/media type | `resource.unsupported` | none |
| wrong execution source mode | `resource.source_invalid` | none |
| Pack default asset ID unavailable for the exact pinned Pack | `resource.pack_asset_missing` | none |
| Pack default bytes do not match declared SHA-256 | `resource.pack_asset_invalid` | none |
| Pack graph, undeclared Primitive or transform version drift | `pack.contribution_invalid` | none |
| media Provider auth/rate/config/transport failure | stable `resource.media_*` family | no candidate |
| repository stage/publish/select failure | `resource.storage_failed` | staged batch rolled back; prior selection unchanged |
| valid candidate batch | result with all `selectedVersion=null` | immutable final candidate directories |
| explicit valid select | selected candidate result | one atomic manifest pointer update |

#### 5. Good / Base / Bad Cases

- Good: the pinned STS2 Character default resolves without a caller path and produces one master
  plus five unselected derived candidates; selecting and binding all five derived roles admits a
  Branded Placeholder Character Single/Composition run.
- Good: one valid 512x512 RGBA relic master produces four unselected candidates; repeated bytes
  produce the same four version hashes and exact provenance; selecting `relic.normal` admits Single.
- Base: upload a valid 128x128 `relic.normal` override; it creates one unselected original
  candidate and does not rerun the master graph.
- Bad: trust `.png` extension or request dimensions without decoding; malformed or RGB-only bytes
  could enter a role requiring alpha.
- Bad: publish each derived candidate independently without rollback; a late transform/store
  failure could leave a partial graph or replace an existing selection.
- Bad: construct a new filesystem repository in every command/Run; concurrent select and consume
  operations would bypass the intended project-scoped mutex.
- Bad: accept a caller file path for `pack_default`, skip the asset hash, or resolve by Pack ID only;
  any of these can present unreviewed bytes as a pinned Pack default.

#### 6. Tests Required

```powershell
cargo test -p ats-workspace resource::tests -- --nocapture
cargo test -p ats-adapters resource_media::tests -- --nocapture
cargo test -p ats-adapters resource_store::tests -- --nocapture
cargo test -p ats-features resource_prepare::tests -- --nocapture
cargo test -p ats-features mod_generate_single::tests -- --nocapture
cargo test -p agentthespire-desktop --lib composition::tests -- --nocapture
cargo test -p agentthespire-desktop --test stage2_character_composition -- --nocapture
```

Assertions must cover malformed/type-mismatched PNG, real RGB alpha detection, exact dimensions,
four/six-candidate output, null selection, deterministic output hashes, exact derived provenance,
Primitive/version drift, explicit selection, Single rejection before selection, batch rollback,
missing/unselected/stale/wrong-shape Character readiness before model work, exact Pack asset
ID/SHA resolution, unchanged prior selection, no staging residue and stable failure codes.

#### 7. Wrong Vs Correct

Wrong - trust declared metadata and select during ingest:

```rust
let asset = repository.ingest(bytes, request.media_type, request.width, request.height)?;
asset.select(asset.original_version())?;

// Also wrong: a caller-controlled path cannot represent a Pack default.
service.prepare_file(processor, repository, pack_default_request, caller_path, context)?;
```

Correct - probe/derive first, publish one candidate batch, then select explicitly:

```rust
let master = processor.prepare_file(path, "image/png")?;
let candidates = prepare_candidates(&master, verified_pack_specs)?;
let assets = repository.ingest_batch(candidates)?; // selectedVersion is null
let selected = repository.select(resource_id, candidate_version)?;

let candidates = service.prepare_default(
    processor,
    repository,
    pack_default_request,
    |pack, asset_id| built_in_game_pack_asset(pack, asset_id).ok(),
    context,
)?; // exact Pack ID/SHA + asset ID + declared asset SHA; still unselected
```

## 6. Prompt And Model

Pinned Feature Recipe + verified Pack contribution + exact StoredItemDefinition + bounded Truth Evidence + selected Resource identities + sanitized project context + one runtime custom-instruction slot + typed output contract produce `ModelRequestSnapshot` v1.

Runtime owns provider-neutral `ModelClient`; Adapters own HTTP. Long Mod/game Prompt strings are forbidden in handler/Shell/Adapter code. Protocol roles, schema/slot IDs, JSON contracts, escaping, truncation and redaction remain code contracts.

HTTP Adapters map the Runtime-owned output contract to provider-native structured output.
Anthropic always uses `output_config.format.json_schema`. OpenAI-compatible configuration defaults
to `llm.openai_response_format=json_schema`, which sends the exact schema with `strict=true`; an
operator may explicitly select `json_object` only for a proxy verified to honor JSON Object while
ignoring or rejecting native JSON Schema. The exact output contract remains in the Recipe Prompt
and Feature validation in both modes. Provider rejection remains typed; automatic format fallback,
prompt-only retry and permissive extraction are forbidden.

Recipe output JSON Schema exposes all expressible identifier, length and collection constraints
enforced by the typed Feature validator. The validator remains authoritative and its failures retain
their typed classification through composition and persisted Run state.

For `mod.generate.single`, the selected Pack item type compiles into a run-scoped bundle v2 schema.
`files` is a role-keyed object with exactly the Pack-owned generated-file roles required and no
additional properties. Normal file roles are bounded strings; roles declared with
`compositionMerge=json_object` are flat objects with string values and are deterministically
serialized by the Feature before checkpoint or project writes. Merge JSON is never double-encoded
inside a model-authored string. Recipe rendering, `ModelRequestSnapshot`, provider-native structured
output, typed decoding, and final role validation therefore share one contract; Pack roles and
merge shapes cannot remain hidden post-provider constraints.

`mod.generate.single` request schema v3 removes caller-authored `selectedResources` and carries the
exact `StoredItemDefinition`. The service validates its hash, Ready mode, Plan identity and all
role-keyed `resourceBindings` before model work. The Recipe owns one required `item.definition`
slot. Artifact extension schema v2 and dedicated provenance both record `definitionHash`.
The Recipe treats ItemDefinition as authoritative over Plan prose and permits up to 16,384 output
tokens for complete multi-file Items. Invalid or truncated output persists only
`feature.mod-generate-single-failure-details` v1 with one closed `reasonCode`; raw completion,
parser text, Provider body and model-authored role text are never persisted.

`mod.generate.batch` request schema v4 embeds exact StoredItemDefinition snapshots and derives each
Plan request from canonical behavior intent. `mod.generate.complex` request schema v3 embeds Batch
v4 unchanged and invokes Build/Package only after every Item outcome succeeds.

## 7. Feature Composition

The shared registry contains exactly 12 current Features, including `composition.plan`, `composition.retry-node` and `composition.generate`. Composition planning persists reviewable Draft state and never publishes Item pointers or project files. Draft construction and revision own deterministic bottom-up rebinding of every internal pinned definition hash, so a Resource, behavior or replacement edit advances all affected ancestor hashes in the same CAS revision without changing optimistic-current baselines. Targeted retry replaces exactly one logical Draft node, then reuses the full Plan graph validator and Draft revision CAS; it never follows a newer Item pointer or lets the model author Resource/current/hash state. Composition generation reuses Plan, Single proposal, Build and Package services but owns one whole-closure publication boundary. Single generation owns the validated model bundle -> rollback-capable project writes -> real validation -> immutable Artifact -> Run success order. Batch invokes Single child Runs. Complex invokes Plan, Batch/Single, Build and Package. No composition creates an alternative Prompt, Resource, file transaction, build or package implementation.

### Scenario: Resume Large Composition Planning From Durable Node Checkpoints

#### 1. Scope / Trigger

This contract applies when a Pack v2 composition blueprint expands a large or cross-item plan into
multiple model nodes. Small bounded Single requests keep their direct path. The public registry
remains exactly 12 Features; execution nodes are not Features or child Runs.

#### 2. Signatures

```rust
// ats-kernel
pub struct ExecutionGraphId(String);
pub struct ExecutionNodeId(String);

// ats-runtime
pub trait ExecutionGraphRepository: Send + Sync {
    fn create_claimed(
        &self,
        graph: &ExecutionGraphRecord,
        run_id: &RunId,
    ) -> Result<(), ExecutionGraphRepositoryError>;
    fn get(&self, id: &ExecutionGraphId)
        -> Result<ExecutionGraphRecord, ExecutionGraphRepositoryError>;
    fn compare_and_set(
        &self,
        expected_revision: u64,
        next: &ExecutionGraphRecord,
    ) -> Result<(), ExecutionGraphRepositoryError>;
    fn list(&self) -> Result<Vec<ExecutionGraphRecord>, ExecutionGraphRepositoryError>;
    fn recover_structure(&self, runs: &dyn RunRepository)
        -> Result<ExecutionGraphRecovery, ExecutionGraphRepositoryError>;
}

// ats-workspace
pub trait CompositionDraftRepository {
    fn create_or_match(
        &self,
        draft: &CompositionDraft,
        expected_payload_sha256: &Sha256Digest,
    ) -> Result<CreateOrMatch, Self::Error>;
}
```

`ExecutionGraphRecord` v1 contains `executionGraphId`, `ownerFeatureId`,
`requestSnapshotHash`, monotonic `revision`, graph `status`, `activeRunId`, `previousRunId`,
versioned+hashed `blueprint`, ordered nodes, optional immutable `commitIntent`, and optional
`finalResultRef`. Each node contains stable ID/role/dependencies, status, logical attempts,
request-snapshot hash, optional versioned+hashed normalized checkpoint, and optional safe failure.

#### 3. Contracts

- Start atomically creates and claims a graph for its candidate Run. Resume atomically claims the
  same graph with `expectedRevision`, a new Run ID and `previousRunId`. A losing claimant creates no
  Pending/Running Run.
- One graph has at most one active Run. A logical node attempt belongs to the public parent Run;
  Adapter HTTP retries remain internal transport attempts.
- Nodes execute in stable blueprint order with default model concurrency one. A node becomes ready
  only after every declared dependency succeeded.
- A succeeded model node has a checkpoint whose SHA-256 recomputes from the canonical versioned
  domain payload. Prompt, provider request/body, raw completion, credentials, stack traces and raw
  errors are forbidden in checkpoints.
- Decode, typed validation or exhausted transport failure pauses the graph. Retry/resume is an
  explicit user action that creates a new parent Run; Features do not add an automatic semantic
  retry loop or silently change model/response format.
- After all nodes succeed, Feature finalization binds local identities, pinned hashes and Resource
  provenance, then reuses the complete composition graph validator.
- `validatedContentDigest` covers immutable owner/request/blueprint identity, every ordered
  successful node identity/dependencies/checkpoint identity, and local finalization output. It
  excludes graph revision/status, Runs, attempts, timestamps, failure, commit metadata and final
  reference.
- `commitIntent` fixes Draft ID, complete canonical Draft payload, payload SHA-256,
  `validatedContentDigest`, and timestamps before publication. The only success order is
  `commit_prepared -> Draft createOrMatch -> graph succeeded -> Run succeeded`.
- `commit_prepared` is roll-forward-only. Draft absence creates the exact intent payload; an exact
  provenance+bytes match is idempotent; a mismatch becomes stable `commit_blocked` and
  `composition.commit.conflict`. It never overwrites a Draft or regenerates timestamps/IDs.
- Project open order is publication recovery, execution-graph structural recovery, Run
  reconciliation, session exposure, then Feature payload compatibility validation. An unsupported
  blueprint/checkpoint pauses only that graph and does not prevent the project from opening.
- Graceful close/switch/shutdown requests pause and drains active staged graphs before releasing the
  project lock. Explicit Cancel preserves graph, Run, Draft intent and all other evidence.

Pack `pack.composition-plan-guidance` v2 owns strict node groups, group dependencies, deterministic
count rules, optional suite-brief guidance, and group-targeted binding/quantity rules. Features
compile stable node/Item IDs and generic stages; Pack data cannot specify threads, HTTP retries,
paths, commands or Provider settings.

#### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| duplicate node/group, missing dependency, cycle, invalid count/binding target | typed preflight failure before HTTP |
| CAS/claim revision mismatch or active Run exists | typed conflict; no new Run |
| node response malformed, wrong typed shape or transport retries exhausted | checkpoint unchanged; graph paused; Run failed/interrupted |
| checkpoint hash/state mismatch or corrupt graph JSON | structural recovery failure for that graph; no guessed recovery |
| Run create fails after graph claim | CAS compensation to paused; project-open recovery handles a failed compensation |
| crash before `commit_prepared` | preserve succeeded checkpoints and resume remaining nodes |
| crash after `commit_prepared`, before Draft | create exact intent Draft and roll forward |
| matching Draft exists, graph not succeeded | match exact payload/provenance and roll forward |
| conflicting Draft ID exists | graph `commit_blocked`; never overwrite or retry model nodes |
| graph succeeded before old Run terminal write | old Run becomes interrupted; explicit resume creates an idempotent succeeded Run |

#### 5. Good / Base / Bad Cases

- Good: an eleven-node Character graph persists each validated node and resumes at node six after a
  Provider outage without repeating nodes one through five.
- Base: a bounded single-item generation continues to use one direct request and one Run.
- Bad: store raw completions as checkpoints or expose partial Items in the library.
- Bad: create the Run before winning the graph claim, downgrade `commit_prepared` to paused, or
  recompute a Draft during recovery.
- Bad: let React infer dependency readiness or decode versioned Feature checkpoint payloads.

#### 6. Tests Required

```powershell
cargo test -p ats-runtime execution_graph -- --nocapture
cargo test -p ats-adapters execution_graph_store -- --nocapture
cargo test -p ats-workspace composition -- --nocapture
cargo test -p ats-features staged_generation -- --nocapture
cargo test -p agentthespire-desktop --test stage2_composition -- --nocapture
npm run test:frontend -- --run
```

Assertions cover ID/schema validation, DAG/cycle checks, transition legality, checkpoint hashes,
CAS conflicts, one active claim, FIFO serial node execution, explicit retry as a new Run, crash at
each commit boundary, exact Draft match/conflict, recovery ordering, cancellation/drain behavior,
typed progress/actions, exact 12-Feature catalog and no partial Draft/Item publication.

#### 7. Wrong Vs Correct

Wrong - keep one large response and retry the whole graph:

```rust
let plan = model.complete(render_entire_graph())?;
drafts.create(decode_and_validate(plan)?)?;
```

Correct - persist only typed domain checkpoints and publish once:

```rust
let claimed = graphs.claim(expected_revision, new_run_id, previous_run_id)?;
for ready_node in claimed.ready_nodes() {
    let checkpoint = feature.generate_and_validate(ready_node)?;
    graphs.compare_and_set(claimed.revision(), claimed.with_checkpoint(checkpoint)?)?;
}
let intent = feature.prepare_commit(graphs.get(id)?)?;
drafts.create_or_match(intent.draft(), intent.draft_payload_sha256())?;
graphs.mark_succeeded(intent.final_ref())?;
```

### Scenario: Resume Whole-Closure Generation Without Repeating Successful Model Work

#### 1. Scope / Trigger

This contract applies to `composition.generate`. A resolved closure may contain dozens of Items and
requires one Plan plus one Single proposal per Item, so one invalid or truncated response must not
discard every earlier successful model result. Direct `mod.generate.single` remains a bounded
single-request path and does not create an execution graph.

#### 2. Signatures

```rust
pub struct CompositionGenerateRequest { // feature.composition-generate-request v2
    pub artifact_id: String,
    pub mod_id: String,
    pub root: StoredItemDefinition,
    pub draft: Option<CompositionDraftRef>,
    pub package: ProjectPackageRequest,
    pub execution: Option<CompositionGenerateExecutionRequest>,
}

SingleGenerateProposal::checkpoint() -> SingleGenerateProposalCheckpoint;
SingleGenerateService::restore_composition_proposal(
    resources, request, context, checkpoint,
) -> Result<SingleGenerateCompositionProposal, SingleGenerateError>;

CompositionGenerateService::prepare_staged_start(...)
    -> Result<StagedCompositionGenerateStart, CompositionGenerateError>;
CompositionGenerateService::prepare_staged_resume(...)
    -> Result<StagedCompositionGenerateStart, CompositionGenerateError>;
CompositionGenerateService::execute_staged(...)
    -> Result<StagedCompositionGenerateExecution, CompositionGenerateError>;
```

`resume_execution_graph(executionGraphId, expectedRevision)` dispatches by the persisted graph
`ownerFeatureId`; callers do not choose a resume Feature or author an internal execution identity.

#### 3. Contracts

The backend enriches an initial v2 request with `execution.kind=start`. Resume creates a new parent
Run and enriches the exact blueprint request with `kind=resume`, the graph ID, expected revision and
previous Run ID. The graph contains exactly two model nodes per resolved Item plus one local node:

```text
item.000.plan -> item.000.single -> item.001.plan -> item.001.single -> ...
                                                                    -> composition.finalize
```

- Dependencies impose stable FIFO-style Item order even before the shared HTTP queue is considered.
- A Plan checkpoint contains the normalized typed Plan and its terminal child Run.
- A Single checkpoint contains the validated `composition_staged` result, canonical generated role
  contents and safe definition/model/resource provenance. It excludes the Prompt, request messages,
  Provider body and raw completion envelope.
- Restore revalidates the exact definition hash, Pack-generated role set, selected immutable
  Resource versions, normalized bundle and project writes without calling `ModelClient`.
- Child Runs are persisted create-or-match by exact Run ID and bytes after checkpoint CAS. A crash
  between graph CAS and child persistence is repaired from the checkpoint; a different existing Run
  is a storage failure.
- `composition.finalize` reconstructs every proposal, enforces one validation Primitive, merges
  files and validates writes locally. Only then may the graph enter `commit_prepared` with a hashed
  publication intent.
- Build, Package, real-project transaction and the one composition Artifact execute only after all
  model nodes and finalize succeed. A finalization failure releases the `commit_prepared` claim for
  explicit resume and never reruns successful model nodes.
- Result and Artifact extension are v2 and include the execution graph ID. The public Feature
  catalog remains exactly 12 entries; execution nodes are not Features.
- A succeeded graph reconciliation decodes the final result, succeeds a new parent Run and performs
  zero model, validation, Build, Package, project-write or Artifact work.

Runtime `ExecutionCommitIntent` accepts exactly one legacy Draft intent or one generic publication
intent. Mixed forms, unsafe target IDs or payload-hash mismatch are invalid graph records. Existing
Draft-intent graph JSON remains readable without rewriting historical evidence.

#### 4. Validation & Error Matrix

| Condition | Graph / Run result | Work retained |
| --- | --- | --- |
| Plan or Single typed output invalid/truncated | current node Pending with safe failure; graph paused; parent failed | every earlier succeeded checkpoint and child Run |
| explicit resume revision/claim conflict | `composition.execution.conflict`; no new Running Run | unchanged graph |
| checkpoint schema/hash/domain mismatch | `composition.execution.invalid` | no guessed output or model fallback |
| child Run create conflicts with different bytes | `run.storage_failed` | checkpoint remains authoritative; no overwrite |
| local merge/Primitive/write validation fails | finalize node fails and graph pauses | all Plan/Single checkpoints |
| Build/Package/validation/publication fails after prepare | claim is released while commit remains roll-forward | all model checkpoints and publication intent |
| crash with an active model node | structural recovery interrupts that attempt and pauses graph | all earlier succeeded nodes |
| graph already succeeded but parent is not authoritative | new reconciliation Run succeeds from final result | zero model/publication work |

#### 5. Good / Base / Bad Cases

- Good: Item zero Plan succeeds, Item zero Single returns invalid JSON, and resume requests only that
  Single followed by later Items; a repository re-instantiation proves restart recovery.
- Base: a two-Item closure completes four model nodes, one local finalize, one validation, one Build,
  one Package, one project transaction and one composition Artifact.
- Bad: restart `composition.generate` from the root request after one node fails, store a complete
  Prompt/request snapshot in a checkpoint, or expose a partially generated project/Artifact.
- Bad: add automatic Feature-level semantic retry or model/format fallback; resume is explicit and
  Adapter transport retry remains the only bounded automatic retry layer.

#### 6. Tests Required

```powershell
cargo test -p ats-runtime execution_graph -- --nocapture
cargo test -p agentthespire-desktop --test composition_generation -- --nocapture
cargo test -p agentthespire-desktop --test stage2_composition -- --nocapture
npm run test:frontend
npx tsc -b --pretty false
```

Assertions must cover Plan success plus Single invalid pause, repository re-instantiation, resume
request count excluding successful nodes, stable serial order, exact child Run create-or-match,
local finalize, one final publication path, succeeded reconciliation with zero model requests,
claim CAS, Pause/Cancel and no staging/transaction residue.

#### 7. Wrong Vs Correct

Wrong - repeat the entire closure after one semantic failure:

```rust
for definition in resolved.nodes {
    plans.push(plan_model(definition).await?);
    proposals.push(single_model(definition).await?);
} // any error discards every previous result
```

Correct - persist validated domain checkpoints and restore proposals locally:

```rust
if node.status != Succeeded {
    let proposal = single.propose(...).await?;
    graph.complete_node(node.id, proposal.checkpoint())?;
}
let proposal = single.restore_composition_proposal(
    resources, &request, &context, decode_checkpoint(node)?,
)?;
```

## 8. Shell Cutover

- Tauri product Runs execute only through `Stage2Composition` and `ProjectSession`.
- React uses v3 DTO guards and persisted Run polling.
- Web exposes shared health/catalog/static SPA only.
- CLI `features` emits the shared registry.
- Raw LLM, Prompt preview, old planning/codegen APIs, v2 transport and legacy handler submission are removed.

## 9. Project Session

One session owns the OS lock, v3 repository, cancellation tokens, and task handles. Submit and closing are serialized so no untracked Pending Run exists. Close/switch/shutdown cancels, drains and then releases the lock; timeout retains closing state and lock. Reopen first recovers `.ats/transactions`: prepared records roll back old/new targets and run-created directories, while a same-directory `.committed-*` decision preserves published files and only removes journal state. Corrupt, duplicate, symlink or escaping records fail open. Run reconciliation happens only after project publication recovery and before exposure.

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
