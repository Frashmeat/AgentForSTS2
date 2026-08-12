# Backend Quality Guidelines

> Executable Stage 2 contracts for the modular monolith. Historical `ats-core` v1/v2 contracts are not active production guidance.
>
> This file defines implementation discipline, scenario-level checks and required gates. Authoritative persisted contracts live in [`stage2-contracts.md`](./stage2-contracts.md), while failure/redaction rules live in [`error-handling.md`](./error-handling.md). Candidate results and task completion state are evidence, not stable quality rules.

## 1. Dependency Direction

The production DAG is:

```text
ats-kernel
  <- ats-runtime / ats-game-context / ats-workspace
  <- ats-features
ats-runtime / ats-game-context / ats-workspace
  <- ats-adapters (implements ports; never depends on Features)
Shell composition roots -> all required layers
```

Foundation, Game Context, Workspace, and Adapters must not depend on `ats-features`. No Stage 2 crate may depend on deleted `ats-core`. Enforce with Cargo metadata, not review convention alone.

```powershell
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
```

## 2. Feature Contract

Every product capability has one `FeatureId`, request schema, result schema, validator, and registry entry. Current catalog contains exactly 12 Features: project create, single-item plan, composition plan/targeted retry/generate, resource prepare, single/batch/complex generation, log analyze, build, and package.

Adding a Feature must not add a Runtime `RunKind`, center result union, Shell-specific implementation, or duplicate Prompt pipeline. Batch v4 owns definition-driven Plan -> Single child composition; Complex v3 reuses that exact Batch request/result and adds Build/Package only after every Item succeeds.

## 3. RunRecord v3

- Persist schema v3 with `featureId` and versioned request/result.
- `RunRepository::create` creates Pending; persistence uses expected-status CAS.
- One timeline and one terminal event are required.
- Existing old records are preserved and never silently rewritten as v3.
- `ProjectSession` owns one repository, all cancellation tokens/task handles, and the project OS lock.
- Reconciliation converts interrupted Pending/Running records to a typed failed terminal before exposing the session.

## 4. ArtifactManifest v3

- Artifact infrastructure accepts generic versioned contexts/provenance/extension; it never imports Feature or Game Pack types.
- Every source is a regular non-symlink file under project root.
- Every snapshot path is normalized and below `files/`; every byte length and SHA-256 is recomputable.
- Publish writes a complete same-directory `.staging-*`, then atomically renames to immutable final.
- Only classified transient Windows rename conflicts receive bounded retry. Deterministic failure is not retried.
- Failure cleans this run's staging and restores project files; it does not delete prior artifacts or evidence.

## 5. Pack, Truth, And Resource

- Pack v4 bytes/schema/hash are validated before registration.
- A Feature declares required Contribution slots; missing/duplicate/unknown slots fail before work.
- Pack data may reference only registered Primitive IDs and cannot execute arbitrary script/native code.
- Truth Snapshot sources/indexes and current pointer are content-addressed and verified for the active Pack.
- Evidence is bounded and traceable; missing/invalid current Truth stops dependent Features.
- Plan v2 stores descriptive `evidenceRequirements`; executable `symbols`/`terms` queries belong to
  each item type in the Pack v4 top-level catalog. Every query group must match current verified
  Truth before model work, and final Evidence is bounded and deduplicated.
- Required Resource role IDs are Pack-owned execution facts in the same item catalog. The Plan
  model output cannot author them; the Feature deterministically attaches them to Plan result v2.
  Plan and Single must not maintain parallel item type/resource/Truth declarations.
- Item capability readiness is computed from the pinned Pack plus current verified Truth before
  model work. Undeclared types are absent; declared types with missing Truth are disabled with
  typed blockers and do not create a Run.
- ItemDefinition schema v2 is validated on construction/deserialization and hashed from ordered
  canonical content. It carries Pack-keyed localization fields, exact Resource bindings, typed
  identity/pinned references and optional composition-profile provenance. Run/Artifact consumers
  bind the exact hash; they never reconstruct locked fields from prose or mutate an older snapshot.
- Definition-bound Plan passes the complete validated StoredItemDefinition as authoritative Feature
  context without changing the public Plan request wire. The Recipe may ask for implementation,
  evidence and acceptance guidance, but Features rebind Plan identity/type/behavior from the
  definition and Single resolves every prose conflict in favor of that definition. Do not compare
  or heuristically parse Plan prose to reconstruct canonical/localization/Resource/reference facts.
- Pack v4 `referenceSlots` own kind/target/cardinality/quantity rules. Identity edges never expand
  a version hash; pinned edges expand exact definitions and must form an acyclic closure.
- Composition guidance `referenceBindingRules` generically constrain per-source slot binding count
  or pinned total quantity. Duplicate targets and Draft nodes disconnected from the root pinned
  closure are rejected before definitions are enriched or persisted; identity edges do not satisfy
  closure reachability.
- A binding rule whose resolved target-group union is empty authors no `referenceBindings` entry.
  It must not persist an empty slot array because ItemDefinition v2 rejects empty binding lists;
  zero-count optional groups remain represented by the selected composition profile and Blueprint.
- Pack v4 `resourceProfiles` own conditional required roles. The selector is a required canonical
  choice whose options exactly cover profile IDs; Single resolves only the selected profile.
- `ModPlan.requiredResourceRoles` is the bounded union across a type's Pack profiles, while
  definition-bound Single/Composition readiness is the exact selected-profile set. Do not compare
  these sets for equality: validate the Plan union against the descriptor, then validate selected
  Resource bindings independently against the definition selector. Cover an empty-role Placeholder
  and a five-role Branded Placeholder in the same regression.
- Pack v4 `compositionProfiles` own Standard/Prototype presets, Custom bounds, cross-parameter
  constraints and the hard <=128-node estimate. Feature/UI interpret this generic schema and do
  not embed STS2 composition counts.
- CompositionDraft v2 is persisted separately from ItemDefinition with revision CAS, optional
  execution-graph provenance and payload-hash `createOrMatch`. Atomic confirmation validates a
  closed selected subgraph and updates every affected current pointer through one recoverable
  prepared/committed journal; failure never exposes a partial pointer set. Old v1 files remain
  preserved and are not rewritten.
- Draft construction and every revision deterministically recompute internal pinned definition
  hashes bottom-up across the complete Draft DAG. Resource, behavior or replacement edits may
  change only one submitted node, but every affected ancestor hash is code-owned and advances in
  the same revision while optimistic-current baselines and staged provenance remain unchanged.
