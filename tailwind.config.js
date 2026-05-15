/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: ["selector", '[data-theme="ink"]'],
  theme: {
    extend: {
      colors: {
        // 兼容旧用法（card 里大量 text-muted / border-muted/30 / text-accent ...）
        background: "var(--paper)",
        foreground: "var(--ink)",
        muted: "var(--ink-mute)",
        accent: "var(--accent)",

        // 模板新增的语义 token，组件里可按需用
        paper: "var(--paper)",
        "paper-deep": "var(--paper-deep)",
        "paper-soft": "var(--paper-soft)",
        ink: "var(--ink)",
        "ink-soft": "var(--ink-soft)",
        "ink-mute": "var(--ink-mute)",
        "ink-faint": "var(--ink-faint)",
        rule: "var(--rule)",
        "accent-deep": "var(--accent-deep)",
        jade: "var(--jade)",
        "jade-soft": "var(--jade-soft)",
        gold: "var(--gold)",
      },
      fontFamily: {
        display: ['"Fraunces"', '"Noto Serif SC"', "serif"],
        serif: ['"Noto Serif SC"', '"Fraunces"', "serif"],
        sans: [
          '"Instrument Sans"',
          '"Noto Sans SC"',
          "-apple-system",
          "BlinkMacSystemFont",
          '"Segoe UI"',
          "sans-serif",
        ],
        mono: ['"JetBrains Mono"', "ui-monospace", "Menlo", "Consolas", "monospace"],
      },
    },
  },
  plugins: [],
};
