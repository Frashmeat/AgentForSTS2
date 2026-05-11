import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;

// __IS_TAURI__ 是关键：vite 通过 define 在编译期把它替换为字面量布尔，
// 让 src/services/api.ts 在 tree-shake 后只保留一边的 import。
//   - tauri dev/build 会设 TAURI_ENV_PLATFORM → true
//   - 纯 web 构建（npm run build:web）无此环境变量 → false
export default defineConfig(async () => {
  const pkg = await import("./package.json", { with: { type: "json" } });
  return {
    plugins: [react()],
    resolve: {
      alias: {
        "@": path.resolve(__dirname, "./src"),
      },
    },
    define: {
      __IS_TAURI__: JSON.stringify(!!process.env.TAURI_ENV_PLATFORM),
      __APP_VERSION__: JSON.stringify(pkg.default.version),
    },
    build: {
      target: "es2022",
      outDir: "dist",
      emptyOutDir: true,
    },
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      host: host || false,
      hmr: host
        ? { protocol: "ws", host, port: 1421 }
        : undefined,
      watch: {
        ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"],
      },
    },
  };
});
