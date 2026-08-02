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
