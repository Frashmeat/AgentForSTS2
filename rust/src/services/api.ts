// 统一 API 入口。详见 docs/rust-rewrite/ADR-002-frontend-stack.md。
//
// 编译期常量 __IS_TAURI__ 决定动态 import 哪一边的实现。Proxy 把同步访问
// 包成 async 调用，保证 app 启动不依赖顶层 await。

import type * as TauriApi from "./tauriApi";

type ApiModule = typeof TauriApi;

const apiModulePromise: Promise<ApiModule> = __IS_TAURI__
  ? import("./tauriApi")
  : (import("./webApi") as unknown as Promise<ApiModule>);

export const api = new Proxy({} as ApiModule, {
  get(_target, prop) {
    return async (...args: unknown[]) => {
      const mod = await apiModulePromise;
      const member = mod[prop as keyof ApiModule];
      if (typeof member !== "function") {
        return member;
      }
      return (member as (...a: unknown[]) => unknown)(...args);
    };
  },
});
