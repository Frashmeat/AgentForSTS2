// ESLint 9 flat config。
// 规则集分两类：
// 1) 静态正确性 / hooks 规则 = error（接入即生效）
// 2) 风格倾向 / 类型严格化 = warn（收集 baseline，后续逐步降为 error）
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactPlugin from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";
import prettier from "eslint-config-prettier";

export default [
  {
    ignores: ["dist", "node_modules", "public/runtime-config.js"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx,js,jsx}", "tests/**/*.{ts,tsx,js,jsx}"],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        ecmaFeatures: { jsx: true },
      },
      globals: {
        // 浏览器环境
        window: "readonly",
        document: "readonly",
        navigator: "readonly",
        fetch: "readonly",
        console: "readonly",
        setTimeout: "readonly",
        clearTimeout: "readonly",
        setInterval: "readonly",
        clearInterval: "readonly",
        WebSocket: "readonly",
        URL: "readonly",
        URLSearchParams: "readonly",
        FormData: "readonly",
        File: "readonly",
        FileReader: "readonly",
        Blob: "readonly",
        atob: "readonly",
        btoa: "readonly",
        TextEncoder: "readonly",
        TextDecoder: "readonly",
        crypto: "readonly",
        structuredClone: "readonly",
        // Node/Vitest globals（tests 里用）
        process: "readonly",
        globalThis: "readonly",
      },
    },
    plugins: {
      react: reactPlugin,
      "react-hooks": reactHooks,
    },
    settings: {
      react: { version: "18.3" },
    },
    rules: {
      // hooks 强制约束
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",

      // React 18 不需要再 import React
      "react/react-in-jsx-scope": "off",
      "react/jsx-uses-react": "off",
      // Tailwind 用 className 不需要 prop-types 校验
      "react/prop-types": "off",

      // 类型严格化（先 warn 收集 baseline）
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
      ],

      // 普遍宽松（误报多 / 不强制）
      "@typescript-eslint/no-empty-object-type": "off",
      "@typescript-eslint/no-explicit-this": "off",
      "no-empty": ["warn", { allowEmptyCatch: true }],
      "no-constant-condition": ["error", { checkLoops: false }],
    },
  },
  // 测试文件允许更宽
  {
    files: ["tests/**/*.{ts,tsx,js,jsx}", "src/**/*.test.{ts,tsx,js,jsx}"],
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unused-vars": "off",
      "no-useless-escape": "off",
    },
  },
  // 必须放在最后：关闭与 Prettier 冲突的格式化规则
  prettier,
];
