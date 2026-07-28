import type { SettingsPatch, SettingsSnapshot } from "@/services/tauriApi";

export interface FormState {
  llmProvider: string;
  llmModel: string;
  llmBaseUrl: string;
  llmApiKey: string;
  llmApiKeyTouched: boolean;
  igProvider: string;
  igModel: string;
  igBaseUrl: string;
  igSize: string;
  igProtocol: string;
  igApiKey: string;
  igApiKeyTouched: boolean;
  rtGithubToken: string;
  rtGithubTokenTouched: boolean;
}

export function formFromSnapshot(s: SettingsSnapshot): FormState {
  return {
    llmProvider: s.llm.provider,
    llmModel: s.llm.model,
    llmBaseUrl: s.llm.baseUrl,
    llmApiKey: "",
    llmApiKeyTouched: false,
    igProvider: s.imageGen.provider,
    igModel: s.imageGen.model,
    igBaseUrl: s.imageGen.baseUrl,
    igSize: s.imageGen.size,
    igProtocol: s.imageGen.protocol || "auto",
    igApiKey: "",
    igApiKeyTouched: false,
    rtGithubToken: "",
    rtGithubTokenTouched: false,
  };
}

export function buildPatch(
  form: FormState,
  original: SettingsSnapshot,
): SettingsPatch {
  const patch: SettingsPatch = {};
  const llm: NonNullable<SettingsPatch["llm"]> = {};
  if (form.llmProvider !== original.llm.provider) llm.provider = form.llmProvider;
  if (form.llmModel !== original.llm.model) llm.model = form.llmModel;
  if (form.llmBaseUrl !== original.llm.baseUrl) llm.base_url = form.llmBaseUrl;
  if (form.llmApiKeyTouched) llm.api_key = form.llmApiKey;
  if (Object.keys(llm).length > 0) patch.llm = llm;

  const imageGen: NonNullable<SettingsPatch["image_gen"]> = {};
  if (form.igProvider !== original.imageGen.provider) imageGen.provider = form.igProvider;
  if (form.igModel !== original.imageGen.model) imageGen.model = form.igModel;
  if (form.igBaseUrl !== original.imageGen.baseUrl) imageGen.base_url = form.igBaseUrl;
  if (form.igSize !== original.imageGen.size) imageGen.size = form.igSize;
  if (form.igProtocol !== (original.imageGen.protocol || "auto")) {
    imageGen.protocol = form.igProtocol;
  }
  if (form.igApiKeyTouched) imageGen.api_key = form.igApiKey;
  if (Object.keys(imageGen).length > 0) patch.image_gen = imageGen;

  if (form.rtGithubTokenTouched) {
    patch.runtime_workstation = { github_token: form.rtGithubToken };
  }
  return patch;
}
