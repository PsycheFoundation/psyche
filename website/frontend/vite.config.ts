import { defineConfig } from "vite";
import react from "@vitejs/plugin-react-swc";
import { TanStackRouterVite } from "@tanstack/router-plugin/vite";
import wyw from "@wyw-in-js/vite";
import svgr from "vite-plugin-svgr";
import { nodePolyfills } from "vite-plugin-node-polyfills";
import glsl from "vite-plugin-glsl";

// https://vite.dev/config/
export default defineConfig({
  server: {
    host: true,
    allowedHosts: true,
  },
  plugins: [
    nodePolyfills({
      globals: {
        Buffer: true,
      },
    }),
    TanStackRouterVite({ autoCodeSplitting: true, addExtensions: true }),
    react(),
    svgr(),
    wyw(),
    glsl(),
  ],
});
