// SettingsPanel 工作站标签的"配置加载 + 编辑 + 自动保存"状态机封装。
// 与 useDetectAppPathsTask / useRefreshKnowledgeTask / useServerPreferences 同模式：
// mountedRef 保护跨实例 + 自动淡出 notice + 闭包 save 用 700ms debounce。
// cfg schema 在后端松散维护，前端整体保留 any（审查 #8 严格化债务）。

import { useEffect, useRef, useState, type MutableRefObject } from "react";

import { loadAppConfig, testImageGenerationConfig, updateAppConfig } from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";
import { PROVIDER_MODELS } from "./settings-panel-helpers.ts";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function buildConfigSaveBody(cfg: any, llmKey: string, imgKey: string, imgSecret: string) {
  const body = structuredClone(cfg);
  if (llmKey.trim()) body.llm.api_key = llmKey.trim();
  if (imgKey.trim()) body.image_gen.api_key = imgKey.trim();
  if (imgSecret.trim()) body.image_gen.api_secret = imgSecret.trim();
  return body;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function buildConfigSaveSignature(cfg: any, llmKey: string, imgKey: string, imgSecret: string) {
  return JSON.stringify(buildConfigSaveBody(cfg, llmKey, imgKey, imgSecret));
}

export interface UseAppConfigEditorOptions {
  mountedRef: MutableRefObject<boolean>;
}

export interface UseAppConfigEditorResult {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  cfg: any;
  llmKey: string;
  imgKey: string;
  imgSecret: string;
  setLlmKey: (value: string) => void;
  setImgKey: (value: string) => void;
  setImgSecret: (value: string) => void;
  saving: boolean;
  saveError: string;
  saveNotice: string;
  hasSavedConfigOnce: boolean;
  configDirty: boolean;
  imageTestLoading: boolean;
  set: (path: string[], value: string | number) => void;
  handleProviderChange: (provider: string) => void;
  save: () => Promise<void>;
  handleTestImageGenerationConfig: () => Promise<void>;
}

export function useAppConfigEditor({ mountedRef }: UseAppConfigEditorOptions): UseAppConfigEditorResult {
  // 同上，cfg 留作 any（审查 #8 前端拆分时严格化）
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [cfg, setCfg] = useState<any>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [saveNotice, setSaveNotice] = useState("");
  const [hasSavedConfigOnce, setHasSavedConfigOnce] = useState(false);
  const [llmKey, setLlmKey] = useState("");
  const [imgKey, setImgKey] = useState("");
  const [imgSecret, setImgSecret] = useState("");
  const [imageTestLoading, setImageTestLoading] = useState(false);
  const lastSavedConfigSignatureRef = useRef("");
  const configSaveSignature = cfg ? buildConfigSaveSignature(cfg, llmKey, imgKey, imgSecret) : "";
  const configDirty = Boolean(cfg) && configSaveSignature !== lastSavedConfigSignatureRef.current;

  useEffect(() => {
    loadAppConfig().then((loaded) => {
      if (!mountedRef.current) {
        return;
      }
      setCfg(loaded);
      lastSavedConfigSignatureRef.current = buildConfigSaveSignature(loaded, "", "", "");
    });
    // mountedRef 是 ref 永不进 deps；首次 mount 加载一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!cfg || saving || !configDirty || saveError) {
      return;
    }

    const timer = setTimeout(() => {
      void save();
    }, 700);

    return () => {
      clearTimeout(timer);
    };
    // save 是组件内闭包函数，每次 render 都重新创建；放进 deps 会让 debounce 永远重置。
    // 这里依赖 state 而非 callback —— 真正影响是否触发 save 的是 cfg/saving/configDirty 等。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cfg, saving, configDirty, configSaveSignature, saveError]);

  useEffect(() => {
    if (!saveNotice) {
      return;
    }

    const timer = setTimeout(() => {
      setSaveNotice("");
    }, 2600);

    return () => {
      clearTimeout(timer);
    };
  }, [saveNotice]);

  function set(path: string[], value: string | number) {
    setSaveError("");
    setSaveNotice("");
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    setCfg((prev: any) => {
      if (!prev) {
        return prev;
      }
      let cur = prev;
      for (let i = 0; i < path.length - 1; i++) cur = cur[path[i]];
      if (cur[path[path.length - 1]] === value) {
        return prev;
      }
      const next = structuredClone(prev);
      let nextCur = next;
      for (let i = 0; i < path.length - 1; i++) nextCur = nextCur[path[i]];
      nextCur[path[path.length - 1]] = value;
      return next;
    });
  }

  function handleProviderChange(provider: string) {
    setSaveError("");
    setSaveNotice("");
    const models = PROVIDER_MODELS[provider] ?? [];
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    setCfg((prev: any) => {
      const next = structuredClone(prev);
      next.image_gen.provider = provider;
      if (models.length > 0 && !models.includes(next.image_gen.model)) {
        next.image_gen.model = models[0];
      }
      return next;
    });
  }

  async function save() {
    if (!cfg) {
      return;
    }
    const body = buildConfigSaveBody(cfg, llmKey, imgKey, imgSecret);
    const signature = JSON.stringify(body);
    if (signature === lastSavedConfigSignatureRef.current) {
      return;
    }
    setSaving(true);
    setSaveError("");
    setSaveNotice("");
    try {
      await updateAppConfig(body);
      lastSavedConfigSignatureRef.current = signature;
      if (!mountedRef.current) {
        return;
      }
      setHasSavedConfigOnce(true);
      setSaveNotice("已自动保存工作区设置");
    } catch (error) {
      const message = resolveErrorMessage(error) || "保存设置失败";
      if (mountedRef.current) {
        setSaveError(message);
      }
      throw error;
    } finally {
      if (mountedRef.current) {
        setSaving(false);
      }
    }
  }

  async function handleTestImageGenerationConfig() {
    if (imageTestLoading) {
      return;
    }

    setImageTestLoading(true);
    setSaveError("");
    setSaveNotice("");

    try {
      const result = await testImageGenerationConfig();
      if (!mountedRef.current) {
        return;
      }
      const sizeText = result.size ? `（${result.size[0]} x ${result.size[1]}）` : "";
      setSaveNotice(`生图配置测试成功${sizeText}`);
    } catch (error) {
      if (mountedRef.current) {
        setSaveError(resolveErrorMessage(error) || "生图配置测试失败");
      }
    } finally {
      if (mountedRef.current) {
        setImageTestLoading(false);
      }
    }
  }

  return {
    cfg,
    llmKey,
    imgKey,
    imgSecret,
    setLlmKey,
    setImgKey,
    setImgSecret,
    saving,
    saveError,
    saveNotice,
    hasSavedConfigOnce,
    configDirty,
    imageTestLoading,
    set,
    handleProviderChange,
    save,
    handleTestImageGenerationConfig,
  };
}