- `composition.plan` owns pinned whole-plan, Suite Brief and single-node Recipes plus one
  `pack.composition-plan-guidance` v2 contribution. The desktop product path compiles a deterministic
  Blueprint and asks the model only for a bounded brief and one node's normalized content at a time.
  The Feature validates the selected Pack profile, queries bounded Truth, binds references locally,
  computes pinned definition hashes, attaches exact profile/current/graph provenance and persists
  only one complete CompositionDraft. The model cannot author references, selected Resources,
  expected-current hashes or Item current pointers.
- Each staged model response has a strict run-scoped output contract for its one role. Complete
  per-type counts, quantity sums, target identity, pinned closure and definition hashes remain local
  typed validation after checkpoints are assembled. Model violations persist bounded versioned
  details with stable reason codes and optional expected/actual counts or validated Item/slot IDs;
  raw model output is forbidden from checkpoints and persisted failures.
- `composition.retry-node` binds one Draft ID, expected revision and target Item ID. Its model output
  is exactly one logical node with the same identity/type. The Feature reuses composition Plan
  structure/count/closure rules, preserves every code-owned Resource binding and expected-current
  baseline, recomputes the complete pinned graph and persists one Draft CAS revision. A retry Pack
  contribution adds only retry semantics; it must not duplicate node/reference quantity rules.
- ResolvedItemGraph v1 expands exact pinned edges, validates identity edges against the resolved
  closure, applies Pack/Truth/locale/Resource readiness and hashes sorted nodes, edges and pinned
  Pack/Truth/Draft/profile provenance. It completes before model or project mutation.
- Single request schema v3 carries one `StoredItemDefinition`; Batch request v4 carries pinned
  definitions and derives Plan requirements from canonical behavior intent. It persists Plan and
  Single child Runs and returns per-item result v2 outcomes even if every Item fails. Complex v3
  embeds the exact Batch request and omits Build/Package results unless all Items succeed.
  `selectedResources` and caller-authored Batch Plan objects are forbidden at these boundaries.
  Failed retry must reuse the original parent request definition snapshot, not a newer pointer.
- ResourceAsset schema v2 keeps new upload, Pack default, AI and deterministic derived outputs as
  immutable candidates with `selectedVersion=null`; only an explicit select may move the pointer.
  Master derivation uses one staged batch, and a failure preserves every prior selection.
- `pack.resource-specs` v3 owns exact media dimensions/alpha, direct master/derived edges,
  transform Primitive/version and optional master-only `defaultAsset {id, sha256}`. A Pack default
  request carries no caller path; the Shell resolves reviewed bytes by exact Pack ID/SHA plus asset
  ID and the Feature verifies the declared asset hash before PNG decode/storage. Derived provenance
  binds source version, transform parameters and pinned Pack identity. `ProjectSession` owns the
  sole filesystem Resource repository for all Runs and commands.
- Media Adapter registration, provider configuration/connectivity, and generated-media quality are separate facts. Health reports only registration; failures remain typed and generated bytes are bounded before ingest.
- `pack.mod-generate-single` v4 separates bounded top-level common guidance from required
  `itemTypes[].guidance`; Single serializes only the selected type as `itemGuidance`. Generation
  entries must cover exactly the top-level catalog IDs and declare exact file roles. Type-specific
  rules must never be appended to common guidance or selected by a code branch.
- A generated file path may repeat across composition nodes only when every matching Pack file role
  declares `compositionMerge=json_object`. The merger accepts only flat string-valued JSON objects,
  rejects duplicate keys deterministically, and writes one sorted object before staging. STS2 uses
  this for Card/Relic/Character tables and keeps Character's four Architect lines in separate
  `ancients.json` roles for both locales.
- ExecutionGraphRecord v2 directly replaces v1 and persists only the current normalized checkpoint,
  attempt count/active attempt, repair round and latest diagnostic fingerprint/checkpoint hash. The
  graph repository reads only `.ats/execution-graphs-v2`; no v1 fallback, migration or repair archive
  is allowed.
- Composition Generate request v3 fixes `until_passed` or `max_rounds(1..20)` before graph creation.
  Registered validation runs before commit intent. A repair round may replace exactly one uniquely
  owned Single checkpoint using complete role contents, then must rebuild finalize output and rerun
  the complete validator suite. `commit_prepared` is unreachable before validation succeeds.
- Project-local `local.props` synchronizes only `Sts2AssemblyPath` and `GodotPath` from Settings,
  preserving `ModsPath` and unknown XML. Create/Open synchronize before session exposure;
  Generate/Build/Package synchronize before work. Missing/non-file paths fail locally before model
  or tool execution, and absolute values do not enter Run/Artifact/IPC.
- `ProjectFolder::create_configured(parent, name, game_id, template, local_build_paths)` owns the
  Create transaction: template files, metadata, version, managed `local.props` and the lock must all
  succeed before the folder is returned. Any failure removes only the exact root newly created by
  that call. Open/sync of an existing project never uses this rollback and must not delete existing
  content. Tests must cover invalid template `local.props` -> `ProjectError::LocalConfig` -> absent
  new root, plus preservation of unmanaged properties on an existing file.

## 6. Prompt And Model Request

```text
Feature Recipe + Pack Contribution + Item Definition + Truth Evidence + Selected Resources
+ Project Context + Runtime Custom Instructions + Typed Output Contract
= ModelRequestSnapshot
```

Feature Recipe owns cross-game task language. Pack v4 generation contributions own common and
selected-item game guidance separately. Truth owns current facts. Workspace owns selected
resources. Settings own `llm.custom_prompt`. Code owns protocol/safety/schema only.

Recipe and Pack resources are pinned by SHA-256. Slot resolution is exact and deterministic. Model requests must be replay-auditable without persisting secrets or provider bodies.

### Scenario: Plan, Review, And Confirm A Composition Draft

#### 1. Scope / Trigger

This contract applies when a Pack declares one or more `compositionProfiles` and a caller uses
`composition.plan` or the Draft IPC commands. Planning creates review state only; Item current
pointers and project files must remain unchanged until explicit confirmation.

#### 2. Signatures

