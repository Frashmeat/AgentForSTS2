# Stage 2 Contracts And Dependency DAG

> Executable contract for the Stage 2 modular-monolith boundary.

## Ownership

`ats-kernel` owns only stable values shared by at least two responsibility domains:

```rust
FeatureId / ContributionId / PrimitiveId / SchemaId / FailureCode
SchemaVersion
Sha256Digest
SchemaRef { id, version }
SchemaEnvelope<T> { schema, payload }
```

Feature workflow, game declarations, project resources, execution mechanisms and adapter implementations remain in their owning crates. Kernel cannot import IO, async runtimes, providers, games or Feature implementations.

## Value Contracts

- Qualified IDs contain at least two dot-separated segments. Each segment matches `[a-z][a-z0-9_-]*`.
- `SchemaVersion` rejects zero.
- `Sha256Digest` accepts exactly 64 hexadecimal characters and normalizes to lowercase.
- Constructors and Serde deserialization use the same validation path.
- `SchemaEnvelope<T>` serializes only `schema` and `payload`; its `SchemaRef` serializes `id` and `version`.

## Allowed Direct Project Dependencies

```text
ats-kernel       -> []
ats-runtime      -> [ats-kernel]
ats-game-context -> [ats-kernel]
ats-workspace    -> [ats-kernel]
ats-features     -> [ats-kernel, ats-runtime, ats-game-context, ats-workspace]
ats-adapters     -> [ats-kernel, ats-runtime, ats-game-context, ats-workspace]
```

No target crate may depend on legacy `ats-core` or a shell crate. Runtime, Game Context, Workspace and Adapters cannot depend on Features.

The authoritative gate is:

```text
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
```

It compares `cargo metadata` with the exact direct dependency sets. Updating the DAG requires an approved architecture change, this spec, the script fixture and the target Cargo manifests in one task.

## Validation And Error Matrix

| Condition | Required result |
| --- | --- |
| Valid qualified ID and version | Construct and JSON round-trip |
| Single segment, uppercase, empty segment or path syntax | Typed `InvalidQualifiedId` |
| Version zero | Typed `InvalidSchemaVersion` |
| Valid uppercase digest | Normalize to lowercase |
| Invalid digest length/character | Typed `InvalidSha256` |
| Allowed Cargo graph | Gate succeeds |
| Added reverse, legacy or undeclared edge | Gate fails and names the crate |
| Missing target crate or required edge | Gate fails before compilation |

## Good / Base / Bad

- Good: Runtime stores a validated `FeatureId` and schema reference without importing a Feature implementation.
- Base: a responsibility crate compiles with its exact declared lower-layer dependencies while current production behavior remains in `ats-core` during migration.
- Bad: a crate uses a raw string for the same cross-domain identity, adds a Runtime-to-Feature edge, or bypasses typed envelope decoding with unchecked JSON.

## Required Tests

```text
cargo test -p ats-kernel
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo check --workspace --all-targets
```

## Run And Artifact Envelope

`ats-runtime` owns `VersionedPayload`, `RunRecord` schema v3 and the base
`ArtifactManifest` schema v3. These contracts contain validated Feature/schema identities but no
product `RunKind`, product result enum, Game Pack type, evidence type, codegen type or image
processing type.

- `VersionedPayload::from_typed` accepts only typed values that serialize to a JSON object.
- Persisted request, result and Artifact extension payloads are decoded only through the registered
  `FeatureSpec` in `ats-features::FeatureRegistry`; unknown Features, schema mismatch and typed
  decode errors fail.
- A Run transition is transactional in memory: an invalid transition or invariant failure leaves
  the prior record unchanged. Deserialization rechecks schema, timestamps, timeline, attempts,
  failure/result exclusivity and terminal-event invariants.
- A base Artifact manifest contains only identity, producing Run, versioned context/provenance
  records, one Feature extension and immutable file records. Deserialization reruns full manifest
  validation.

`ats-adapters::FileArtifactStore` owns filesystem publication. It validates all source and
published paths before staging, rejects symlink inputs and non-directory ancestors, writes the
complete manifest and file snapshots under a unique `.staging-*` sibling, then exposes the
snapshot with one same-directory rename. Existing final directories are immutable and are never
overwritten.

The rename operation retries only `Interrupted` and, on Windows, `PermissionDenied` or OS errors
5/32/33 using 50/100/200/400/800 ms delays. Exhaustion and deterministic errors remain failures;
staging cleanup is best effort and no copy fallback can report success.

### Validation And Error Matrix

