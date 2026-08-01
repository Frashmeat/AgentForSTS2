# Database Guidelines

> Current status: not applicable to the desktop MVP.

AgentTheSpire does not use a product database in the current Rust/Tauri desktop scope. A Mod project is a self-contained folder whose authoritative state is persisted through versioned JSON, immutable artifact snapshots, and atomic filesystem operations.

## Current Persistence Contracts

- Project identity: `project.json` plus `.ats/version`.
- Run facts: `history/` schema-v2 `RunRecord` files through `FileRunRepository` CAS.
- Successful artifacts: `artifacts/<artifact-id>/runs/<run-id>/artifact-manifest.json` and immutable `files/`.
- Failed diagnostics: `.ats/diagnostics/<run-id>/`.
- Workstation configuration, Game Pack Truth Snapshots, and recent projects live below the configured external app-data root, not inside the repository.

## Forbidden Without A New Approved Task

- Adding SQLx/PostgreSQL, migrations, a platform queue, or database-backed Run/Audit storage.
- Treating a database as a hidden second authority beside project RunRecord/ArtifactManifest files.
- Moving desktop project locks or transactional file publication into an unrelated service.

Any future database introduction requires an explicit architecture task defining ownership, migration/rollback, concurrency, backup, and the cutover from current filesystem authorities.
