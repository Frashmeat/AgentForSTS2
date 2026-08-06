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
and checked by runtime guards in `src/services/tauriApi.ts`. A TypeScript interface alone is not
proof of a Tauri response shape.

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

Pack v4 descriptor guards must validate `localizationFields`, `referenceSlots`, optional
`resourceProfileField`, `resourceProfiles` and bounded `compositionProfiles`. ItemDefinition v2
guards require field-keyed localization maps, `referenceBindings`, optional typed
`compositionProfile`, exact camelCase enum payload fields and schemaVersion 2. A TypeScript union
or `as` cast does not replace these runtime checks.

Malformed values map through the fixed local `toActionableFailure(undefined)` fallback. Do not
render `String(error)`, accept `query_index`, or cast IPC data directly to the target interface.

Resource IPC follows the same rule. Runtime guards validate `ResourceCatalog`, ResourceAsset v2,
every immutable version/blob/provenance, bounded PNG `ResourcePreview`, and select results. A data
URL must begin with `data:image/png;base64,`; absolute or relative workspace paths are never a
preview transport. Single v3 carries `definition: StoredItemDefinition` and has no
`selectedResources` field.

Composition IPC guards validate CompositionDraft v1 identity/revision/Pack hash/root/profile,
field-keyed nodes and exact ItemDefinition v2 shapes. Confirmation guards validate the Draft ref,
every StoredItemDefinition and the confirmation digest. A TypeScript interface or direct cast is not
accepted for list/get/update/confirm responses.

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
