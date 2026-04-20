import { watch } from "fs";
import { join } from "path";
import type { BunPlugin } from "bun";

const isWatch = process.argv.includes("--watch");

// Plugin to load .css files as text strings
const cssTextPlugin: BunPlugin = {
  name: "css-text",
  setup(build) {
    build.onLoad({ filter: /\.css$/ }, async (args) => {
      const text = await Bun.file(args.path).text();
      return {
        contents: `export default ${JSON.stringify(text)};`,
        loader: "js",
      };
    });
  },
};

async function build() {
  const results = await Promise.all([
    Bun.build({
      entrypoints: [join(import.meta.dir, "src/content/index.ts")],
      outdir: join(import.meta.dir, "dist"),
      naming: "content.js",
      target: "browser",
      format: "iife",
      plugins: [cssTextPlugin],
    }),
    Bun.build({
      entrypoints: [join(import.meta.dir, "src/background/service-worker.ts")],
      outdir: join(import.meta.dir, "dist"),
      naming: "service-worker.js",
      target: "browser",
      format: "esm",
    }),
    Bun.build({
      entrypoints: [join(import.meta.dir, "src/panel/panel.tsx")],
      outdir: join(import.meta.dir, "dist"),
      naming: "panel.js",
      target: "browser",
      format: "iife",
    }),
  ]);

  for (const result of results) {
    if (!result.success) {
      console.error("Build failed:");
      for (const log of result.logs) {
        console.error(log);
      }
      process.exit(1);
    }
  }

  console.log("Build complete: dist/content.js, dist/service-worker.js, dist/panel.js");
}

await build();

if (isWatch) {
  console.log("Watching for changes...");
  watch(join(import.meta.dir, "src"), { recursive: true }, async () => {
    console.log("Rebuilding...");
    await build();
  });
}
