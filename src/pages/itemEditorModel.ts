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
    schemaVersion: 1,
    itemId,
    itemType: capability.descriptor.id,
    canonicalFields,
    behaviorIntent: [],
    localizations: {},
    resourceBindings: {},
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

export function setLocalizationText(
  definition: ItemDefinition,
  locale: string,
  primaryLocale: string,
  patch: Partial<Pick<ItemLocalization, "name" | "description">>,
): ItemDefinition {
  const previous = definition.localizations[locale];
  const name = patch.name ?? previous?.name ?? "";
  const description = patch.description ?? previous?.description ?? "";
  const changed =
    previous === undefined ||
    previous.name !== name ||
    previous.description !== description;
  const current: ItemLocalization = {
    name,
    description,
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
): ItemDefinition {
  const localization = definition.localizations[locale];
  if (!localization?.name.trim() || !localization.description.trim()) return definition;
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
    if (!value.name.trim() || !value.description.trim()) {
      issues.push(`${locale} localization is incomplete.`);
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
