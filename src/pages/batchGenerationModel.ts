import type {
  BatchDefinitionItem,
  BatchGenerateRequest,
  BatchGenerateResult,
  BatchItemResult,
  ComplexGenerateRequest,
  ComplexGenerateResult,
  PlanItem,
  ProjectPackageRequest,
  RunRecord,
  SingleGenerateResult,
  StoredItemDefinition,
} from "@/services/tauriApi";

export interface GenerationComposition {
  request: BatchGenerateRequest;
  result: BatchGenerateResult;
  childRunIds: string[];
  deliverySkipped: boolean;
}

export function buildBatchRequest(
  modId: string,
  definitions: StoredItemDefinition[],
  failFast: boolean,
): BatchGenerateRequest {
  return {
    modId,
    items: definitions.map((definition) => ({
      artifactId: definition.definition.itemId,
      definition,
    })),
    failFast,
  };
}

export function buildComplexRequest(
  batch: BatchGenerateRequest,
  packageRequest: ProjectPackageRequest,
): ComplexGenerateRequest {
  return { batch, package: packageRequest };
}

export function decodeGenerationComposition(run: RunRecord): GenerationComposition | null {
  if (run.status !== "succeeded" || !run.result) return null;
  if (run.featureId === "mod.generate.batch") {
    if (
      run.request.schema.id !== "feature.mod-generate-batch-request" ||
      run.request.schema.version !== 4 ||
      run.result.schema.id !== "feature.mod-generate-batch-result" ||
      run.result.schema.version !== 2 ||
      !isBatchGenerateRequest(run.request.payload) ||
      !isBatchGenerateResult(run.result.payload)
    ) {
      return null;
    }
    return {
      request: run.request.payload,
      result: run.result.payload,
      childRunIds: itemChildRunIds(run.result.payload.items),
      deliverySkipped: false,
    };
  }
  if (
    run.featureId !== "mod.generate.complex" ||
    run.request.schema.id !== "feature.mod-generate-complex-request" ||
    run.request.schema.version !== 3 ||
    run.result.schema.id !== "feature.mod-generate-complex-result" ||
    run.result.schema.version !== 2 ||
    !isComplexGenerateRequest(run.request.payload) ||
    !isComplexGenerateResult(run.result.payload)
  ) {
    return null;
  }
  const result = run.result.payload;
  return {
    request: run.request.payload.batch,
    result: result.batch,
    childRunIds: [
      result.batchRunId,
      ...itemChildRunIds(result.batch.items),
      ...(result.buildRunId ? [result.buildRunId] : []),
      ...(result.packageRunId ? [result.packageRunId] : []),
    ],
    deliverySkipped: !result.buildRunId || !result.packageRunId,
  };
}

export function failedRequestItems(composition: GenerationComposition): BatchDefinitionItem[] {
  const failed = new Set(
    composition.result.items
      .filter((item) => item.status === "failed")
      .map((item) => `${item.itemId}:${item.definitionHash}`),
  );
  return composition.request.items.filter((item) =>
    failed.has(`${item.definition.definition.itemId}:${item.definition.definitionHash}`),
  );
}

export function unprocessedRequestItems(
  composition: GenerationComposition,
): BatchDefinitionItem[] {
  const processed = new Set(
    composition.result.items.map((item) => `${item.itemId}:${item.definitionHash}`),
  );
  return composition.request.items.filter((item) =>
    !processed.has(`${item.definition.definition.itemId}:${item.definition.definitionHash}`),
  );
}

function itemChildRunIds(items: BatchItemResult[]): string[] {
  return items.flatMap((item) => [
    item.planRunId,
    ...(item.generationRunId ? [item.generationRunId] : []),
  ]);
}

function isBatchGenerateRequest(value: unknown): value is BatchGenerateRequest {
  return isRecord(value) &&
    typeof value.modId === "string" &&
    typeof value.failFast === "boolean" &&
    Array.isArray(value.items) &&
    value.items.length > 0 &&
    value.items.every(isBatchDefinitionItem);
}

function isBatchDefinitionItem(value: unknown): value is BatchDefinitionItem {
  return isRecord(value) &&
    typeof value.artifactId === "string" &&
    isStoredItemDefinition(value.definition);
}