| Condition | Required result |
| --- | --- |
| Registered typed request/result/Artifact extension | Registry decodes the owning associated type |
| Unknown Feature | `FeatureRegistryError::UnknownFeature` before workflow execution |
| Schema mismatch or malformed object | `InvalidPayload` with typed `PayloadError`; no unchecked business input |
| Invalid Run transition or tampered persisted record | Typed lifecycle failure; prior in-memory record remains unchanged |
| Empty files, unsafe/non-normalized path, external source or symlink | Contract/path failure before final publication |
| Complete valid staging directory | Same-directory rename publishes one immutable final snapshot |
| Transient Windows rename conflict | At most five delayed retries after the initial attempt |
| Retry exhaustion or deterministic rename failure | Typed I/O failure, no final directory and best-effort staging cleanup |
| Existing final directory | `SnapshotExists`; existing contents are never overwritten |

### Good / Base / Bad

- Good: a newly registered fixture Feature validates its typed request, result and Artifact
  extension without adding a Runtime enum branch; the published manifest and every file hash can
  be recomputed and no `.staging-*` entry remains.
- Base: legacy `ats-core` v2 continues to compile and serve current Shells while the isolated v3
  contract is exercised only by Stage 2 fixtures.
- Bad: Runtime matches a product Feature ID, Adapters imports Feature/Game/codegen types, a raw JSON
  object enters a workflow without registry decode, or a failed rename is replaced by copy/success.

The legacy `ats-core` RunRecord v2 and ArtifactManifest v2 remain the production Shell contract
until the vertical Feature migrations and Work Order 7 cutover. New schema v3 files cannot be
presented as current production output before that cutover.

### Required Tests

```text
cargo test -p ats-runtime
cargo test -p ats-features
cargo test -p ats-adapters
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

## Model Request And Log Analysis Vertical Slice

`ats-runtime::ModelRequestSnapshot` schema v1 is the game-neutral, replayable model boundary:

```text
featureId
recipe { id, version, sha256 }
gamePack { id, sha256 }
truthSnapshotId?
selectedResources[] { resourceId, logicalRole, selectedVersion, mediaType }
request { messages, outputContract, maxOutputTokens, temperature?, model? }
requestSha256
```

The request identity hashes canonical JSON for every field except `requestSha256` itself. Resource
references are sorted and unique by `ResourceId`; deserialization revalidates structure and identity.
Runtime owns message roles, provider-neutral request/response/stream contracts, limits and safe model
error categories. It does not own Feature Recipe text, Pack guidance, project semantics or a product
result enum.

`ats-features::prompt` loads exact pinned Recipe bytes. A Recipe declares its Feature/version,
messages, each bounded slot exactly once, typed output schema, token limit and temperature. The
renderer rejects unknown, duplicate, unconsumed, missing or oversized slots and never reparses slot
tokens introduced by a supplied value. Recipe resources use LF exact bytes; Stage 2 does not support
unsigned runtime Recipe replacement.

`ats-features::log_analyze` owns the typed request/result, bounded UTF-8 tail policy, generic
diagnostic result validation and orchestration. It requires `log.analyze.rules` schema v1 from a
`VerifiedContributionSet`, obtains bounded evidence from a verified Truth Snapshot and records all
identities in the model snapshot. The STS2 Pack owns its compiler/build/runtime/mod-lifecycle
indicators and checks. Runtime and the generic Recipe contain none of those game/toolchain terms.

Runtime Custom Instructions enter only through `runtime.custom_instructions`, an optional slot that
the Recipe loader requires to occur exactly once. They cannot replace the system message or typed
output contract. Full prompt/model content is not copied into failure messages.

### Validation And Error Matrix

| Condition | Required result |
| --- | --- |
| Exact pinned Recipe and complete slots | Deterministic messages and verified request SHA-256 |
| Recipe hash/schema/template tampering | Fail before model invocation |
| Required/unknown/duplicate/oversized slot | Typed Recipe assembly failure |
| Pack slot/schema/Primitive missing | Contribution resolution fails before Feature execution |
| Pack, contribution and Truth identities differ | `ContextIdentityMismatch`; no model invocation |
| Same Recipe/request with different Pack guidance | Different rendered content and request identity |
| Oversized Unicode log | Keep the requested number of trailing characters on valid UTF-8 boundaries |
| Model returns malformed, oversized or truncated typed output | No fabricated `LogAnalyzeResult` |

### Good / Base / Bad

- Good: the same pinned Recipe combines one verified STS2 contribution and one fixed Truth Snapshot;
  the model sees the exact rendered request and the returned snapshot hash verifies after JSON
  round-trip.
- Good: a synthetic Pack supplies different typed log guidance, so the rendered messages and request
  hash change without a Feature or Runtime branch.
- Base: the new vertical slice compiles and is exercised through mocks while the current Shell still
  invokes the legacy `ats-core` handler until WO7.
- Bad: a handler contains a long production prompt, parses raw Pack JSON, accepts mismatched Pack and
  Truth identities, injects Custom Instructions into multiple messages or treats malformed model
  output as a typed success.

### Required Tests

```text
cargo test -p ats-kernel
cargo test -p ats-runtime
cargo test -p ats-game-context
cargo test -p ats-features
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

