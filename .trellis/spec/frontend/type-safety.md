# Type Safety

> Type safety patterns in this project.

---

## Overview

<!--
Document your project's type safety conventions here.

Questions to answer:
- What type system do you use?
- How are types organized?
- What validation library do you use?
- How do you handle type inference?
-->

(To be filled by the team)

---

## Type Organization

<!-- Where types are defined, shared types vs local types -->

(To be filled by the team)

---

## Validation

<!-- Runtime validation patterns (Zod, Yup, io-ts, etc.) -->

Desktop IPC results that carry Run, capability, Item or Resource state are received as `unknown`
and checked by runtime guards. Tauri-only guards may remain in `src/services/tauriApi.ts`; contracts
shared by both backend implementations, such as `ExecutionGraphView`, live in a pure shared module
(`src/services/executionGraphContract.ts`) and are re-exported identically by `tauriApi.ts` and
`webApi.ts`. A TypeScript interface alone is not proof of a response shape.

For Pack-driven Item state, Rust and TypeScript use the exact camelCase wire fields:

```text
ItemCapabilityCatalog:
  gamePackId, gamePackSha256, truthSnapshotId?, itemTypes[], compositionProfiles[]
ItemCapabilityBlocker:
  {code:"truth.snapshot_unavailable"}
  {code:"truth.evidence_missing", queryIndex:u32}
StoredItemDefinition:
  definitionHash, definition
```

Pack v5 descriptor guards must validate `localizationFields`, `referenceSlots`, optional
`resourceProfileField`, `resourceProfiles` and bounded `compositionProfiles`. ItemDefinition v2
guards require field-keyed localization maps, `referenceBindings`, optional typed
`compositionProfile`, exact camelCase enum payload fields and schemaVersion 2. A TypeScript union
or `as` cast does not replace these runtime checks.

Malformed values map through the fixed local `toActionableFailure(undefined)` fallback. Do not
render `String(error)`, accept `query_index`, or cast IPC data directly to the target interface.

Resource IPC follows the same rule. Runtime guards validate `ResourceCatalog`, ResourceAsset v2,
every immutable version/blob/provenance, bounded PNG `ResourcePreview`, and select results. A data
URL must begin with `data:image/png;base64,`; absolute or relative workspace paths are never a
preview transport. Every `ResourceRoleDescriptor` requires boolean `packDefaultAvailable`; absence
is a malformed IPC response, not equivalent to `false`. A Pack default prepare source has the exact
wire shape `{kind:"pack_default"}` and never carries `sourcePath`. Single v3 carries
`definition: StoredItemDefinition` and has no
`selectedResources` field.

Composition IPC guards validate CompositionDraft v2 identity/revision/Pack hash/root/profile,
optional paired `sourceExecutionGraphId`/`validatedContentDigest`, field-keyed nodes and exact
ItemDefinition v2 shapes. ExecutionGraph guards validate only the bounded View projection
(`executionGraphId`, revision/status including `validating`/`repairing`, Run IDs,
progress/current node, safe failure, aggregate `repairRound`, nullable exact `feedbackPhase`
(`output_contract | generated_content`), action flags and bounded `adjustableItems[]` containing only
`itemId + definitionHash`);
React must not receive or decode checkpoints, Blueprint payloads or commit intent. Confirmation
guards validate the Draft ref, every StoredItemDefinition and the confirmation digest. A TypeScript
interface or direct cast is not accepted for list/get/update/confirm/status responses.

Composition targeted retry uses the generic Feature submission boundary with exact camelCase
`draftId`, `expectedRevision`, `itemId` and `instructions`. React derives this request from the
currently loaded Draft; it does not send definition content, Resources or caller-authored hashes.

`RunFailure.details`, when present, must pass the generic `VersionedPayload` runtime guard.
Composition-specific display additionally requires schema
`feature.composition-plan-failure-details` v1 and validates bounded reason/count/identifier fields;
unknown schemas or malformed payloads fall back to the already validated failure stage.
The only interpreted payload fields are `reasonCode`, `expectedCount`, `actualCount`, `itemId`,
`itemType` and `slotId`. `reasonCode` is a 1-64 character qualified identifier; Item/type/slot IDs
are 1-128 character qualified identifiers; counts are integers in `0..=u32::MAX`. Expected/actual
are displayed only as a valid pair. Every other field is ignored, never stringified into the UI.

Composition generation submits request schema v7 with an exact `StoredItemDefinition`, optional
Draft ref, optional nested Package request, immutable internal `repairPolicy` and optional backend-authored tagged
`execution` / `adjustment`. Ordinary React always submits `{kind:"until_passed"}`
and does not expose feedback or semantic-budget controls. Each planned Behavior owns a baseline
request; the 20-round policy limits only additional shared semantic feedback. The only
execution forms are `start {executionGraphId}` and
`resume {executionGraphId, expectedRevision, previousRunId}`; the request builder for an initial
user submission omits both execution and adjustment. `submit_composition_item_feedback` accepts only the
selected succeeded Graph revision plus one `itemId + expectedDefinitionHash + expectedBehaviorSha256 + instruction`; Tauri authors the
instruction hash and timestamp. A succeeded source derives a new Graph/Run/Artifact version, while
the source evidence remains immutable. Result and Artifact extension v4 include the graph ID,
per-Item Behavior request/hash, RenderedItemBundle hash and Adapter identity. React does not decode
these checkpoints or present a Behavior/Render node as an independently published Artifact.
Project Build request v2 carries only optional `outputRelativeRoot`; Pack-owned isolation property
names never enter the React request.

Batch/Complex feature payloads are decoded only after matching exact schema identities:

```text
Batch request v4 / result v2
Complex request v3 / result v2
```

The feature-specific decoder in `src/pages/batchGenerationModel.ts` checks StoredItemDefinition
hashes, terminal per-item status, Plan/Single result shape,
counter invariants, optional child Run IDs and all-or-none Complex delivery fields. A malformed
payload returns the local actionable fallback; UI code must not cast `RunRecord.result.payload`.

---

## Common Patterns

<!-- Type utilities, generics, type guards -->

(To be filled by the team)

---

## Forbidden Patterns

<!-- any, type assertions, etc. -->

(To be filled by the team)
