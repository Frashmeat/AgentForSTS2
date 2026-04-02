interface RunApprovalActionOptions<TResult> {
  actionId: string;
  action: (actionId: string) => Promise<TResult>;
  onBusyChange: (actionId: string | null) => void;
  onSuccess: (result: TResult) => void;
  onError: (message: string) => void;
}

function resolveErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function runApprovalAction<TResult>(
  options: RunApprovalActionOptions<TResult>,
): Promise<TResult | undefined> {
  const {
    actionId,
    action,
    onBusyChange,
    onSuccess,
    onError,
  } = options;

  onBusyChange(actionId);
  try {
    const result = await action(actionId);
    onSuccess(result);
    return result;
  } catch (error) {
    onError(resolveErrorMessage(error));
    return undefined;
  } finally {
    onBusyChange(null);
  }
}
