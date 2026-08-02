# Stage 2 Work Order 4: Prompt And Log Analysis Vertical Slice

## Goal

Establish the Stage 2 model-request boundary and migrate `log.analyze` into a new vertical slice
without switching the current Shell or `ats-core` production path. A model request must be assembled
from a versioned Feature Recipe, a verified Game Pack contribution, bounded Truth Evidence, selected
resource references, sanitized project context, one Runtime Custom Instructions slot, and the typed
run request. The resulting request must be deterministic and independently verifiable by identity and
hash.

## Ownership

- `ats-runtime`: game-neutral model messages, request/response/stream port, limits, request snapshot,
  deterministic hashing, and transport-safe model errors.
- `ats-features`: versioned Recipe loader/registry, required-slot rendering, typed `log.analyze`
  request/result, truncation policy, output contract, and orchestration.
- `ats-game-context`: verified contribution payload access and bounded Truth Evidence already created
  by WO3. It does not assemble prompts.
- `game_packs/sts2`: typed, declarative STS2 log-analysis guidance. Framework, MSBuild, Godot,
  BaseLib, and publish knowledge must not live in Runtime or the generic Feature Recipe.

Shell composition and old `ats-core` handlers remain unchanged until WO7. No compatibility facade is
added between old and new request types.

## Contracts

```text
Feature Recipe
+ Verified Pack Contribution
+ Bounded Truth Evidence
+ Selected Resource References
+ Sanitized Project Context
+ Runtime Custom Instructions (one slot only)
+ Typed Feature Request / Output Contract
= ModelRequestSnapshot -> ModelClient port
```

- Recipe schema/version/ID, required slots, message roles, templates, and output schema are validated
  before rendering.
- Templates use a deliberately small exact-slot syntax; unknown, duplicate, missing, or unconsumed
  slots fail with a typed assembly error.
- Runtime Custom Instructions may be supplied only through the dedicated slot. Empty instructions are
  omitted deterministically and cannot replace the system/output contract.
- `ModelRequestSnapshot` records Feature, Recipe and Game Pack identities, Truth Snapshot identity,
  selected resource identities, rendered messages, output contract, limits, and its canonical SHA-256.
- `requestSha256` excludes only itself. Map ordering and caller input ordering are normalized where
  order has no semantic meaning.
- `log.analyze` accepts inline log text in this vertical slice, keeps the last bounded number of
  characters on a UTF-8 boundary, invokes the Runtime model port, and returns a versioned typed result.
- The Feature recipe owns the generic diagnostic task and output shape. The Pack contribution owns
  game/framework-specific indicators and remedies. Truth Evidence remains quoted evidence, not Pack
  guidance.
- No long production natural-language prompt constant may exist in the new handler/service source.

## Failure And Safety

- Unsupported Recipe/schema, invalid template, missing slot, incompatible Pack contribution, invalid
  input, model failure, and malformed structured output are distinct typed errors.
- Provider bodies, credentials, absolute paths, full prompts, and model output are not embedded in
  error messages.
- Pack payload is decoded through `VerifiedContributionSet::decode`; raw unchecked JSON never enters
  Feature behavior.
- Log, context, custom instructions, evidence, and Pack guidance have explicit size/count bounds.

## Machine Acceptance

- A valid STS2 request renders all owned sections, invokes a mock model client, and decodes a typed
  diagnosis.
- A synthetic Pack with different guidance changes the request content and hash while using the same
  Recipe and Feature code.
- Missing required contribution/slot and wrong schema fail before model invocation.
- Recipe version/content changes are reflected by identity/hash; a tampered declared Recipe hash is
  rejected.
- Runtime Custom Instructions appear at most once and empty optional context is deterministic.
- UTF-8 log truncation, bounded evidence/resources/context, and malformed model output are tested.
- Repository search proves STS2/Godot/BaseLib/MSBuild terms do not appear in Runtime or generic Recipe.
- Run targeted tests, workspace `cargo check`, `cargo test`, and `cargo clippy --workspace --all-targets
  -- -D warnings`; run the Stage 2 Cargo DAG check.

## Documentation

Update the Stage 2 architecture progress, backend executable contract, current progress, and current
solution index with the exact implemented ownership and verification evidence.

## Out Of Scope

- Shell/Tauri/React migration.
- Replacing the legacy `ats-core` log-analysis production handler.
- `mod.plan`, code generation, resource preparation, build, or package migration.
- A new Windows release candidate or any manipulation of accepted release evidence.
