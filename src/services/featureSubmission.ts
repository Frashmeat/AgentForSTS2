export function buildFeatureSubmission(
  featureId: string,
  schemaId: string,
  payload: Record<string, unknown>,
  schemaVersion = 1,
  sourcePath?: string,
) {
  return {
    featureId,
    request: { schema: { id: schemaId, version: schemaVersion }, payload },
    ...(sourcePath ? { sourcePath } : {}),
  };
}