## Game Context And Resource Workspace

`crates/ats-game-context/src/pack.rs` owns `GamePackManifest` schema v2 loading. The loader receives
an expected `Sha256Digest` and hashes the exact input bytes before parsing. The built-in STS2 target
manifest is `game_packs/sts2/stage2-game-pack.json`; its digest is pinned beside the compile-time
`include_bytes!`. The legacy `game-pack.json` remains the current `ats-core` production manifest and
is not interpreted as schema v2.

Each contribution has `slotId`, `featureId`, `schema`, `requiredPrimitives` and an object `payload`.
`ContributionResolver::resolve` checks every Feature requirement and available Primitive before it
returns `VerifiedContributionSet`. Consumers can only obtain a typed value through
`VerifiedContributionSet::decode`; raw Pack JSON is not a Feature input.

`crates/ats-game-context/src/truth.rs` owns Truth Snapshot schema v2 and bounded evidence records:

```text
TruthSnapshotManifest
  schemaVersion / snapshotId
  gamePackId / gamePackSchemaVersion / gamePackSha256
  sources[] / indexes[] / toolVersions / createdAt

TruthEvidenceRecord
  sourceId / symbol / purpose / boundedExcerpt / relativePath
```

The snapshot ID is the SHA-256 of the canonical identity fields excluding `createdAt` and the ID
itself. `ats-adapters::FileTruthSnapshotRepository` reads
`truth/<pack>/current.json`, verifies every directory/file against symlinks, checks the Pack and
snapshot identities, recomputes source/index hashes and counts, then calls
`VerifiedTruthSnapshot::verify`. `EvidenceQuery` requires at least one symbol/term, limits results to
1..=50 and queries only the fixed verified record set.

`crates/ats-workspace/src/resource.rs` owns `ResourceAsset` schema v1. `ResourceOrigin` is exactly
`user_upload`, `ai_generated` or `pack_default`. AI origin records registered provider/model/request
hash; Pack origin records Pack ID/hash and contribution slot. Absolute input paths are transient
adapter input and never enter the manifest.

`ats-adapters::FileResourceRepository` stores each asset under
`.ats/resources/<resource-id>/`, with immutable content-addressed blobs below
`versions/<sha256>/` and an atomically replaced `resource-manifest.json`. A derived version names an
existing parent and registered transform; every parent chain must terminate at `originalVersion`.
Selection only updates `selectedVersion`; it cannot overwrite or point outside the asset.

### Validation And Error Matrix

| Condition | Required result |
| --- | --- |
| Pinned STS2 or synthetic exact bytes | Same Loader/Registry accepts schema v2 |
| Wrong Pack hash, schema, duplicate slot/Primitive or scalar payload | Typed load failure before registration |
| Missing slot, Feature/schema mismatch or unavailable Primitive | Typed resolver failure before Feature work |
| Snapshot identity, Pack identity, source/index hash/count mismatch | No `VerifiedTruthSnapshot` |
| Empty/oversized Evidence query or invalid record path/excerpt | Typed query/store failure |
| User, AI or Pack-default Resource input | Same repository and schema preserve distinct provenance |
| Derived version with unknown parent, cycle, wrong digest path or tampered blob | Reject without changing valid manifest |
| Selection names unknown version | Reject; manifest bytes stay unchanged |
| Symlinked Pack/Truth/Resource path | Reject; no verified handle or success state |

### Good / Base / Bad

- Good: pinned STS2 and a synthetic Pack resolve through one contract; a fixed verified Snapshot
  returns bounded evidence; all three Resource origins produce immutable, reloadable versions.
- Base: `ats-core` continues to use legacy Pack/Truth v1 while schema v2 fixtures establish the new
  ownership boundary for later Feature migration.
- Bad: a Feature parses raw Pack JSON, switches on `game_id`, accepts an unverified Snapshot, stores
  an upload absolute path, overwrites an old blob or selects a version from another asset.

### Required Tests

```text
cargo test -p ats-kernel
cargo test -p ats-game-context
cargo test -p ats-workspace
cargo test -p ats-adapters
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```
