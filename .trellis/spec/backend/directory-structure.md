# Backend Directory Structure

> Current Stage 2 workspace ownership and placement contract.

## Workspace Layout

```text
crates/
  ats-kernel/       stable values, actionable failure, BuildInfo
  ats-runtime/      Run/Artifact envelopes, cancellation and ports
  ats-game-context/ Pack contributions, Truth/Evidence and templates
  ats-workspace/    project lock/recents and Resource Workspace
  ats-features/     typed Feature contracts, Recipes and workflows
  ats-adapters/     external provider/tool/storage implementations
  ats-web/          health/catalog/static SPA shell
  ats-cli/          operator/deploy/catalog shell
src-tauri/          desktop composition root, session and IPC
src/                React/TypeScript UI and transport guards
game_packs/         pinned game declarations, templates and resources
scripts/            bounded CI/release/E2E automation
runtime/            shipped configuration examples
artifacts/          ignored local build/release/verification evidence
```

## Ownership Rules

- `ats-kernel` contains only stable validated values shared across responsibility domains.
- `ats-runtime`, `ats-game-context`, and `ats-workspace` cannot depend on `ats-features`.
- `ats-adapters` implements lower-layer ports and cannot import Feature workflows.
- No target crate depends on deleted `ats-core`; exact edges are enforced by the DAG script.
- `src-tauri` owns composition, active ProjectSession, app-data paths, configuration binding, IPC and exit drain. It delegates product work to `Stage2Composition`.
- Web/CLI are Shells and cannot copy Run state machines, Feature rules, Prompt or game-specific content.
- `game_packs/<id>/` owns game contributions and template/resources; generic code cannot branch on STS2.
- `crates/ats-features/recipes/` owns pinned cross-game task language and output slots.
- Mutable runtime/Truth/model data stays in configured app-data or ignored evidence roots, never source directories.

## Placement Decision

```text
stable cross-domain value      -> ats-kernel
execution envelope/port        -> ats-runtime
game declaration/evidence      -> ats-game-context
project/resource state         -> ats-workspace
product behavior               -> ats-features
provider/filesystem/tool IO     -> ats-adapters
transport/session/composition   -> Shell
```

## Naming And Files

- Rust files/modules use `snake_case`; public types use `UpperCamelCase`.
- Persisted JSON has explicit schema/version and serde casing; schema changes update tests/spec in the same task.
- Generated/build/release data stays under project output, configured app-data, `target/`, or ignored `artifacts/`.
- Stable contracts live in `.trellis/spec`; task state/evidence in `.trellis/tasks`; current architecture/facts in `docs/01-总览` and `docs/02-现状`.

## Forbidden Examples

- React writes project files or owns Run state transitions.
- An IPC handler builds a second Prompt or filesystem transaction.
- Runtime imports a Feature to decode product results.
- An Adapter imports game/Feature types instead of implementing a port.
- A Pack embeds arbitrary commands or executable plugin code.
