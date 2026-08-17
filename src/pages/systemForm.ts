import type { SettingsPatch, SettingsSnapshot } from "@/services/tauriApi";

export interface FormState {
  llmProvider: string;
  llmModel: string;
  llmBaseUrl: string;
  llmCustomPrompt: string;
  llmMaxOutputTokens: string;
  llmApiKey: string;
  llmApiKeyTouched: boolean;
  imageProvider: string;
  imageModel: string;
  imageBaseUrl: string;
  imageSize: string;
  imageProtocol: string;
  imageApiKey: string;
  imageApiKeyTouched: boolean;
  githubToken: string;
  githubTokenTouched: boolean;
  sts2DllPath: string;
  godotExePath: string;
}

export function formFromSnapshot(snapshot: SettingsSnapshot): FormState {
  return {
    llmProvider: snapshot.llm.provider,
    llmModel: snapshot.llm.model,
    llmBaseUrl: snapshot.llm.baseUrl,
    llmCustomPrompt: snapshot.llm.customPrompt ?? "",
    llmMaxOutputTokens: snapshot.llm.maxOutputTokens?.toString() ?? "",
    llmApiKey: "",
    llmApiKeyTouched: false,
    imageProvider: snapshot.imageGen.provider,
    imageModel: snapshot.imageGen.model,
    imageBaseUrl: snapshot.imageGen.baseUrl,
    imageSize: snapshot.imageGen.size,
    imageProtocol: snapshot.imageGen.protocol,
    imageApiKey: "",
    imageApiKeyTouched: false,
    githubToken: "",
    githubTokenTouched: false,
    sts2DllPath: snapshot.knowledge.sts2DllPath,
    godotExePath: snapshot.toolchain.godotExePath,
  };
}

export function buildPatch(form: FormState, original: SettingsSnapshot): SettingsPatch {
  const maxOutputTokensError = validateMaxOutputTokens(form.llmMaxOutputTokens);
  if (maxOutputTokensError) throw new Error(maxOutputTokensError);
  const patch: SettingsPatch = {};
  const llm: NonNullable<SettingsPatch["llm"]> = {};
  if (form.llmProvider !== original.llm.provider) llm.provider = form.llmProvider;
  if (form.llmModel !== original.llm.model) llm.model = form.llmModel;
  if (form.llmBaseUrl !== original.llm.baseUrl) llm.baseUrl = form.llmBaseUrl;
  if (form.llmCustomPrompt !== (original.llm.customPrompt ?? "")) llm.customPrompt = form.llmCustomPrompt;
  const maxOutputTokens = form.llmMaxOutputTokens.trim() === ""
    ? null
    : Number(form.llmMaxOutputTokens.trim());
  if (maxOutputTokens !== original.llm.maxOutputTokens) llm.maxOutputTokens = maxOutputTokens;
  if (form.llmApiKeyTouched) llm.apiKey = form.llmApiKey;
  if (Object.keys(llm).length > 0) patch.llm = llm;

  const image: NonNullable<SettingsPatch["imageGen"]> = {};
  if (form.imageProvider !== original.imageGen.provider) image.provider = form.imageProvider;
  if (form.imageModel !== original.imageGen.model) image.model = form.imageModel;
  if (form.imageBaseUrl !== original.imageGen.baseUrl) image.baseUrl = form.imageBaseUrl;
  if (form.imageSize !== original.imageGen.size) image.size = form.imageSize;
  if (form.imageProtocol !== original.imageGen.protocol) image.protocol = form.imageProtocol;
  if (form.imageApiKeyTouched) image.apiKey = form.imageApiKey;
  if (Object.keys(image).length > 0) patch.imageGen = image;

  if (form.githubTokenTouched) patch.runtimeWorkstation = { githubToken: form.githubToken };
  if (form.sts2DllPath !== original.knowledge.sts2DllPath) patch.knowledge = { sts2DllPath: form.sts2DllPath };
  if (form.godotExePath !== original.toolchain.godotExePath) patch.toolchain = { godotExePath: form.godotExePath };
  return patch;
}

export function validateMaxOutputTokens(value: string): string | null {
  const normalized = value.trim();
  if (normalized === "") return null;
  if (!/^\d+$/.test(normalized)) return "Enter a whole number from 1 to 65,536.";
  const parsed = Number(normalized);
  return Number.isSafeInteger(parsed) && parsed >= 1 && parsed <= 65_536
    ? null
    : "Enter a whole number from 1 to 65,536.";
}
