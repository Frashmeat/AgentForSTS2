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
  gamePackId, gamePackSha256, truthSnapshotId?, itemTypes[]
ItemCapabilityBlocker:
  {code:"truth.snapshot_unavailable"}
  {code:"truth.evidence_missing", queryIndex:u32}
StoredItemDefinition:
  definitionHash, definition
```

Malformed values map through the fixed local `toActionableFailure(undefined)` fallback. Do not
render `String(error)`, accept `query_index`, or cast IPC data directly to the target interface.

---

## Common Patterns

<!-- Type utilities, generics, type guards -->

(To be filled by the team)

---

## Forbidden Patterns

<!-- any, type assertions, etc. -->

(To be filled by the team)
