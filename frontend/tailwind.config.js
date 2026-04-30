/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        spire: {
          bg: "#0f0c1a",
          surface: "#1a1428",
          border: "#2d2445",
          accent: "#c9a227",
          red: "#c0392b",
          purple: "#8e44ad",
          text: "#e8dcc8",
          muted: "#8a7a6a",
        },
      },
    },
  },
  plugins: [],
};