function isBatchGenerateResult(value: unknown): value is BatchGenerateResult {
  if (!isRecord(value) || !Array.isArray(value.items) || !value.items.every(isBatchItemResult)) {
    return false;
  }
  const { total, processed, succeeded, failed } = value;
  if (![total, processed, succeeded, failed].every(isNonNegativeInteger)) return false;
  return (processed as number) === value.items.length &&
    (succeeded as number) + (failed as number) === processed &&
    (processed as number) <= (total as number);
}

function isBatchItemResult(value: unknown): value is BatchItemResult {
  if (!isRecord(value)) return false;
  return typeof value.itemId === "string" &&
    isSha256(value.definitionHash) &&
    typeof value.planRunId === "string" &&
    (value.generationRunId === undefined || value.generationRunId === null || typeof value.generationRunId === "string") &&
    isTerminalStatus(value.status) &&
    (value.plan === undefined || value.plan === null || isPlanItem(value.plan)) &&
    (value.result === undefined || value.result === null || isSingleGenerateResult(value.result)) &&
    (value.failureCode === undefined || value.failureCode === null || typeof value.failureCode === "string") &&
    (value.status === "succeeded"
      ? value.plan !== undefined && value.result !== undefined
      : value.status === "failed"
        ? typeof value.failureCode === "string"
        : value.result === undefined || value.result === null);
}

function isComplexGenerateRequest(value: unknown): value is ComplexGenerateRequest {
  return isRecord(value) && isBatchGenerateRequest(value.batch) && isProjectPackageRequest(value.package);
}

function isComplexGenerateResult(value: unknown): value is ComplexGenerateResult {
  if (!isRecord(value) || typeof value.batchRunId !== "string" || !isBatchGenerateResult(value.batch)) {
    return false;
  }
  const buildPresent = typeof value.buildRunId === "string" && isRecord(value.build);
  const packagePresent = typeof value.packageRunId === "string" && isRecord(value.package);
  const deliveryAbsent =
    (value.buildRunId === undefined || value.buildRunId === null) &&
    (value.build === undefined || value.build === null) &&
    (value.packageRunId === undefined || value.packageRunId === null) &&
    (value.package === undefined || value.package === null);
  return (buildPresent && packagePresent) || deliveryAbsent;
}

function isSingleGenerateResult(value: unknown): value is SingleGenerateResult {
  return isRecord(value) &&
    typeof value.artifactManifestRef === "string" &&
    isSha256(value.manifestSha256) &&
    isNonNegativeInteger(value.generatedFileCount) &&
    typeof value.validationPrimitive === "string" &&
    isStringArray(value.acceptanceNotes);
}

function isPlanItem(value: unknown): value is PlanItem {
  return isRecord(value) &&
    typeof value.itemId === "string" &&
    typeof value.itemType === "string" &&
    typeof value.name === "string" &&
    typeof value.summary === "string" &&
    isStringArray(value.behaviorIntent) &&
    isStringArray(value.implementationConstraints) &&
    isStringArray(value.evidenceRequirements) &&
    isStringArray(value.requiredResourceRoles) &&
    isStringArray(value.acceptanceCriteria);
}

function isProjectPackageRequest(value: unknown): value is ProjectPackageRequest {
  return isRecord(value) &&
    typeof value.artifactId === "string" &&
    typeof value.modId === "string" &&
    typeof value.sourceRelativeRoot === "string" &&
    typeof value.outputRelativePath === "string" &&
    (value.compressionLevel === undefined || value.compressionLevel === null || isNonNegativeInteger(value.compressionLevel));
}

function isStoredItemDefinition(value: unknown): value is StoredItemDefinition {
  return isRecord(value) && isSha256(value.definitionHash) && isItemDefinition(value.definition);
}

function isItemDefinition(value: unknown): value is StoredItemDefinition["definition"] {
  return isRecord(value) &&
    value.schemaVersion === 1 &&
    typeof value.itemId === "string" &&
    typeof value.itemType === "string" &&
    isRecord(value.canonicalFields) &&
    isStringArray(value.behaviorIntent) &&
    isRecord(value.localizations) &&
    isRecord(value.resourceBindings);
}

function isTerminalStatus(value: unknown): value is BatchItemResult["status"] {
  return value === "succeeded" || value === "failed" || value === "cancelled";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isNonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}

function isSha256(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}