```rust
// crates/ats-features/src/composition_plan.rs
pub struct CompositionPlanRequest {
    pub draft_id: CompositionDraftId,
    pub composition_id: CompositionId,
    pub concept: String,
    pub source: ItemCompositionSource,
    pub parameters: BTreeMap<CompositionParameterId, u32>,
}

// crates/ats-workspace/src/composition.rs
fn compare_and_set(expected_revision: u64, next: &CompositionDraft) -> Result<(), Error>;
fn delete(draft_id: &CompositionDraftId, expected_revision: u64) -> Result<(), Error>;

// src-tauri/src/commands/stage2.rs
list_composition_drafts() -> Vec<CompositionDraft>;
get_composition_draft(draft_id) -> CompositionDraft;
update_composition_draft(draft_id, expected_revision, nodes) -> CompositionDraft;
delete_composition_draft(draft_id, expected_revision) -> ();
confirm_composition_draft(draft_id, expected_revision, selected_item_ids)
    -> CompositionConfirmation;
```

#### 3. Contracts

| Boundary | Required behavior |
| --- | --- |
| Pack | `pack.composition-plan-guidance` v2 covers exactly every `compositionProfiles[].id` and declares stable node groups, group dependencies, count rules, group-targeted binding/quantity rules and optional Suite Brief guidance; resolved group counts still match the selected profile and <=128-node limit |
| Request | Preset parameters exactly match the selected immutable preset; Custom names a Pack preset base and passes all Pack bounds, constraints and <=128-node estimate |
| Model request/output | Feature compiles one bounded Suite Brief schema and one strict single-Item schema per Blueprint node. Node output contains only normalized domain content; references, Resources, expected-current hashes, definition hashes, current pointers and project files remain local/code-owned |
| Feature enrichment | Queries bounded Truth, persists hashed normalized checkpoints, binds identity/pinned references locally, reuses complete count/root/type/reference/closure validation, computes pinned hashes bottom-up and attaches root profile/current/graph provenance |
| Draft repository | Creates `.ats/composition-drafts-v2/<draftId>.json` only after graph validation through payload-hash `createOrMatch`; update/delete require exact revision CAS; ProjectSession owns the sole repository instance |
| Confirmation | Accepts only a non-empty pinned-closed selection, repeats Pack Draft/Ready/graph checks and performs one recoverable atomic Item pointer transaction |
| React | Receives IPC as `unknown`, validates CompositionDraft/Confirmation guards, renders Pack metadata only, prepares and binds each Plan-authored node's required Resources through the shared Workbench plus Draft revision-CAS, and never branches on Character or a game ID |

`composition.plan` result is `draftId + revision + rootItemId + nodeCount + modelRequestSha256`
plus optional `executionGraphId + validatedContentDigest` provenance for staged execution.
The complete graph remains authoritative in the Draft repository rather than being duplicated in the
Run result.

`composition.profile.count_mismatch` and `model.output_invalid` may carry
`feature.composition-plan-failure-details` v1. Its payload is limited to a stable `reasonCode`,
optional expected/actual counts and validated Item/type/slot identifiers. Provider bodies, Prompt
text, parse messages and paths are never persisted. Composition Studio polls and displays the real
terminal Plan Run; it must not silently discard a failed terminal result.

#### 4. Validation & Error Matrix

| Failure | Stable result | Mutation |
| --- | --- | --- |
| Unknown composition or invalid preset/custom values | `composition.profile.*` | no model call, Draft or Item mutation |
| Missing/stale Truth or invalid Pack contribution | `truth.*` / `pack.contribution_invalid` | no model call when preflight can decide; no Draft/Item mutation |
| Truncated/malformed/count-mismatched output or pinned cycle | `model.output_*` / `composition.profile.count_mismatch` / `composition.graph.cycle` | no Draft/Item mutation |
| Existing Draft ID or stale update/delete revision | `composition.draft.conflict` | existing Draft preserved |
| Draft filesystem failure | `composition.draft.storage_failed` | no fabricated success; owned temporary file cleaned on repository access |
| Invalid/open confirmation selection | `composition.draft.invalid` or typed `composition.confirm.*` internally | no Item current pointer changes |
| Draft node fails Ready Resource validation | `composition.confirm.not_ready`, `replace_resource`, non-retryable | Draft remains editable; no Item current pointer changes |
| Atomic confirmation conflict/storage failure | `composition.draft.conflict` / `composition.draft.storage_failed` at Shell | all current pointers rolled back or recovery journal retained |
| Success | succeeded Plan Run or `CompositionConfirmation` | planning writes one Draft; confirmation writes only the selected closed definition set |

#### 5. Good / Base / Bad Cases

- Good: Pack Standard is selected by default, model returns the exact estimated node count, Feature
  computes child hashes and persists revision 1; a closed subset confirms atomically.
- Base: Pack has empty `compositionProfiles` and an empty composition contribution catalog; registry
  and UI remain valid, while the Studio displays unavailable and no Plan can be submitted.
- Bad: React invents Standard counts or a `character` branch; Pack expansion would require code and
  violates the generic boundary.
- Bad: model authors a `definitionHash` or `expectedCurrentDefinitionHash`; this would let untrusted
  output bypass deterministic provenance.
- Bad: update omits `expectedRevision`, or confirmation saves nodes individually; concurrent edits or
  failures could expose lost updates/partial current pointers.

#### 6. Tests Required

```powershell
cargo test -p ats-features composition_plan -- --nocapture
cargo test -p ats-workspace composition -- --nocapture
cargo test -p ats-adapters composition_draft -- --nocapture
cargo test --workspace --all-targets
npm run test:frontend
npx tsc -b --pretty false
```

Assertions must cover: Pack/profile/Truth binding in the request snapshot; exact run-scoped total,
nonzero Item variants and Pack-owned field/locale/reference schema; the <=32 KiB compact contract;
safe versioned failure details without model/Prompt/Provider text; deterministic pinned hash
enrichment; Draft-only persistence; revision CAS/delete; closed partial confirmation; malformed IPC
guards; persisted failed Plan Run display; Pack-default Standard; Custom bounds; deterministic
filtering/pagination. O5 adds the first real STS2 Character model/compile path; O8 owns installed
real-game acceptance. Frontend model tests also prove that a Draft resource edit replaces only the
selected definition while preserving every node's optimistic-current baseline; the Tauri mapping
test fixes the not-ready code, stage, action and retryability.

#### 7. Wrong vs Correct

Wrong: trust model-authored hashes and publish definitions during Plan.

```text
model nodes + model definitionHash -> save each Item current pointer
```

Correct: enrich deterministic provenance, persist review state, then confirm explicitly.

