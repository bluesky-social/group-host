// The docs: every relative link reaches a file, and every `#anchor` a
// heading in it, as the repository's pages and the docs site slug them.
//
//   node harness/docs.mjs
//
// And every guide's picture is drawn by a look line above it
// (harness/shots.mjs), in both schemes and as text, under a name of its
// own, and docs/guides/shots holds no picture no guide shows. A guide's
// words in bold are words on screen, so each is in the text of one of
// its pictures: a guide that says a button the console has since renamed
// fails here, before anyone draws it again.
//
// Prints each problem as `file:line: what`, failing if any.

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { allGuides, shotsDir, shotsIn } from './shots.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const files = (d) =>
  readdirSync(d, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(d, e.name)) : e.name.endsWith('.md') ? [join(d, e.name)] : []
  );
const docs = ['README.md', 'AGENTS.md'].map((f) => join(root, f)).concat(files(join(root, 'docs')));

// The lines outside fences, with the line each is.
const split = (text) => {
  const prose = [];
  let fenced = false;
  text.split('\n').forEach((line, i) => {
    if (line.startsWith('```')) fenced = !fenced;
    else if (!fenced) prose.push([i + 1, line]);
  });
  return { prose };
};

const slugged = new Map();
const slugs = (file) => {
  if (!slugged.has(file)) {
    const seen = new Map();
    const out = new Set();
    for (const [, line] of split(readFileSync(file, 'utf8')).prose) {
      const m = line.match(/^#{1,6}\s+(.*)/);
      if (!m) continue;
      const slug = m[1]
        .trim()
        .toLowerCase()
        .replace(/[^\p{L}\p{N}_\- ]/gu, '')
        .replaceAll(' ', '-');
      const n = seen.get(slug) ?? 0;
      seen.set(slug, n + 1);
      out.add(n ? `${slug}-${n}` : slug);
    }
    slugged.set(file, out);
  }
  return slugged.get(file);
};

const problems = [];
for (const file of docs) {
  const name = relative(root, file);
  const { prose } = split(readFileSync(file, 'utf8'));
  for (const [n, line] of prose) {
    for (const [, url] of line.replace(/`[^`]*`/g, '').matchAll(/\]\(([^)\s]+)\)/g)) {
      if (/^[a-z]+:/.test(url)) continue;
      const [path, anchor] = url.split('#');
      const target = path ? resolve(dirname(file), path) : file;
      if (!existsSync(target)) problems.push(`${name}:${n}: no ${path}`);
      else if (anchor && target.endsWith('.md') && !slugs(target).has(anchor))
        problems.push(`${name}:${n}: no heading #${anchor} in ${relative(root, target)}`);
    }
  }
}

/// Text as compared: one kind of quote, one space.
const plain = (t) =>
  t
    .replace(/[\u2018\u2019]/g, "'")
    .replace(/[\u201c\u201d]/g, '"')
    .replace(/\\"/g, '"')
    .replace(/\s+/g, ' ');

const shown = new Map();
// Every picture's text, of every guide: a guide may name a button that
// another guide's picture shows.
const seen = [];
const bolds = [];
for (const guide of allGuides()) {
  const name = relative(root, guide);
  const text = readFileSync(guide, 'utf8');
  for (const shot of shotsIn(text)) {
    if (shot.problem) problems.push(`${name}:${shot.line}: ${shot.problem}`);
    if (!shot.name) continue;
    if (shown.has(shot.name))
      problems.push(
        `${name}:${shot.line}: shots/${shot.name}.png is shown by ${shown.get(shot.name)} already`
      );
    shown.set(shot.name, name);
    if (!existsSync(join(shotsDir, `${shot.name}-dark.png`)))
      problems.push(`${name}:${shot.line}: no shots/${shot.name}-dark.png: \`just shots ${name}\``);
    const said = join(shotsDir, `${shot.name}.txt`);
    if (existsSync(said)) seen.push(plain(readFileSync(said, 'utf8')));
    else problems.push(`${name}:${shot.line}: no shots/${shot.name}.txt: \`just shots ${name}\``);
  }
  // Each bold run outside code, read a paragraph at a time as a bold run
  // may wrap, is a word on screen: in some picture's text, checked below.
  let para = [];
  const flush = () => {
    const joined = para.map(([, l]) => l.replace(/^\s*(\d+\.|-)\s+/, '').trim()).join(' ');
    for (const [, bold] of joined.replace(/`[^`]*`/g, '').matchAll(/\*\*([^*]+)\*\*/g))
      bolds.push([`${name}:${para[0][0]}`, bold]);
    para = [];
  };
  for (const [n, line] of split(text).prose) {
    if (!line.trim() || /^\s*(\d+\.|-)\s/.test(line) || /^#/.test(line)) flush();
    if (line.trim() && !/^\s*<!--/.test(line)) para.push([n, line]);
  }
  flush();
}
for (const [at, bold] of bolds)
  if (!seen.some((t) => t.includes(plain(bold))))
    problems.push(`${at}: **${bold}** is in none of the guides' pictures`);
if (existsSync(shotsDir))
  for (const f of readdirSync(shotsDir))
    if (!shown.has(f.replace(/(-dark\.png|\.png|\.txt)$/, '')))
      problems.push(`docs/guides/shots/${f}: no guide shows it`);

for (const p of problems) console.log(p);
console.log(`${docs.length} pages, ${problems.length} problems`);
process.exit(problems.length ? 1 : 0);
