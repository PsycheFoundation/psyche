import * as esbuild from 'esbuild'
import { copyFile } from 'fs/promises'
import path from 'path'

await esbuild
	.build({
		entryPoints: ['./src/index.ts'],
		bundle: true,
		platform: 'node',
		target: 'node22',
		outfile: 'dist/index.cjs',
		define: {
			'process.env.GITCOMMIT': `"${process.env.GITCOMMIT}"`,
		},
		sourcemap: 'inline',
	})
	.catch(() => process.exit(1))

await copyFile(
	path.join(
		import.meta.dirname,
		'../wasm/pkg/psyche_deserialize_zerocopy_wasm_bg.wasm'
	),
	path.join(
		import.meta.dirname,
		'dist/psyche_deserialize_zerocopy_wasm_bg.wasm'
	)
)
