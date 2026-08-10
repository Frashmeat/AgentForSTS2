import type {
  CompositionConfirmation,
  CompositionDraft,
  CompositionDraftNode,
  ItemCompositionProfile,
  ItemDefinition,
  ItemFieldValue,
  ItemLocalization,
  ItemReferenceBinding,
  ItemResourceBinding,
  StoredItemDefinition,
} from "./tauriApi";

export function isCompositionDraft(value: unknown): value is CompositionDraft {
  return (
    isRecord(value) &&
    value.schemaVersion === 2 &&
    typeof value.draftId === "string" &&
    isPositiveInteger(value.revision) &&
    typeof value.gamePackId === "string" &&
    isSha256(value.gamePackSha256) &&
    typeof value.rootItemId === "string" &&
    isItemCompositionProfile(value.profile) &&
    isRecord(value.nodes) &&
    (value.sourceExecutionGraphId === undefined ||
      value.sourceExecutionGraphId === null ||
      typeof value.sourceExecutionGraphId === "string") &&
    (value.validatedContentDigest === undefined ||
      value.validatedContentDigest === null ||
      typeof value.validatedContentDigest === "string") &&
    Object.values(value.nodes).every(isCompositionDraftNode) &&
    typeof value.createdAt === "string" &&
    typeof value.updatedAt === "string"
  );
}

export function isCompositionConfirmation(value: unknown): value is CompositionConfirmation {
  return (
    isRecord(value) &&
    isRecord(value.draft) &&
    typeof value.draft.draftId === "string" &&
    isPositiveInteger(value.draft.revision) &&
    Array.isArray(value.definitions) &&
    value.definitions.every(isStoredItemDefinition) &&
    isSha256(value.confirmationDigest)
  );
}

function isCompositionDraftNode(value: unknown): value is CompositionDraftNode {
  return (
    isRecord(value) &&
    isItemDefinition(value.definition) &&
    (value.expectedCurrentDefinitionHash === undefined ||
      value.expectedCurrentDefinitionHash === null ||
      isSha256(value.expectedCurrentDefinitionHash))
  );
}

export function isStoredItemDefinition(value: unknown): value is StoredItemDefinition {
  return isRecord(value) && isSha256(value.definitionHash) && isItemDefinition(value.definition);
}

export function isItemDefinition(value: unknown): value is ItemDefinition {
  return (
    isRecord(value) &&
    value.schemaVersion === 2 &&
    typeof value.itemId === "string" &&
    typeof value.itemType === "string" &&
    isRecord(value.canonicalFields) &&
    Object.values(value.canonicalFields).every(isItemFieldValue) &&
    isStringArray(value.behaviorIntent) &&
    isRecord(value.localizations) &&
    Object.values(value.localizations).every(isItemLocalization) &&
    isRecord(value.resourceBindings) &&
    Object.values(value.resourceBindings).every(isItemResourceBinding) &&
    isRecord(value.referenceBindings) &&
    Object.values(value.referenceBindings).every(
      (bindings) => Array.isArray(bindings) && bindings.every(isItemReferenceBinding),
    ) &&
    (value.compositionProfile === undefined ||
      value.compositionProfile === null ||
      isItemCompositionProfile(value.compositionProfile))
  );
}

function isItemFieldValue(value: unknown): value is ItemFieldValue {
  if (!isRecord(value) || typeof value.kind !== "string") return false;
  switch (value.kind) {
    case "text":
    case "choice":
      return typeof value.value === "string";
    case "integer":
      return Number.isInteger(value.value);
    case "boolean":
      return typeof value.value === "boolean";
    case "string_list":
      return isStringArray(value.value);
    default:
      return false;
  }
}

function isItemLocalization(value: unknown): value is ItemLocalization {
  return (
    isRecord(value) &&
    isStringRecord(value.fields) &&
    (value.status === "confirmed" || value.status === "outdated") &&
    (value.translatedFrom === undefined ||
      value.translatedFrom === null ||
      typeof value.translatedFrom === "string")
  );
}

function isItemReferenceBinding(value: unknown): value is ItemReferenceBinding {
  if (!isRecord(value) || typeof value.itemId !== "string") return false;
  return value.kind === "identity"
    ? typeof value.expectedItemType === "string"
    : value.kind === "pinned" &&
        isSha256(value.definitionHash) &&
        isPositiveInteger(value.quantity);
}

function isItemCompositionProfile(value: unknown): value is ItemCompositionProfile {
  if (
    !isRecord(value) ||
    typeof value.compositionId !== "string" ||
    !isRecord(value.source) ||
    !isRecord(value.parameters) ||
    !Object.values(value.parameters).every(Number.isInteger)
  ) {
    return false;
  }
  return value.source.kind === "preset"
    ? typeof value.source.profileId === "string"
    : value.source.kind === "custom" && typeof value.source.baseProfileId === "string";
}

function isItemResourceBinding(value: unknown): value is ItemResourceBinding {
  return isRecord(value) && typeof value.resourceId === "string" && isSha256(value.selectedVersion);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isStringRecord(value: unknown): value is Record<string, string> {
  return isRecord(value) && Object.values(value).every((item) => typeof item === "string");
}

function isPositiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0;
}

function isSha256(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}
