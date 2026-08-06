import type {
  CompositionDraft,
  CompositionDraftNode,
  CompositionPlanRequest,
  CompositionRetryNodeRequest,
  CompositionGenerateRequest,
  CompositionProfileSet,
  ItemCompositionSource,
  ProjectPackageRequest,
  StoredItemDefinition,
} from "@/services/tauriApi";

export type CompositionProfileChoice =
  | { kind: "preset"; profileId: string }
  | { kind: "custom"; baseProfileId: string };

export type DraftNodeStatus = "new" | "replacement";

export interface DraftNodeRow {
  itemId: string;
  node: CompositionDraftNode;
  status: DraftNodeStatus;
}

export function defaultProfileChoice(profile: CompositionProfileSet): CompositionProfileChoice {
  return { kind: "preset", profileId: profile.defaultProfile };
}

export function parametersForChoice(
  profile: CompositionProfileSet,
  choice: CompositionProfileChoice,
  custom?: Record<string, number>,
): Record<string, number> {
  const profileId = choice.kind === "preset" ? choice.profileId : choice.baseProfileId;
  const base = profile.profiles.find((candidate) => candidate.id === profileId);
  if (!base) return {};
  return choice.kind === "custom" ? { ...base.values, ...custom } : { ...base.values };
}

export function profileIssues(
  profile: CompositionProfileSet,
  parameters: Record<string, number>,
): string[] {
  const issues: string[] = [];
  const expectedIds = new Set(profile.parameters.map((parameter) => parameter.id));
  if (Object.keys(parameters).length !== expectedIds.size) issues.push("Profile parameters are incomplete.");
  let nodes = profile.baseNodeCount;
  for (const parameter of profile.parameters) {
    const value = parameters[parameter.id];
    if (!Number.isInteger(value) || value < parameter.min || value > parameter.max) {
      issues.push(`${displayName(parameter.displayNames)} must be between ${parameter.min} and ${parameter.max}.`);
      continue;
    }
    nodes += value * parameter.nodeWeight;
  }
  if (nodes > profile.maxNodes) issues.push(`The profile exceeds the ${profile.maxNodes}-node limit.`);
  for (const constraint of profile.constraints) {
    if (parameters[constraint.left] > parameters[constraint.right]) {
      issues.push(`${constraint.left} must not exceed ${constraint.right}.`);
    }
  }
  return issues;
}

export function buildCompositionPlanRequest(
  draftId: string,
  profile: CompositionProfileSet,
  choice: CompositionProfileChoice,
  parameters: Record<string, number>,
  concept: string,
): CompositionPlanRequest {
  const source: ItemCompositionSource = choice.kind === "preset"
    ? { kind: "preset", profileId: choice.profileId }
    : { kind: "custom", baseProfileId: choice.baseProfileId };
  return {
    draftId: draftId.trim(),
    compositionId: profile.id,
    concept: concept.trim(),
    source,
    parameters: { ...parameters },
  };
}

export function buildCompositionRetryNodeRequest(
  draft: CompositionDraft,
  itemId: string,
  instructions: string,
): CompositionRetryNodeRequest {
  return {
    draftId: draft.draftId,
    expectedRevision: draft.revision,
    itemId,
    instructions: instructions.trim(),
  };
}

export function draftRows(
  draft: CompositionDraft,
  itemType: string,
  status: "all" | DraftNodeStatus,
  query: string,
): DraftNodeRow[] {
  const normalized = query.trim().toLocaleLowerCase();
  return Object.entries(draft.nodes)
    .map(([itemId, node]) => ({
      itemId,
      node,
      status: node.expectedCurrentDefinitionHash ? "replacement" as const : "new" as const,
    }))
    .filter((row) => !itemType || row.node.definition.itemType === itemType)
    .filter((row) => status === "all" || row.status === status)
    .filter((row) => !normalized || row.itemId.toLocaleLowerCase().includes(normalized))
    .sort((left, right) => left.itemId.localeCompare(right.itemId));
}

export function pageRows<T>(rows: T[], page: number, pageSize: number): {
  rows: T[];
  page: number;
  pageCount: number;
} {
  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize));
  const boundedPage = Math.min(Math.max(1, page), pageCount);
  return {
    rows: rows.slice((boundedPage - 1) * pageSize, boundedPage * pageSize),
    page: boundedPage,
    pageCount,
  };
}

export function closedSelection(draft: CompositionDraft, selected: Set<string>): boolean {
  if (selected.size === 0) return false;
  for (const itemId of selected) {
    const node = draft.nodes[itemId];
    if (!node) return false;
    for (const bindings of Object.values(node.definition.referenceBindings)) {
      for (const binding of bindings) {
        if (binding.kind === "pinned" && !selected.has(binding.itemId)) return false;
      }
    }
  }
  return true;
}

export function compositionRoots(
  definitions: StoredItemDefinition[],
  profiles: CompositionProfileSet[],
): StoredItemDefinition[] {
  const rootTypes = new Set(profiles.map((profile) => profile.rootItemType));
  return definitions
    .filter((definition) =>
      definition.definition.compositionProfile !== undefined &&
      definition.definition.compositionProfile !== null &&
      rootTypes.has(definition.definition.itemType))
    .sort((left, right) => left.definition.itemId.localeCompare(right.definition.itemId));
}

export function buildCompositionGenerateRequest(
  artifactId: string,
  modId: string,
  root: StoredItemDefinition,
  draft: CompositionDraft | null,
  packageRequest: Pick<
    ProjectPackageRequest,
    "sourceRelativeRoot" | "outputRelativePath" | "compressionLevel"
  >,
): CompositionGenerateRequest {
  return {
    artifactId: artifactId.trim(),
    modId: modId.trim(),
    root,
    draft: draft && draft.rootItemId === root.definition.itemId
      ? { draftId: draft.draftId, revision: draft.revision }
      : null,
    package: {
      artifactId: artifactId.trim(),
      modId: modId.trim(),
      sourceRelativeRoot: packageRequest.sourceRelativeRoot.trim(),
      outputRelativePath: packageRequest.outputRelativePath.trim(),
      compressionLevel: packageRequest.compressionLevel,
    },
  };
}

export function displayName(names: Record<string, string>): string {
  return names.zhs ?? names.eng ?? Object.values(names)[0] ?? "Unnamed";
}
