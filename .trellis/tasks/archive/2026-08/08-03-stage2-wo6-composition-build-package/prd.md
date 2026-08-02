# Stage 2 Work Order 6: Composition, Build And Package

## Goal

Add typed `mod.generate.batch`, `mod.generate.complex`, `project.build` and `project.package`
Features without switching the current Shell. Batch must invoke the existing Single service; Complex
must coordinate Plan, prepared Resource selections, Batch, Build and Package. Build/package use
game-neutral Runtime ports and Adapter-registered implementations selected only by verified Pack
data.

## Ownership

- `ats-runtime`: game-neutral build-step and rollback-capable package-output ports and bounded reports.
- `ats-features`: batch/complex/build/package requests, results, workflow order, child Run ownership,
  contribution schemas and Artifact extensions.
- `ats-game-context`: verified build steps and package layout data only.
- `ats-workspace`: prepared selected Resource identities already established by WO5.
- `ats-adapters`: fixed `process.dotnet-publish` execution and safe atomic ZIP construction. It does
  not decode Pack or Feature payloads.
- `game_packs/sts2`: build step Primitive references and exact package file layout.

Legacy handlers and Shell composition remain unchanged until WO7.

## Contracts

### `project.build`

- `project.build.recipe` schema v1 contains a bounded ordered list of step IDs and registered
  Primitive IDs. It never contains a command, argument, environment variable or script.
- The Feature executes each step through `BuildRunner`; the Adapter maps
  `process.dotnet-publish` to fixed `dotnet publish --nologo` behavior.
- Reports contain stable step/Primitive/exit-code/success and bounded redacted output only.
- Unknown Primitive, cancellation, unavailable process and nonzero exit are typed and cannot become
  a successful Run.

### `project.package`

- `project.package.layout` schema v1 owns only normalized required-file templates. `{mod_id}` is the
  only placeholder.
- Request paths are normalized project-relative paths; absolute/external paths never enter the Run.
- `PackageWriter` validates every declared source against symlink/path escape, writes a same-directory
  temporary ZIP and exposes it atomically. Existing output is backed up until Artifact publication
  and Run success; failure/cancellation removes the new file and restores the old bytes.
- Package ArtifactManifest v3 contains the ZIP only. Feature extension records file/byte counts and
  the verified layout identity.

### `mod.generate.batch`

- Batch owns no Prompt, output decoder, path expansion, file transaction or compile implementation.
- Every item creates a typed child Run and calls `SingleGenerateService` with the same verified
  context and lower-layer dependencies.
- Successful child Runs are independent immutable results. Cancellation stops future children and
  never rewrites already terminal child evidence. `fail_fast` controls whether ordinary item failure
  stops later children.
- At least one success yields a typed partial/full batch result; zero successes is a typed batch
  failure.

### `mod.generate.complex`

```text
typed planning requests + prepared Resource selections
-> ModPlanService child executions
-> derived Single requests (Feature owns artifact IDs/paths)
-> BatchGenerateService
-> ProjectBuildService
-> ProjectPackageService
-> Complex Run success
```

- Complex does not add a second planning Prompt, code generator, Resource repository, build process
  or package implementation.
- Child Run IDs and typed summaries remain visible in the Complex result.
- Failure/cancellation stops later stages. Completed child Runs remain truthful and immutable; the
  outer Run cannot fabricate success for an unexecuted Build or Package stage.

## Machine Acceptance

- A two-item batch invokes Single twice and publishes two independently verifiable child Artifacts.
- Fail-fast, continue-on-error and cancellation preserve truthful child counts/statuses and stop at
  the documented boundary.
- A synthetic Pack and STS2 resolve through the same build/package schemas; unknown Primitive and
  unsafe layout fail before process/package mutation.
- A real isolated `dotnet publish` passes through the registered Adapter with bounded output.
- A package ZIP contains only Pack-declared files, publishes ArtifactManifest v3 and leaves no
  partial/backup/staging residue.
- Missing/symlinked required files, Artifact failure and cancellation restore an existing package.
- A complex fixture proves Plan -> Batch/Single -> Build -> Package using only composed services.
- Runtime/Adapters contain no game branch; Pack contains no command/script.

## Required Gates

```text
cargo test -p ats-runtime
cargo test -p ats-features
cargo test -p ats-adapters
cargo test -p agentthespire-desktop --test stage2_composition
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
rustfmt --edition 2024 --check <changed Rust files>
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

## Out Of Scope

- Shell/Tauri/React/CLI cutover and legacy Core deletion (WO7).
- A new installer/release candidate or mutation of accepted release evidence.
- Arbitrary Pack commands, dynamic plugins or a general workflow DSL.
- Creating new media from Complex input; WO6 consumes Resource versions prepared through WO5.

## Completion Evidence

- Added game-neutral Runtime build/package ports and registered Adapter implementations. The build
  Adapter maps only `process.dotnet-publish` to fixed `dotnet publish --nologo`; the ZIP Adapter
  packages only Feature-supplied entries with same-directory publication and old-output rollback.
- Added typed `mod.generate.batch`, `mod.generate.complex`, `project.build` and `project.package`
  slices. Batch invokes the existing Single service per child Run; Complex composes Plan,
  Batch/Single, Build and Package without a parallel Prompt, decoder, Resource or file transaction.
- The STS2 Pack build payload contains only step/Primitive IDs and its package payload contains only
  six required-file templates. Exact Pack SHA-256 is
  `00a0cc406271ef993254f78fafb9e55cd27b5f9edf03ede1fc49dd2dc99f721e`.
- `src-tauri/tests/stage2_composition.rs` proves two-item composition, continue/fail-fast semantics,
  cancellation before project writes, a real `dotnet publish`, exact six-file ZIP publication,
  ArtifactManifest v3, succeeded child Runs, old-package restoration and no run-scoped staging or
  transaction residue.
- Review found and fixed two cleanup gaps: every build completion/error path now removes its
  run-scoped report directory, and an abandoned package transaction automatically restores the
  previous output and removes its transaction directory.

Machine gates passed:

```text
cargo test -p ats-runtime
cargo test -p ats-features
cargo test -p ats-adapters
cargo test -p agentthespire-desktop --test stage2_composition
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
rustfmt --edition 2024 --check <all WO6 changed Rust files>
Pack SHA-256 and build/package payload boundary checks
git diff --check
```
