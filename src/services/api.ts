// 统一 API 入口。详见 docs/01-总览/项目架构总览.md。
//
// 编译期常量 __IS_TAURI__ 决定动态 import 哪一边的实现。Proxy 把同步访问
// 包成 async 调用，保证 app 启动不依赖顶层 await。

import type * as TauriApi from "./tauriApi";
import { toActionableFailure } from "./actionableFailure";

type ApiModule = typeof TauriApi;

// webApi 必须实现 tauriApi 的全部导出：把 import 结果赋给 Promise<ApiModule>，
// 若 webApi 漏实现某导出或签名不兼容，编译期即报错（取代旧的 `as unknown as` 逃逸，
// 那会悄悄抹掉双后端契约，让 Web 端缺接口要到运行期才暴露）。
const apiModulePromise: Promise<ApiModule> = __IS_TAURI__
  ? import("./tauriApi")
  : import("./webApi");

export const api = new Proxy({} as ApiModule, {
  get(_target, prop) {
    return async (...args: unknown[]) => {
      const mod = await apiModulePromise;
      const member = mod[prop as keyof ApiModule];
      if (member === undefined) {
        // 当前后端未实现该成员：直接抛错，而非静默返回 undefined（旧行为会让
        // 漏接口在调用处变成 `undefined is not a function` 之类的间接报错）。
        throw toActionableFailure(undefined);
      }
      if (typeof member !== "function") {
        return member;
      }
      return (member as (...a: unknown[]) => unknown)(...args);
    };
  },
});
