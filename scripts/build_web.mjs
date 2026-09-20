import {build} from 'esbuild-wasm';
import {mkdir, writeFile, readFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
await mkdir(`${root}web/dist`, {recursive: true});
for (const [name, globalName] of [['svg', 'CetzSvg'], ['controls', 'CetzControls'], ['drag-router', 'CetzDragRouter']]) {
  await build({
    absWorkingDir: root,
    entryPoints: [`web/src/${name}.js`],
    outfile: `web/dist/${name}.js`,
    bundle: true,
    format: 'iife',
    globalName,
    target: ['es2022'],
    minify: true,
    legalComments: 'inline',
  });
}
await writeFile(`${root}web/dist/DOMPurify-LICENSE.txt`,
  await readFile(`${root}node_modules/dompurify/LICENSE`));
