import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

/**
 * Inline the built JS and CSS into index.html: pmx embeds one self-contained page (plan D17), which
 * also has to work when opened straight from disk.
 */
function inlineIntoHtml(): Plugin {
  return {
    name: "pmx-inline-into-html",
    apply: "build",
    enforce: "post",
    generateBundle(_options, bundle) {
      const html = bundle["index.html"];
      if (!html || html.type !== "asset") return;
      let page = String(html.source);
      const inlined: string[] = [];
      for (const [file, out] of Object.entries(bundle)) {
        if (out.type === "chunk" && out.isEntry) {
          const code = out.code.replace(/<\/script/gi, "<\\/script");
          page = page.replace(
            new RegExp(`<script[^>]*src="[^"]*${escape(file)}"[^>]*></script>`),
            () => `<script type="module">${code}</script>`,
          );
          inlined.push(file);
        } else if (out.type === "asset" && file.endsWith(".css")) {
          const css = String(out.source).replace(/<\/style/gi, "<\\/style");
          page = page.replace(
            new RegExp(`<link[^>]*href="[^"]*${escape(file)}"[^>]*>`),
            () => `<style>${css}</style>`,
          );
          inlined.push(file);
        }
      }
      for (const file of inlined) Reflect.deleteProperty(bundle, file);
      html.source = page;
    },
  };
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export default defineConfig(({ command }) => ({
  plugins: [react(), inlineIntoHtml()],
  base: "./",
  // `npm run dev` serves fictional demo data from dev-data/ (`npm run dev-data` makes it).
  publicDir: command === "serve" ? "dev-data" : false,
  build: {
    target: "es2022",
    cssCodeSplit: false,
    modulePreload: false,
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    chunkSizeWarningLimit: 4096,
    rolldownOptions: { output: { codeSplitting: false } },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["src/test/setup.ts"],
    css: false,
  },
}));
