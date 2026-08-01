# Backend Directory Structure

> Current Rust workspace ownership and placement contract.

## Workspace Layout

```text
crates/
  ats-core/       framework-independent domain and application logic
  ats-web/        Axum HTTP/static frontend shell
  ats-cli/        operator/build command shell
src-tauri/        Tauri desktop composition root and IPC commands
src/              shared React/TypeScript frontend
game_packs/       validated game-specific declarations and resources
scripts/          CI, release, E2E, and bounded workstation automation
runtime/          shipped config examples only; mutable runtime data is external
artifacts/        ignored local build/release/E2E outputs
```

## Ownership Rules

- `ats-core` owns project folders, Game Pack loading, Truth Snapshot verification, Prompt/evidence assembly, Run lifecycle, ArtifactManifest, generation, validation, build/package algorithms, cancellation, and stable failures. It does not import Tauri, Axum, or React types.
- `src-tauri` owns the active desktop project, `ProjectSession`, app-data paths, workstation configuration bindings, IPC commands, startup prewarm, and application exit integration. It delegates domain work to Core.
- `ats-web` and `ats-cli` are shells. They may map transport/CLI arguments but do not copy Run state machines, artifact schemas, feature rules, or game-specific content.
- `game_packs/<game-id>/` owns truth-source declarations, guidance, project templates, resource specifications, validation rules, build recipe, and package layout. Generic handlers do not branch on `game_id == "sts2"`.
- `scripts/` may orchestrate closed project commands. A release script cannot accept arbitrary shell commands or reimplement Cargo feature/BuildInfo identity owned by `build.ps1` and `ats-core/build.rs`.

## Platform DDD Placement

```text
crates/ats-core/src/platform/
  domain/          RunRecord, RunResult, status/timeline invariants
  repository/      CAS and persistence ports/implementations
  application/     submission services and handlers
  artifact/        immutable ArtifactManifest publication
```

Domain types must not depend on filesystem UI state. Application handlers receive verified project, Pack/Snapshot, adapter, cancellation, and repository dependencies through explicit inputs.

## Naming And File Rules

- Rust modules and files use `snake_case`; public types use `UpperCamelCase`.
- Persisted JSON uses explicit serde casing/version fields. A schema change updates its tests and code-spec in the same task.
- PowerShell scripts use kebab-case filenames and approved verbs for exported functions where practical.
- Generated files never enter source directories. Build, release, model, Truth Snapshot, and E2E outputs stay below ignored `artifacts/` or configured external app-data roots.
- Stable contracts live in `.trellis/spec/`; task state and evidence live in `.trellis/tasks/`; current architecture/facts live in `docs/01-总览` and `docs/02-现状`.

## Good / Base / Bad

- Good: a new generic validation algorithm is implemented in Core and selected by a validated Game Pack declaration.
- Base: a Tauri-only workstation path binding remains in the desktop composition layer.
- Bad: a React component writes project files, an IPC command owns a second Run repository, or a generic Core handler contains STS2 paths/hooks.