```text
Pack profile + Truth + logical model nodes
  -> Feature validates/counts/computes pinned hashes
  -> CompositionDraft revision 1
  -> guarded review/update
  -> closed selection + atomic confirmation
```

### Scenario: Retry One Composition Draft Node

#### 1. Scope / Signatures

`composition.retry-node` revises review state only. It never confirms Items or writes project files.

```rust
// crates/ats-features/src/composition_plan.rs
pub struct CompositionRetryNodeRequest {
    pub draft_id: CompositionDraftId,
    pub expected_revision: u64,
    pub item_id: ItemId,
    pub instructions: String,
}

pub struct CompositionRetryNodeResult {
    pub draft_id: CompositionDraftId,
    pub revision: u64,
    pub item_id: ItemId,
    pub definition_hash: Sha256Digest,
    pub model_request_sha256: Sha256Digest,
}
```

React sends exact camelCase `draftId`, `expectedRevision`, `itemId`, `instructions` through the
generic `submit_feature` command and waits for the persisted terminal Run.

#### 2. Contracts And Errors

| Boundary | Required behavior |
| --- | --- |
| Pack | Reuse verified `composition.plan.guidance` structure/count rules; `composition.retry-node.guidance` adds retry semantics only |
| Prompt | Serialize the logical Draft graph without Resource bindings, expected-current values or definition hashes; query Truth for the target type |
| Model output | Exactly one node whose Item ID/type match the target; no whole-plan envelope |
| Enrichment | Restore every existing Resource binding/current baseline, replace the target, rerun full count/reference/root-closure/cycle validation and recompute pinned hashes |
| Persistence | `CompositionDraftRepository::compare_and_set(expected_revision, revised)` is the sole mutation |

| Failure | Stable result | Mutation |
| --- | --- | --- |
| missing target/Draft | `composition.draft.*` | no model call when known; Draft unchanged |
| stale revision | `composition.draft.conflict` | no model call; Draft unchanged |
| changed target identity/type or malformed node | `model.output_invalid` | Draft unchanged |
| invalid count/reference/closure/cycle after replacement | `composition.profile.count_mismatch`, `model.output_invalid` or `composition.graph.cycle` | Draft unchanged |
| CAS/storage failure | `composition.draft.conflict` / `composition.draft.storage_failed` | prior revision remains authoritative |
| success | succeeded Run | one Draft revision; Item pointers/project files unchanged |

Good: retry a child, advance revision once and update every ancestor pinned hash. Base: retry the
root with unchanged references. Bad: use a whole-plan response, follow a newer Item current pointer,
or let model output replace Resource bindings.

Required tests:

```powershell
cargo test -p ats-features composition_plan -- --nocapture
cargo test -p agentthespire-desktop --lib commands::stage2
npm run test:frontend
```

Assertions cover one model call, identity/type preservation, revision CAS, root hash recomputation,
absence of code-owned fields from the prompt, stale retry before a second model call and frontend
request trimming.

### Scenario: Generate And Publish A Whole Composition Closure

#### 1. Scope / Trigger

This contract applies when `composition.generate` consumes one confirmed composition root. The
complete pinned closure is one publication unit; a node may produce successful Plan/Generate child
evidence without publishing its files independently.

#### 2. Signatures And Schemas

```rust
pub struct CompositionGenerateRequest {
    pub artifact_id: String,
    pub mod_id: String,
    pub root: StoredItemDefinition,
    pub draft: Option<CompositionDraftRef>,
    pub package: ProjectPackageRequest,
    pub execution: Option<CompositionGenerateExecutionRequest>,
}

SingleGenerateService::propose(...) -> SingleGenerateProposal
SingleGenerateService::restore_composition_proposal(...) -> SingleGenerateCompositionProposal
ProjectPackageService::prepare(...) -> PreparedProjectPackage
ProjectStager::stage(ProjectStageRequest) -> Box<dyn PendingProjectStage>
```

The parent request/result and Artifact extension use schema v2. The backend authors `execution`
only after it wins a start/resume graph claim. `SingleGenerateResult` and
`ProjectPackageResult` use schema v2 and an exact `publication` discriminator:

```text
published          -> final Artifact refs are required
composition_staged -> final Artifact refs are absent
```

`ProjectBuildRequest` is schema v2 with optional `outputRelativeRoot`. A Pack build step may name
one `isolatedOutputProperty`; the registered Adapter currently accepts only `ModsPath` and binds it
to a normalized directory below the staged project.

#### 3. Contracts

| Boundary | Required behavior |
| --- | --- |
| Preflight | Resolve and validate the complete `ResolvedItemGraph` before Run creation and repeat it inside the Feature before model or project work |
| Node execution | ExecutionGraph stores a strict serial `Plan -> Single` pair per sorted Item and one local finalize node; succeeded normalized checkpoints and exact terminal child Runs survive parent failure/restart |
| Resume | A new parent Run claims the same graph and restores successful Single proposals from typed checkpoints plus immutable Resources; it does not call the model for those nodes |
| Staging | Copy one bounded, non-symlink project worktree below `.ats/composition-staging/<parentRunId>` and exclude mutable evidence/build roots |
| Validation | Apply every proposed write to the isolated copy, validate once, then Build once with a Pack-declared isolated output property |
| Package | Prepare one ZIP inside the isolated copy; its child result remains `composition_staged` |
| Publication | Stream generated files plus the prepared ZIP through one rollback-capable real-project transaction, clean the isolated stage, publish one composition Artifact, then complete the parent Run |
| Evidence | Parent result/Artifact bind graph digest, exact root/profile/Draft provenance, every node definition/model request/resource identity, all child Run IDs, package report and every final file hash |
| React | Select only confirmed Pack-declared composition roots, submit the typed request, and render persisted Run terminal state without a Character/game branch |

The package output path must be unique. Generated target paths are unique unless every collision
declares the same `compositionMerge=json_object` contract; those inputs are flat-string validated,
duplicate-key checked and deterministically consolidated before staging. Every node must resolve the
same validation Primitive. Pack data cannot choose commands, arguments, arbitrary environment
variables or an output path outside the isolated stage.

#### 4. Validation And Error Matrix

