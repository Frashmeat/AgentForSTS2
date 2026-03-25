export interface WorkflowMigrationFlags {
  use_modular_single_workflow: boolean;
  use_modular_batch_workflow: boolean;
  use_unified_ws_contract: boolean;
}

export interface AppConfig {
  default_project_root?: string;
  migration?: Partial<WorkflowMigrationFlags>;
  [key: string]: unknown;
}

const DEFAULT_MIGRATION_FLAGS: WorkflowMigrationFlags = {
  use_modular_single_workflow: false,
  use_modular_batch_workflow: false,
  use_unified_ws_contract: false,
};

export function resolveMigrationFlags(config?: Pick<AppConfig, "migration"> | null): WorkflowMigrationFlags {
  return {
    ...DEFAULT_MIGRATION_FLAGS,
    ...(config?.migration ?? {}),
  };
}

export async function loadAppConfig(): Promise<AppConfig> {
  const response = await fetch("/api/config");
  return response.json();
}
