import * as esbuild from "esbuild";

await esbuild
  .build({
    entryPoints: ["./src/main.ts"],
    bundle: true,
    platform: "node",
    target: "node22",
    outfile: "dist/index.cjs",
    define: {
      "process.env.GITCOMMIT": `"${process.env.GITCOMMIT}"`,
    },
  })
  .catch(() => process.exit(1));