| Failure | Stable family | Required mutation result |
| --- | --- | --- |
| stale/missing/wrong graph node, resource or Truth | `composition.graph.*` | no model call when preflight decides; no staging/project/Artifact mutation |
| one Plan/Single failure or cancellation | originating `model.*`, `truth.*`, `resource.*`, `pack.*` or `run.*` | graph paused at that node; completed checkpoints/child Runs retained; no real-project publication |
| graph claim/checkpoint/storage failure | `composition.execution.*` or `run.storage_failed` | no overwrite or guessed recovery; successful checkpoints remain immutable |
| invalid/oversized/symlinked staging source | `composition.staging.*` | owned stage removed; real project unchanged |
| whole-closure validation rejection | `validation.rejected` | staged copy removed; real project/Artifact unchanged |
| Build/Package rejection | typed Build/`artifact.*` family | child Run terminal; staged copy removed; real project unchanged |
| package commit, final write, Artifact publish or parent transition failure | typed `artifact.*`, `composition.publication.*` or `run.*` | all still-rollback-capable project state is restored; no fabricated success |
| success | succeeded parent + all terminal children | exactly one composition Artifact and final source/package set; no `.staging` or composition stage residue |

Ordinary in-process failures must complete cleanup before return. `FileProjectWriter` persists one
versioned immutable `record-<index>.json` per write below `.ats/transactions/<runId>` before moving
the old target. The same-directory rename to `.committed-<runId>` is the durable commit decision.
`FileProjectWriter::recover(project_root)` rolls back prepared targets/backups/temporary files and
run-created empty directories, while committed recovery preserves published bytes and removes only
journal state. `ProjectSession::open` runs this before Run reconciliation. Unknown, duplicate,
symlinked or escaping records fail recovery without guessing a result.

#### 5. Good / Base / Bad Cases

- Good: a two-node identity+pinned graph creates four Plan/Single children plus Build and Package,
  validates/builds once, publishes two sources and one ZIP in one composition Artifact and leaves no
  staging directory.
- Good: the first Single output is invalid after its Plan succeeds; a new repository instance and
  new parent Run resume that Single, then continue later Items without repeating the Plan.
- Base: validation rejects after every proposal. Four successful proposal child Runs remain valid
  evidence, while the real project, package and Artifact roots remain unchanged.
- Bad: invoke ordinary Single independently for each node. The first node could publish before a
  later cross-reference fails and whole-closure compilation would never be proven.
- Bad: point normal Build output at the real game Mods directory. An isolated rejection could still
  mutate external published state.

#### 6. Tests Required

```powershell
cargo test -p agentthespire-desktop --test composition_generation -- --nocapture
cargo test -p ats-adapters project_stager -- --nocapture
cargo test -p ats-adapters project_writer -- --nocapture
cargo test -p agentthespire-desktop --lib project_session -- --nocapture
cargo test --workspace --all-targets
npm run test:frontend
npx tsc -b --pretty false
```

Assertions must cover graph/root identity, exact child count/status, staged discriminators, one
validation/build/package, source-file ZIP streaming, manifest/hash recomputation, validation and
package/final-commit rollback, zero real-project mutation on failure, zero staging residue, and no
Character/STS2 branch in generic Feature/Shell/React code. They must also cover restart recovery,
successful-node request counts, exact child Run create-or-match and succeeded reconciliation with
zero model requests.

`mod-plan` pretty-serializes the complete verified `itemTypes` catalog plus plan guidance into the
required `pack.guidance` slot. That slot is bounded to 64,000 characters. A built-in Pack expansion
must keep the rendered value within this bound and pass the desktop facade Plan tests; overflow is
`FeatureRecipeError::SlotTooLarge`, persists as `feature.recipe_invalid`, and must not call the model.

### Scenario: Transport A Typed Output Contract To HTTP Providers

#### 1. Scope / Trigger

This contract applies whenever `ats-adapters::HttpModelClient` sends an
`ats-runtime::ModelRequest`. The request already contains a validated
`ModelOutputContract { schema, json_schema }`; dropping it at the HTTP boundary turns a typed
request into prompt-only JSON and can produce `model.output_invalid` for an otherwise valid Run.

#### 2. Signatures

```rust
// crates/ats-runtime/src/model.rs
pub struct ModelRequest {
    pub messages: Vec<ModelMessage>,
    pub output_contract: ModelOutputContract,
    pub max_output_tokens: u32,
    pub temperature: Option<f32>,
    pub model: Option<String>,
}

// crates/ats-adapters/src/model_client.rs
pub struct ModelRequestQueue { /* one FIFO slot */ }
impl ModelRequestQueue {
    pub fn new() -> Self;
}

impl HttpModelClient {
    pub fn new_with_queue(
        config: &LlmConfig,
        queue: Arc<ModelRequestQueue>,
    ) -> Result<Self, ModelError>;
}

fn openai_request_body(
    request: &ModelRequest,
    model: &str,
    response_format: OpenAiResponseFormat,
) -> serde_json::Value;
fn anthropic_request_body(request: &ModelRequest, model: &str) -> serde_json::Value;

// crates/ats-adapters/src/config.rs
pub enum OpenAiResponseFormat {
    JsonSchema,
    JsonObject,
}

pub struct LlmConfig {
    // existing provider/model/credential fields omitted
    pub openai_response_format: OpenAiResponseFormat,
    pub retry_initial_delay_ms: u64,
    pub retry_followup_delay_ms: u64,
}
```

#### 3. Contracts

| Runtime/config field | OpenAI-compatible Chat Completions | Anthropic Messages |
| --- | --- | --- |
| `messages` | `messages[]`; all roles retained | system roles joined into `system`; other roles in `messages[]` |
| `output_contract.json_schema` | exact schema remains in the rendered Recipe Prompt for both configured modes | `output_config.format.schema` |
| `openai_response_format=json_schema` | `response_format.type=json_schema`, fixed name `agentthespire_output`, `strict=true`, exact schema | not applicable |
| `openai_response_format=json_object` | exactly `response_format.type=json_object`; no unsupported schema member is sent | not applicable |
| `max_output_tokens` | `max_tokens` | `max_tokens` |
| optional `temperature` | present only when configured | present only when configured |

The Adapter sends no Feature/Game Pack prompt of its own and never persists or logs the request,
Authorization header, or provider body.

`llm.openai_response_format` is an explicit compatibility choice, not a runtime fallback. It
defaults to `json_schema` for old configuration files. `json_object` is allowed only for an
OpenAI-compatible proxy that demonstrably ignores or rejects native JSON Schema while honoring
JSON Object mode. The Adapter sends exactly one configured format and never retries the same
logical request with a weaker format. In both modes the Feature performs the same authoritative
typed decode and validation against `ModelOutputContract`; `json_object` does not enable Markdown
stripping, first-object extraction, missing-field defaults or partial success.

