import type {
  ItemDefinition,
  ItemFieldValue,
  ItemLocalization,
  ItemTypeCapability,
  ItemTypeDescriptor,
} from "@/services/tauriApi";

export function localizedLabel(
  values: Record<string, string>,
  locale = "zhs",
): string {
  return values[locale] ?? values.eng ?? Object.values(values)[0] ?? "";
}

export function createItemDraft(
  capability: ItemTypeCapability,
  itemId: string,
): ItemDefinition {
  const canonicalFields: Record<string, ItemFieldValue> = {};
  for (const field of capability.descriptor.fields) {
    if (field.value.kind === "choice" && field.required && field.value.options[0]) {
      canonicalFields[field.id] = {
        kind: "choice",
        value: field.value.options[0].value,
      };
    } else if (field.value.kind === "boolean" && field.required) {
      canonicalFields[field.id] = { kind: "boolean", value: false };
    } else if (field.value.kind === "integer" && field.required) {
      canonicalFields[field.id] = { kind: "integer", value: field.value.min };
    }
  }
  return {
    schemaVersion: 2,
    itemId,
    itemType: capability.descriptor.id,
    canonicalFields,
    behaviorIntent: [],
    localizations: {},
    resourceBindings: {},
    referenceBindings: {},
  };
}

export function setFieldValue(
  definition: ItemDefinition,
  fieldId: string,
  value?: ItemFieldValue,
): ItemDefinition {
  const canonicalFields = { ...definition.canonicalFields };
  if (value === undefined) delete canonicalFields[fieldId];
  else canonicalFields[fieldId] = value;
  return { ...definition, canonicalFields };
}

export function setBehaviorText(
  definition: ItemDefinition,
  value: string,
): ItemDefinition {
  return {
    ...definition,
    behaviorIntent: value
      .split("\n")
      .map((item) => item.trim())
      .filter(Boolean),
  };
}

export function setLocalizationFieldText(
  definition: ItemDefinition,
  locale: string,
  primaryLocale: string,
  fieldId: string,
  value: string,
): ItemDefinition {
  const previous = definition.localizations[locale];
  const fields = { ...previous?.fields, [fieldId]: value };
  const changed = previous === undefined || previous.fields[fieldId] !== value;
  const current: ItemLocalization = {
    fields,
    status:
      locale === primaryLocale
        ? "confirmed"
        : changed
          ? "outdated"
          : previous?.status ?? "outdated",
    translatedFrom:
      locale === primaryLocale ? undefined : previous?.translatedFrom ?? primaryLocale,
  };
  const localizations = { ...definition.localizations, [locale]: current };
  if (locale === primaryLocale && changed) {
    for (const [candidateLocale, candidate] of Object.entries(localizations)) {
      if (candidateLocale !== locale && candidate.translatedFrom === locale) {
        localizations[candidateLocale] = { ...candidate, status: "outdated" };
      }
    }
  }
  return { ...definition, localizations };
}

export function confirmLocalization(
  definition: ItemDefinition,
  locale: string,
  descriptor: ItemTypeDescriptor,
): ItemDefinition {
  const localization = definition.localizations[locale];
  if (!localization || descriptor.localizationFields.some(
    (field) => field.required && !localization.fields[field.id]?.trim(),
  )) return definition;
  return {
    ...definition,
    localizations: {
      ...definition.localizations,
      [locale]: { ...localization, status: "confirmed" },
    },
  };
}

export function draftIssues(
  definition: ItemDefinition,
  descriptor: ItemTypeDescriptor,
): string[] {
  const issues: string[] = [];
  if (!/^[a-z][a-z0-9_-]{0,127}$/.test(definition.itemId)) {
    issues.push("Item ID must be a lowercase slug.");
  }
  for (const fieldId of Object.keys(definition.canonicalFields)) {
    if (!descriptor.fields.some((field) => field.id === fieldId)) {
      issues.push(`Unknown field: ${fieldId}`);
    }
  }
  for (const [locale, value] of Object.entries(definition.localizations)) {
    for (const fieldId of Object.keys(value.fields)) {
      if (!descriptor.localizationFields.some((field) => field.id === fieldId)) {
        issues.push(`${locale} has unknown localization field: ${fieldId}.`);
      }
    }
    for (const field of descriptor.localizationFields) {
      if (field.required && !value.fields[field.id]?.trim()) {
        issues.push(`${locale} ${localizedLabel(field.displayNames)} is incomplete.`);
      }
    }
    if (
      value.translatedFrom &&
      definition.localizations[value.translatedFrom] === undefined
    ) {
      issues.push(
        `${locale} translation source ${value.translatedFrom} must be created first.`,
      );
    }
  }
  return issues;
}

export function requiredResourceRoles(
  definition: ItemDefinition,
  descriptor: ItemTypeDescriptor,
): string[] {
  if (descriptor.resourceProfiles.length === 0) return [];
  if (!descriptor.resourceProfileField) {
    return descriptor.resourceProfiles[0]?.requiredResourceRoles ?? [];
  }
  const selected = definition.canonicalFields[descriptor.resourceProfileField];
  if (selected?.kind !== "choice") return [];
  return descriptor.resourceProfiles.find((profile) => profile.id === selected.value)
    ?.requiredResourceRoles ?? [];
}

export function capabilityReason(capability: ItemTypeCapability): string {
  if (capability.ready) return "Ready";
  if (capability.blockers.some((item) => item.code === "truth.snapshot_unavailable")) {
    return "Truth Snapshot unavailable";
  }
  const missing = capability.blockers.filter(
    (item) => item.code === "truth.evidence_missing",
  ).length;
  return `${missing} Truth evidence group${missing === 1 ? "" : "s"} missing`;
}
