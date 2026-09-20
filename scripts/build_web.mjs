import {build} from 'esbuild-wasm';
import {mkdir, writeFile, readFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
await mkdir(`${root}web/dist`, {recursive: true});
await build({
  absWorkingDir: root,
  entryPoints: ['web/src/index.js'],
  outfile: 'web/dist/ui.js',
  bundle: true,
  format: 'iife',
  globalName: 'CetzUi',
  target: ['es2022'],
  minify: true,
  legalComments: 'inline',
});
await writeFile(`${root}web/dist/licenses.txt`,
  await readFile(`${root}node_modules/dompurify/LICENSE`));