`Stage2Composition::built_in` creates exactly one `Arc<ModelRequestQueue>`. Every production call
to `select_model` passes that same Arc to `HttpModelClient::new_with_queue`; the public
`HttpModelClient::new` private-queue convenience is not used by the desktop composition path.
`llm.retry_initial_delay_ms` and `llm.retry_followup_delay_ms` default to `120000` and `300000`.
Figment environment overrides use `SPIREFORGE_LLM__RETRY_INITIAL_DELAY_MS` and
`SPIREFORGE_LLM__RETRY_FOLLOWUP_DELAY_MS`.

#### 4. Validation & Error Matrix

| Provider result | Adapter result | Persisted Feature family |
| --- | --- | --- |
| 2xx with a normal completion envelope | `ModelResponse`; Feature performs authoritative typed decode | `succeeded` or `model.output_invalid` |
| 2xx with malformed/empty completion envelope | `ModelError::InvalidResponse` | `model.response_invalid` |
| 400/other non-retryable rejection, including unsupported structured output | `ModelError::Rejected` | `model.request_rejected` |
| 401/403 | `ModelError::Authentication` | `model.authentication` |
| 429 | `ModelError::RateLimited` | `model.rate_limited` |
| transport/5xx after bounded retries | `ModelError::Transport` | `model.transport_failed` |
| unknown `openai_response_format` during configuration load | configuration remains invalid before model work | `model.configuration` / configuration status |
| retry delay is zero or above 3,600,000 ms | construction returns `ModelError::Configuration`; no queue/HTTP work | `model.configuration` / composition-root configuration mapping |
| another task owns the FIFO slot | wait cancellation-aware; issue no HTTP request | Run remains `running` until acquired or cancelled |
| cancellation wins while queued | `ModelError::Cancelled`; slot is not retained | `cancelled` through the Run supervisor |

The HTTP Model Adapter makes at most three attempts. Retryable failures wait through a
cancellation-aware bounded delay: a numeric provider `Retry-After` is honored between one and 120
seconds; otherwise the first and second retries use the validated
`llm.retry_initial_delay_ms`/`llm.retry_followup_delay_ms` values, whose compatible defaults are
120 and 300 seconds. Feature code must not add a second retry loop, and a terminal Run records only
the final typed provider family.

Every desktop HTTP model task enters the one FIFO `ModelRequestQueue` owned by the composition
root. One logical task holds its slot through all attempts, retry waits, response parsing and
terminal return. A queued task observes cancellation without issuing an HTTP request. Queue state
is process-local Adapter scheduling state and never enters a Prompt, Run, Artifact or Shell DTO.

An incompatible proxy is a configuration/provider failure unless the operator explicitly selects
its verified `json_object` capability before the Run. A terminal provider or typed-output failure
must not trigger a second prompt-only request, a format downgrade, code-fence stripping,
first-object extraction, or a fabricated success.

#### 5. Good / Base / Bad Cases

- Good: a capable provider receives the exact Recipe JSON Schema and returns one contract-valid
  object; Feature code still revalidates the decoded type.
- Good: a proxy proven to support JSON Object but not JSON Schema is configured once with
  `openai_response_format=json_object`; one request returns JSON and the Feature applies its full
  typed contract locally.
- Good: concurrently submitted Plan and Single Runs share one queue; the earlier task completes all
  attempts and retry waits before the later task can send its first HTTP request.
- Base: `temperature=None` omits the provider field, while the output contract is still mandatory.
- Base: a config file without `openai_response_format` uses `json_schema`; missing retry fields use
  120/300-second defaults.
- Bad: the provider rejects `response_format`/`output_config`; the Run retains a typed model failure
  and no fallback request is issued.
- Bad: the provider returns Markdown fences or extra fields; Feature strict decoding fails with
  `model.output_invalid` rather than accepting a partial object.
- Bad: a failed `json_schema` request is automatically retried as `json_object`; this changes one
  Run's contract after dispatch and can duplicate expensive provider work.
- Bad: each Run constructs its own queue, or a task releases the queue between retries; both allow
  a later Run to recreate a provider burst.

#### 6. Tests Required

```powershell
cargo test -p ats-adapters model_client -- --nocapture
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Required assertions:

- `openai_request_enforces_the_core_output_contract`: exact schema, `json_schema`, fixed name and
  `strict=true` are present.
- `openai_request_omits_an_absent_temperature`: optional transport fields do not weaken the schema.
- `openai_json_object_format_is_explicit_and_keeps_the_prompt_contract`: the selected request has
  exactly `response_format.type=json_object`, while messages retain the Recipe-rendered contract.
- `old_llm_config_uses_bounded_long_retry_defaults`: old config also defaults to `json_schema`.
- `llm_config_accepts_an_explicit_openai_json_object_format`: the compatibility value round-trips
  through the typed configuration.
- `llm_config_rejects_an_unknown_openai_response_format`: unsupported compatibility values fail
  configuration parsing instead of selecting a prompt-only or implicit mode.
- `anthropic_request_enforces_the_core_output_contract`: system-message handling and
  `output_config.format.schema` are both preserved.
- `retry_delay_honors_bounded_provider_guidance`: numeric `Retry-After` is honored and clamped to
  the documented one-to-120-second boundary.
- `retry_delay_uses_spaced_fallbacks_without_provider_guidance`: transport failures and rate limits
  without `Retry-After` use the documented configured 120/300-second default schedule.
- `model_request_queue_is_fifo_across_clients`: clients sharing one queue complete in arrival order.
- `queued_model_request_observes_cancellation`: a cancelled waiter never acquires the queue slot.
- `model_clients_can_share_one_request_queue`: independently constructed clients use the same
  composition-root queue and retry policy.
- `retry_configuration_is_bounded`: zero and values above one hour fail before any HTTP request.
- A real-provider acceptance uses a normal natural-language Plan and proves the persisted typed
  result; it is environment acceptance, not a deterministic machine gate.

#### 7. Wrong vs Correct

Wrong — the Adapter silently downgrades after a provider failure:

```rust
client.complete(with_json_schema)
    .or_else(|_| client.complete(with_json_object))
