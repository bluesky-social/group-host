// The docs, as Starlight reads them: each page under docs/ copied into
// src/content/docs, its first heading made its `title`, a README its
// directory's index. docs/ stays plain Markdown that reads on its own.
//
//   node content.mjs

import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
export const docs = join(here, '../docs');
const out = join(here, 'src/content/docs');

const files = (d) =>
  readdirSync(d, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(d, e.name)) : e.name.endsWith('.md') ? [join(d, e.name)] : []
  );

rmSync(out, { recursive: true, force: true });
for (const file of files(docs)) {
  const page = relative(docs, file);
  const text = readFileSync(file, 'utf8');
  const heading = text.match(/^# (.+)\n+/m);
  if (!heading) throw new Error(`docs/${page}: no # heading to take its title from`);
  const body = text.slice(0, heading.index) + text.slice(heading.index + heading[0].length);
  const front = `---\ntitle: ${JSON.stringify(heading[1].replaceAll('`', ''))}\n---\n\n`;
  const target = join(out, page.replace(/(^|\/)README\.md$/, '$1index.md'));
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, front + body);
}
