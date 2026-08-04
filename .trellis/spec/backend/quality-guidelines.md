# Backend Quality Guidelines

> Executable Stage 2 contracts for the modular monolith. Historical `ats-core` v1/v2 contracts are not active production guidance.

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

Every product capability has one `FeatureId`, request schema, result schema, validator, and registry entry. Current catalog contains exactly 9 Features: project create, plan, resource prepare, single/batch/complex generation, log analyze, build, and package.

Adding a Feature must not add a Runtime `RunKind`, center result union, Shell-specific implementation, or duplicate Prompt pipeline. Batch reuses Single; Complex composes Plan, Batch/Single, Build, and Package.

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

- Pack bytes/schema/hash are validated before registration.
- A Feature declares required Contribution slots; missing/duplicate/unknown slots fail before work.
- Pack data may reference only registered Primitive IDs and cannot execute arbitrary script/native code.
- Truth Snapshot sources/indexes and current pointer are content-addressed and verified for the active Pack.
- Evidence is bounded and traceable; missing/invalid current Truth stops dependent Features.
- Plan v2 stores descriptive `evidenceRequirements`; executable `symbols`/`terms` queries belong to
  each item type in the Pack v3 top-level catalog. Every query group must match current verified
  Truth before model work, and final Evidence is bounded and deduplicated.
- Required Resource role IDs are Pack-owned execution facts in the same item catalog. The Plan
  model output cannot author them; the Feature deterministically attaches them to Plan result v2.
  Plan and Single must not maintain parallel item type/resource/Truth declarations.
- Item capability readiness is computed from the pinned Pack plus current verified Truth before
  model work. Undeclared types are absent; declared types with missing Truth are disabled with
  typed blockers and do not create a Run.
- ItemDefinition schema v1 is validated on construction/deserialization and hashed from ordered
  canonical content. Run/Artifact consumers bind the exact hash; they never reconstruct locked
  fields from prose or mutate an older snapshot.
- Single request schema v3 carries one `StoredItemDefinition`; Batch v3 embeds Single v3 and
  Complex v2 carries one definition per planning item. `selectedResources` is forbidden at these
  boundaries. Plan identity is checked or deterministically normalized to the pinned definition.
- ResourceAsset schema v2 keeps new upload, Pack default, AI and deterministic derived outputs as
  immutable candidates with `selectedVersion=null`; only an explicit select may move the pointer.
  Master derivation uses one staged batch, and a failure preserves every prior selection.
- `pack.resource-specs` v2 owns exact media dimensions/alpha, direct master/derived edges and
  transform Primitive/version. PNG bytes are decoded before storage; derived provenance binds the
  source version, transform parameters and pinned Pack identity. `ProjectSession` owns the sole
  filesystem Resource repository for all Runs and commands.
- Media Adapter registration, provider configuration/connectivity, and generated-media quality are separate facts. Health reports only registration; failures remain typed and generated bytes are bounded before ingest.
- `pack.mod-generate-single` v4 separates bounded top-level common guidance from required
  `itemTypes[].guidance`; Single serializes only the selected type as `itemGuidance`. Generation
  entries must cover exactly the top-level catalog IDs and declare exact file roles. Type-specific
  rules must never be appended to common guidance or selected by a code branch.

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
fn openai_request_body(request: &ModelRequest, model: &str) -> serde_json::Value;
fn anthropic_request_body(request: &ModelRequest, model: &str) -> serde_json::Value;
```

#### 3. Contracts

| Runtime field | OpenAI-compatible Chat Completions | Anthropic Messages |
| --- | --- | --- |
| `messages` | `messages[]`; all roles retained | system roles joined into `system`; other roles in `messages[]` |
| `output_contract.json_schema` | `response_format.json_schema.schema` | `output_config.format.schema` |
| strict type | `response_format.type=json_schema`, `json_schema.strict=true` | `output_config.format.type=json_schema` |
| schema name | fixed provider-safe `agentthespire_output` | not required |
| `max_output_tokens` | `max_tokens` | `max_tokens` |
| optional `temperature` | present only when configured | present only when configured |

The Adapter sends no Feature/Game Pack prompt of its own and never persists or logs the request,
Authorization header, or provider body.

#### 4. Validation & Error Matrix

| Provider result | Adapter result | Persisted Feature family |
| --- | --- | --- |
| 2xx with a normal completion envelope | `ModelResponse`; Feature performs authoritative typed decode | `succeeded` or `model.output_invalid` |
| 2xx with malformed/empty completion envelope | `ModelError::InvalidResponse` | `model.response_invalid` |
| 400/other non-retryable rejection, including unsupported structured output | `ModelError::Rejected` | `model.request_rejected` |
| 401/403 | `ModelError::Authentication` | `model.authentication` |
| 429 | `ModelError::RateLimited` | `model.rate_limited` |
| transport/5xx after bounded retries | `ModelError::Transport` | `model.transport_failed` |

An incompatible proxy is a configuration/provider failure. It must not trigger a second
prompt-only request, code-fence stripping, first-object extraction, or a fabricated success.

#### 5. Good / Base / Bad Cases

- Good: a capable provider receives the exact Recipe JSON Schema and returns one contract-valid
  object; Feature code still revalidates the decoded type.
- Base: `temperature=None` omits the provider field, while the output contract is still mandatory.
- Bad: the provider rejects `response_format`/`output_config`; the Run retains a typed model failure
  and no fallback request is issued.
- Bad: the provider returns Markdown fences or extra fields; Feature strict decoding fails with
  `model.output_invalid` rather than accepting a partial object.

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
- `anthropic_request_enforces_the_core_output_contract`: system-message handling and
  `output_config.format.schema` are both preserved.
- A real-provider acceptance uses a normal natural-language Plan and proves the persisted typed
  result; it is environment acceptance, not a deterministic machine gate.

#### 7. Wrong vs Correct

Wrong — the schema exists only as prompt text:

```rust
json!({ "model": model, "messages": messages, "max_tokens": request.max_output_tokens })
```

Correct — the provider request carries the Runtime-owned contract:

```rust
json!({
    "model": model,
    "messages": messages,
    "response_format": {
        "type": "json_schema",
        "json_schema": {
            "name": "agentthespire_output",
            "strict": true,
            "schema": request.output_contract.json_schema,
        }
    }
})
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
    files: BTreeMap<String, String>,
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
| dynamic JSON Schema | exact serialized `output.contract` Prompt slot |
| dynamic JSON Schema | `ModelRequestSnapshot.request.outputContract` and provider-native schema |

Bundle v2 uses a role-keyed object:

```json
{
  "files": {
    "source": "complete generated source"
  },
  "acceptanceNotes": []
}
```

`files.additionalProperties=false`; every declared role is required. Each content string is
non-blank, NUL-free, and bounded to 16 MiB. Acceptance notes are optional as an empty array and are
otherwise non-blank, NUL-free, at most 64 items and 2,000 characters each.

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