```

Correct — configuration selects one provider-native shape before the Run, while the same Recipe
Prompt and local typed validator remain authoritative:

```rust
let response_format = match config.openai_response_format {
    OpenAiResponseFormat::JsonSchema => native_json_schema(&request.output_contract),
    OpenAiResponseFormat::JsonObject => json!({ "type": "json_object" }),
};
client.complete_once(response_format)
```

Wrong — each Run receives an unrelated queue:

```rust
let model = HttpModelClient::new(config)?;
```

Correct — the composition root owns the queue and every Run shares it:

```rust
pub struct Stage2Composition {
    model_queue: Arc<ModelRequestQueue>,
}

let model = HttpModelClient::new_with_queue(config, Arc::clone(&self.model_queue))?;
```

Every constraint enforced on model output that JSON Schema can express must also be present in the
Recipe output contract shown to the model, including identifier patterns, string bounds and array
bounds. Code validation remains authoritative, but it must not rely on a stricter hidden shape that
the model cannot see.

Do not ask a model to echo internal Pack identifiers only so a later Feature can compare them with
the same Pack. Keep those identifiers out of the model-output schema and enrich the public Feature
result deterministically from the verified contribution.

Never treat model-authored prose as an exact Truth symbol or require users to know internal index
keys. Do not add heuristic token splitting in a generic Feature; new game/item evidence discovery is
a versioned Pack contribution contract.

### Scenario: Compile Pack File Roles Into A Run-Scoped Generate Contract

#### 1. Scope / Trigger

This contract applies after `SingleGenerateService` resolves the selected
`pack.mod-generate-single` item type and before it calls `ModelClient`. Pack-owned generated-file
roles vary by item type, so a fixed Recipe schema cannot truthfully express the required model
response shape.

#### 2. Signatures

```rust
// crates/ats-features/src/prompt/mod.rs
pub fn render_with_output_contract(
    &self,
    values: &BTreeMap<String, String>,
    model: Option<String>,
    output_contract: ModelOutputContract,
) -> Result<ModelRequest, FeatureRecipeError>;

// crates/ats-features/src/mod_generate_single.rs
fn run_scoped_output_contract(item_spec: &GenerateItemType) -> ModelOutputContract;

struct GeneratedModBundle {
    files: BTreeMap<String, serde_json::Value>,
    acceptance_notes: Vec<String>,
}
```

The Recipe declares `feature.mod-generate-single-bundle` v2. The selected item type supplies
`generatedFiles[].role`; target paths remain Feature-owned and are never model-authored.

#### 3. Contracts

| Source | Run-scoped destination |
| --- | --- |
| `itemType.id` | `pack.contribution.itemType` |
| `contribution.guidance[]` | `pack.contribution.guidance[]` |
| `itemType.generatedFiles[].role` | `pack.contribution.generatedFileRoles[]` |
| same role set | `output_contract.json_schema.properties.files.properties` keys |
| same role set | `properties.files.required[]` |
| role without `compositionMerge` | one non-blank bounded string property |
| role with `compositionMerge=json_object` | one object property whose values are bounded strings |
| dynamic JSON Schema | exact serialized `output.contract` Prompt slot |
| dynamic JSON Schema | `ModelRequestSnapshot.request.outputContract` and provider-native schema |

Bundle v2 uses a role-keyed object:

```json
{
  "files": {
    "source": "complete generated source",
    "localization.eng": {
      "MOD-ITEM.title": "Generated title"
    }
  },
  "acceptanceNotes": []
}
```

`files.additionalProperties=false`; every declared role is required. A normal role is one
non-blank, NUL-free string bounded to 16 MiB. A `compositionMerge=json_object` role is directly a
flat JSON object with string values; it is never a JSON-encoded string. The Feature orders and
serializes that object locally, enforces the same aggregate 16 MiB bound, and stores only the
normalized string in durable checkpoints and project writes. Acceptance notes are optional as an
empty array and are otherwise non-blank, NUL-free, at most 64 items and 2,000 characters each.

#### 4. Validation & Error Matrix

| Condition | Boundary result | Persisted Run family |
| --- | --- | --- |
| Recipe schema identity differs from dynamic contract | `FeatureRecipeError::InvalidContract` before HTTP | `feature.recipe_invalid` |
| serialized `output.contract` differs from Snapshot contract | `FeatureRecipeError::InvalidContract` before HTTP | `feature.recipe_invalid` |
| provider rejects the dynamic schema | `ModelError::Rejected` | `model.request_rejected` |
| malformed JSON, wrong shape, missing/extra role, blank/NUL/oversized content | typed decode or `validate_bundle` rejection | `model.output_invalid` |
| exact roles and bounded content | continue to project transaction, validation and Artifact publication | later typed stage or `succeeded` |

Code validation remains authoritative. It must reject an incompatible provider response even when a
proxy falsely claims strict-schema support, but it cannot keep JSON-Schema-expressible role/count
requirements hidden from the request.

#### 5. Good / Base / Bad Cases

- Good: `custom_code` compiles one required `source` property into Prompt, Snapshot and provider
  schema; a matching object reaches compile validation.
- Base: `relic` compiles `source`, `localization.eng`, and `localization.zhs`; Pack order controls
  deterministic project writes while JSON object key order is irrelevant.
- Bad: a generic `files: [{ role: string, content: string }]` schema lets the provider return an
  arbitrary role that Runtime later rejects; this caused the installed-candidate failure.
- Bad: declare a merge role as a string and ask the model to JSON-encode a flat object inside that
  string; this hides the merge shape from structured output and makes correctness model-dependent.
- Bad: Prompt displays one schema while the HTTP request carries another; Recipe rendering rejects
  this mismatch before model work.

#### 6. Tests Required

```powershell
cargo test -p ats-features --all-targets
cargo test -p ats-adapters model_client -- --nocapture
cargo test -p agentthespire-desktop --test stage2_single_mod --test stage2_composition
cargo test -p agentthespire-desktop --lib composition::tests::facade_persists_plan_generation_artifact_and_releases_project_lock -- --exact
```

Required assertions:

- exact role keys appear in both `properties` and `required`, with `additionalProperties=false`;
- Prompt contains the exact pretty-serialized Snapshot schema once;
- Prompt Pack contribution exposes item type, guidance and generated roles;
- wrong role fails `model.output_invalid`; correct multi-role output compiles and publishes;
- provider transport tests preserve the `ModelRequest` schema without rewriting it;
- facade E2E still proves Run v3, Artifact v3/hash, no staging and lock reacquisition.

#### 7. Wrong vs Correct

Wrong - fixed generic roles plus a stricter hidden validator:

```json
{"files":{"type":"array","items":{"properties":{"role":{"type":"string"}}}}}
```

Correct - specialize the verified Pack roles once and reuse that exact contract:

```rust
let output_contract = run_scoped_output_contract(item_spec);
let rendered = serialize(&output_contract.json_schema)?;
let request = recipe.render_with_output_contract(&slots, model, output_contract)?;
```

## 7. File And Process Work

- External IO is behind Runtime/Workspace ports and Adapter implementations.
- File updates use explicit transactions and rollback on validation/publication/cancellation failure.
- Child process cancellation kills/waits before drain success.
- Tool runners are finite registered Primitives; Pack data cannot inject shell commands.
- Build/package consume declared inputs and never recurse arbitrary directories into output.

## 8. Shell Boundary

Tauri invokes only `Stage2Composition` for product execution. React uses runtime guards for v3 DTOs and treats persisted `get_run` as terminal authority. Web exposes health/catalog/SPA only until a real Web execution composition is designed. CLI catalog comes from the shared registry.

### Scenario: Resolve Desktop Configuration Independently Of CWD

#### 1. Scope / Trigger

This contract applies during installed Tauri desktop startup. The process working directory is not
a stable installation identity and must not choose the desktop configuration file. Web/CLI keep
their existing `SettingsStore::load` cwd behavior and are outside this desktop-only migration.

#### 2. Signatures

```rust
// crates/ats-adapters/src/config.rs
pub fn SettingsStore::load_desktop(
    default_path: &Path,
    legacy_path: Option<&Path>,
) -> (Settings, ConfigStatus);

