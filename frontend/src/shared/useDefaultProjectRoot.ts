import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import type { AppConfig } from "./api/config.ts";
import { loadAppConfig } from "./api/index.ts";

export function resolveDefaultProjectRootValue(
  currentValue: string,
  defaultProjectRoot?: string,
): string {
  return currentValue || defaultProjectRoot || "";
}

interface UseDefaultProjectRootOptions {
  setProjectRoot: Dispatch<SetStateAction<string>>;
  onConfigLoaded?: (config: AppConfig) => void;
}

export function useDefaultProjectRoot(options: UseDefaultProjectRootOptions) {
  const { setProjectRoot, onConfigLoaded } = options;
  const onConfigLoadedRef = useRef(onConfigLoaded);

  useEffect(() => {
    onConfigLoadedRef.current = onConfigLoaded;
  }, [onConfigLoaded]);

  useEffect(() => {
    loadAppConfig()
      .then((config) => {
        onConfigLoadedRef.current?.(config);
        const defaultProjectRoot =
          typeof config?.default_project_root === "string"
            ? config.default_project_root
            : undefined;
        setProjectRoot((currentValue) =>
          resolveDefaultProjectRootValue(currentValue, defaultProjectRoot),
        );
      })
      .catch(() => {});
  }, [setProjectRoot]);
}
