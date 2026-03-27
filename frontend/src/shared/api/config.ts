export interface AppConfig {
  default_project_root?: string;
  [key: string]: unknown;
}

export async function loadAppConfig(): Promise<AppConfig> {
  const response = await fetch("/api/config");
  return response.json();
}
