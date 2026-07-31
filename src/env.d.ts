/// <reference types="vite/client" />

// 由 vite.config.ts 的 `define` 在编译期注入。
// 详见 docs/01-总览/项目架构总览.md。
declare const __IS_TAURI__: boolean;
declare const __APP_VERSION__: string;
