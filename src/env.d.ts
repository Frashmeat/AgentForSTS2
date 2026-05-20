/// <reference types="vite/client" />

// 由 vite.config.ts 的 `define` 在编译期注入。
// 详见 docs/04-决策/0002-frontend-and-api-double-adapter.md。
declare const __IS_TAURI__: boolean;
declare const __APP_VERSION__: string;
