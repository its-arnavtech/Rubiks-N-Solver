import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // The API server (`python -m nxnn.server`) listens on 127.0.0.1:8000.
  server: { port: 5173, strictPort: true, proxy: { "/api": "http://127.0.0.1:8000" } },
  build: { target: "es2022" },
});
