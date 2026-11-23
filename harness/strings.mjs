// Every word a page shows is a message (web/messages/en.json, called as
// `m.<key>()` from `#lib/paraglide/messages.js`), so the console can be
// read in another language by adding that language's file. Words written
// straight into a component's markup can't be, and are found here.
//
//   node harness/strings.mjs
//
// Parses every component under web/src and web/shell, and prints each run
// of text in its markup with a letter in it, and each attribute that
// people read (a `title`, `label`, `placeholder`, …) given words as it is
// written, as `file:line: words`, failing if any. Words that are no
// language's (a DNS record's type, the host's name) are let be by
// `NAMES` below. A script's strings aren't read: a message called there
// is the same as one called in the markup, and words built there are
// found in review.

import { readdirSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const web = join(root, 'web');
// The console's own Svelte, which `npm ci` in web/ installs.
const { parse } = createRequire(join(web, 'package.json'))('svelte/compiler');

/** The attributes whose words people read, on an element or a component. */
const READ = new Set(['title', 'label', 'placeholder', 'aria-label', 'alt', 'confirm', 'submit']);

/** Words that are the same in every language: names, a DNS record's type, a host's first label. */
const NAMES = new Set([
  'opensocial',
  'A',
  'AAAA',
  'TXT',
  'CNAME',
  'DID',
  'PDS',
  'MCP',
  'OAuth',
  'www'
]);

const LETTER = /\p{L}/u;

/**
 * Whether `word` is a language's: not a name, and not code, a domain or
 * an NSID (`_atproto.`, `fyi.opensocial.siteSpace/board`), which hold a
 * dot, an underscore, a slash, a colon or an @ before their end.
 */
function spoken(word) {
  const bare = word.replace(/[.!?…:;)]+$/u, '').replace(/^[(“"‘']+/u, '');
  return LETTER.test(bare) && !NAMES.has(bare) && !/[._/:@]/.test(bare);
}

/** Whether `text` is words to translate: one of them a language's. */
function words(text) {
  return text
    .trim()
    .split(/[\s,·]+/)
    .some(spoken);
}

/** Whether an attribute's written value is words: a capital, or a space between letters. */
function said(value) {
  return words(value) && (/\p{Lu}/u.test(value) || /\p{L}\s+\p{L}/u.test(value));
}

const files = (d) =>
  readdirSync(d, { withFileTypes: true }).flatMap((e) =>
    e.name === 'node_modules' || e.name === 'paraglide'
      ? []
      : e.isDirectory()
        ? files(join(d, e.name))
        : e.name.endsWith('.svelte')
          ? [join(d, e.name)]
          : []
  );

const problems = [];
for (const file of [...files(join(web, 'src')), ...files(join(web, 'shell'))]) {
  const text = readFileSync(file, 'utf8');
  const line = (at) => text.slice(0, at).split('\n').length;
  const where = (at, what) =>
    problems.push(`${relative(root, file)}:${line(at)}: ${what.trim().replace(/\s+/g, ' ')}`);
  /** Each node of the markup under `node`: its text, and the attributes people read. */
  const walk = (node) => {
    if (Array.isArray(node)) return node.forEach(walk);
    if (!node || typeof node !== 'object') return;
    if (node.type === 'Text' && words(node.data)) where(node.start, node.data);
    if (node.type === 'Attribute') {
      const value = Array.isArray(node.value) ? node.value : [];
      if (READ.has(node.name))
        for (const part of value)
          if (part.type === 'Text' && said(part.data))
            where(part.start, `${node.name}="${part.data}"`);
      return;
    }
    // A <style> or <script> of the markup's own is no words on the page.
    if (node.type === 'RegularElement' && (node.name === 'style' || node.name === 'script')) return;
    for (const [key, value] of Object.entries(node)) if (key !== 'metadata') walk(value);
  };
  walk(parse(text, { modern: true }).fragment);
}

if (problems.length) {
  console.log(problems.join('\n'));
  console.log(
    `strings: ${problems.length} words written into markup; make each a message in web/messages/en.json (harness/strings.mjs)`
  );
  process.exit(1);
}
console.log('strings: every word a page shows is a message');
