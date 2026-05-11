/// <reference types="vite/client" />

// 由 vite.config.ts 的 `define` 在编译期注入。
// 详见 docs/rust-rewrite/ADR-002-frontend-stack.md。
declare const __IS_TAURI__: boolean;
declare const __APP_VERSION__: string;