pub fn SettingsStore::desktop_legacy_config_path(
    executable_path: &Path,
) -> Option<PathBuf>;

// src-tauri/src/lib.rs
let app_data = AppDataPaths::resolve();
let legacy_config = std::env::current_exe()
    .ok()
    .as_deref()
    .and_then(SettingsStore::desktop_legacy_config_path);
let (settings, status) =
    SettingsStore::load_desktop(&app_data.config_path, legacy_config.as_deref());
```

#### 3. Contracts

| Input/state | Selected path and behavior |
| --- | --- |
| non-empty `SPIREFORGE_CONFIG_PATH` | resolve it to an absolute path, load it directly and do not attempt legacy migration |
| AppData `AgentTheSpire/config.json` exists | load AppData directly |
| AppData missing; valid EXE-sibling `runtime/agentthespire.config.json` exists | validate file values without environment overlays, save them to AppData, reload AppData with runtime overlays and keep the legacy source |
| AppData and legacy both missing | use default Settings with `ConfigStatus.path=AppData`, `filePresent=false`, `loaded=false` |
| legacy invalid or AppData save fails | return the real legacy status/path; do not claim AppData loaded |

`SPIREFORGE_*` value overrides such as `SPIREFORGE_LLM__API_KEY` are merged only when loading the
selected runtime path; they are not serialized into AppData by legacy migration.
`SPIREFORGE_CONFIG_PATH` is the only environment override for the configuration-file path. Desktop
resolution never calls the cwd/ancestor search in `resolve_path`.

#### 4. Validation & Error Matrix

| Failure | Observable state | Forbidden result |
| --- | --- | --- |
| explicit file missing | explicit path, `filePresent=false`, `loaded=false` | fallback to cwd or legacy |
| explicit/AppData JSON invalid | selected path, `loaded=false`, bounded configuration error | raw serde/IO text or another path silently loaded |
| legacy JSON invalid | legacy path/status remains authoritative; AppData absent | fabricated AppData success |
| legacy-to-AppData save failure | validated legacy Settings with runtime overlays and legacy status returned | deleting legacy or reporting AppData loaded |
| valid migration | file-authored values are persisted; AppData path/status is loaded with runtime overlays and source still exists | destructive move/rename of legacy source or persistence of environment-only values |

#### 5. Good / Base / Bad Cases

- Good: installed desktop starts from any cwd and loads the same AppData config.
- Good: one valid legacy file is copied once, then later starts load AppData without touching source.
- Base: no file exists; defaults load while Settings UI reports the AppData target as absent.
- Bad: repository `runtime/agentthespire.config.json` wins because the app was launched from that
  repository; installed behavior would depend on an unrelated cwd.
- Bad: migration copies invalid JSON or deletes the only legacy file.

#### 6. Tests Required

```powershell
cargo test -p ats-adapters config -- --nocapture
cargo check -p agentthespire-desktop --all-targets
```

Assertions must cover explicit-path precedence, cwd independence, valid non-destructive migration,
file-only persistence before environment overlays, missing-file base status, invalid legacy status
and migration-write failure without fake AppData success.

#### 7. Wrong vs Correct

Wrong:

```rust
SettingsStore::load(None) // searches current_dir().ancestors()
```

Correct:

```rust
let legacy = current_exe().ok().as_deref()
    .and_then(SettingsStore::desktop_legacy_config_path);
SettingsStore::load_desktop(&app_data.config_path, legacy.as_deref())
```

Raw LLM completion, Prompt preview, old planning/codegen routes, v2 submission commands, and Shell-owned filesystem transactions are forbidden.

## 9. Deterministic Facade E2E

The desktop gate must prove:

```text
isolated runtime + verified Truth
-> create/open STS2 project and acquire lock
-> mod.plan through ProjectSession
-> deterministic ModelClient
-> persisted succeeded RunRecord v3
-> mod.generate.single
-> real registered compile
-> ArtifactManifest v3 and file hash recomputation
-> no final .staging residue
-> cancel/drain/release
-> lock can be acquired again
```

The deterministic client removes network variability; it does not replace separate real-provider or installed-app acceptance.

## 10. Required Gates

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

Also run legacy Core/handler/Prompt search guards and rustfmt over every changed Rust file. Full candidate builds, installers, external models, and real game behavior remain explicit higher-cost/manual gates.

## 11. Forbidden Patterns

- Game branches in Kernel/Runtime or generic Feature infrastructure.
- A Feature importing a Shell or provider SDK.
- An Adapter importing Feature orchestration.
- User/AI resource bytes bypassing Resource Workspace.
- Hidden fallback to stale Truth, old Prompt, or old Run/Artifact schema.
- Success persisted before Artifact final publication or cleanup completion.
- Tests deleting existing release, verification, `.tmp`, user projects, or real evidence.
