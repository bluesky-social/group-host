// The guides' pictures, drawn by `just look` from the line above each, so
// none is ever taken by hand or goes stale unseen:
//
//   <!-- look /g/lanterns.groups.test --as alice --seed full -->
//   ![Lantern Hikers, as alice sees it](shots/group.png)
//
//   node harness/shots.mjs [docs/guides/<guide>.md ...]    `just shots`
//
// Each guide is drawn over a fresh world of its own, its pictures in the
// order the page has them: a seed or a sent form early in a guide is
// there for every picture after it, as the reader's own group would be.
// Each picture is drawn at desktop size, light as shots/<name>.png and
// dark as shots/<name>-dark.png, beside the guides; the docs site shows
// the one the reader's scheme asks for. With no guides named, every guide.
//
// Beside each is shots/<name>.txt, what the picture shows as text: its
// headings, buttons, labels and words, as the page's accessibility tree
// has them, with what differs from one world to the next (times, DIDs,
// ports, a new password) put as a placeholder. Pixels differ between any
// two runs; these do not, unless the console does.
//
//   node harness/shots.mjs --check [guide ...]
//
// draws them all again and compares only the text, writing nothing: it
// fails naming each picture whose page has changed, as the e2e check
// runs it, so a guide whose console moved on is drawn again
// (`just shots`) and its words read against the new pictures.
//
// A world kept up by `just look-up` would bring its own state into the
// pictures, so none may be up.

import { spawn } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const guides = join(root, 'docs/guides');
export const shotsDir = join(guides, 'shots');

/// The options a guide's line may not give: each picture is drawn at the
/// one size, in both schemes, where this puts it.
const OWN = ['--out', '--name', '--desktop', '--phone', '--light', '--dark', '--all', '--up'];

/// A line's words, as a shell splits them: quoted, or not.
const words = (line) =>
  [...line.matchAll(/'([^']*)'|"([^"]*)"|(\S+)/g)].map((m) => m[1] ?? m[2] ?? m[3]);

/// The pictures a guide's text has: each with the line it is on, its
/// name, its words for `just look`, and what is wrong with it, if anything.
export function shotsIn(text) {
  const lines = text.split('\n');
  const out = [];
  lines.forEach((line, i) => {
    const look = line.match(/^\s*<!-- look (.*) -->$/);
    const image = line.match(/!\[[^\]]*\]\(shots\/([^)\s]+)\)/);
    if (look) {
      const next = lines[i + 1]?.match(/^\s*!\[[^\]]*\]\(shots\/([a-z0-9-]+)\.png\)$/);
      const args = words(look[1]);
      const own = args.find((a) => OWN.includes(a));
      const problem = !next
        ? 'a look line with no picture under it'
        : own
          ? `${own}: a guide's picture is drawn at one size in both schemes`
          : !args[0]?.startsWith('/')
            ? 'a look line starts with the page, as /g/riders.groups.test'
            : null;
      out.push({ line: i + 1, name: next?.[1], args, problem });
    } else if (image && !/^\s*<!-- look /.test(lines[i - 1] ?? '')) {
      out.push({
        line: i + 1,
        name: image[1].replace(/\.png$/, ''),
        args: [],
        problem: 'a picture with no look line above it'
      });
    }
  });
  return out;
}

/// Every guide, by path.
export const allGuides = () =>
  existsSync(guides)
    ? readdirSync(guides)
        .filter((f) => f.endsWith('.md'))
        .map((f) => join(guides, f))
    : [];

const say = (line) => console.log(`==> ${line}`);
const look = join(root, 'harness/look/look.mjs');

/// A picture's text, with what differs between worlds made the same:
/// links' addresses dropped, and times, DIDs, keys, record keys, ports and
/// app passwords put as placeholders.
export const settled = (text) =>
  text
    .split('\n')
    .filter((l) => !/^\s*- \/url:/.test(l))
    .join('\n')
    .replace(/\b\d{1,2} [A-Z][a-z]{2} \d{4}(, \d{2}:\d{2} UTC)?/g, '<date>')
    .replace(/\b[a-z2-7]{4}-[a-z2-7]{4}-[a-z2-7]{4}-[a-z2-7]{4}\b/g, '<password>')
    .replace(/did:plc:[a-z2-7]{24}/g, 'did:plc:<id>')
    .replace(/did:key:z[1-9A-HJ-NP-Za-km-z]{40,}/g, 'did:key:<key>')
    .replace(/\b3[a-z2-7]{12}\b/g, '<rkey>')
    .replace(/(localhost|127\.0\.0\.1|\.internal)(:|%3A)\d+/g, '$1$2<port>');

/// A world for one guide, up until `stop()`.
function world() {
  return new Promise((done, failed) => {
    const child = spawn(process.execPath, [look, '--up'], {
      cwd: root,
      stdio: ['ignore', 'pipe', 'inherit']
    });
    let said = '';
    child.stdout.on('data', (d) => {
      said += d;
      process.stdout.write(d);
      if (/a world is up at/.test(said))
        done({
          stop: () =>
            new Promise((ended) => {
              // Asked to stop, as a deploy asks the host; killed if it has
              // not within a minute, so a stuck world never holds a check.
              const late = setTimeout(() => child.kill('SIGKILL'), 60_000);
              child.on('exit', () => {
                clearTimeout(late);
                ended();
              });
              child.kill('SIGTERM');
            })
        });
    });
    child.on('exit', (code) =>
      failed(
        new Error(
          `the world stopped before it was up (${code}): is \`just look-up\` running? Stop it first`
        )
      )
    );
  });
}

/// One picture: drawn by look into `into`, its pictures and its text.
function draw(shot, into) {
  return new Promise((done, failed) => {
    const args = [
      look,
      ...shot.args,
      '--desktop',
      '--light',
      '--dark',
      '--out',
      into,
      '--name',
      shot.name,
      '--text'
    ];
    const child = spawn(process.execPath, args, {
      cwd: root,
      stdio: ['ignore', 'pipe', 'inherit']
    });
    let said = '';
    child.stdout.on('data', (d) => (said += d));
    child.on('exit', (code) => {
      if (code) return failed(new Error(`${shot.name}: look failed (${code})\n${said}`));
      const files = said.split('\n');
      const light = files.find((f) => f.endsWith('-desktop-light.png'));
      const dark = files.find((f) => f.endsWith('-desktop-dark.png'));
      const text = files.find((f) => f.endsWith('-desktop.txt'));
      if (!light || !dark || !text)
        return failed(new Error(`${shot.name}: look drew no pictures\n${said}`));
      // What look warned of, a page wider than the screen, said on.
      for (const l of files) if (l.includes('wider than the screen')) say(l.replace(/^==> /, ''));
      done({ light, dark, text: settled(readFileSync(text, 'utf8')) });
    });
  });
}

/// Where two texts first differ, as a few lines of each.
function differs(was, now) {
  const a = was.split('\n');
  const b = now.split('\n');
  let i = 0;
  while (i < a.length && a[i] === b[i]) i++;
  const at = (lines) => lines.slice(i, i + 3).map((l) => `    ${l}`);
  return [`  at line ${i + 1}, was:`, ...at(a), '  now:', ...at(b)].join('\n');
}

async function main() {
  const argv = process.argv.slice(2);
  const check = argv.includes('--check');
  const named = argv.filter((a) => a !== '--check').map((p) => resolve(p));
  const stale = [];
  const which = named.length ? named : allGuides();
  if (!which.length) throw new Error('no guides in docs/guides');
  mkdirSync(shotsDir, { recursive: true });
  const into = mkdtempSync(join(tmpdir(), 'shots-'));
  try {
    for (const guide of which) {
      const shots = shotsIn(readFileSync(guide, 'utf8'));
      const wrong = shots.filter((s) => s.problem);
      if (wrong.length)
        throw new Error(wrong.map((s) => `${basename(guide)}:${s.line}: ${s.problem}`).join('\n'));
      if (!shots.length) continue;
      say(`${basename(guide)}: ${shots.length} pictures, over a world of its own`);
      const w = await world();
      try {
        for (const shot of shots) {
          // Checking, a line the console no longer answers (a button gone)
          // marks its guide and goes on, so one run names every guide.
          const drawn = check
            ? await draw(shot, into).catch((e) => ({ failed: e.message.split('\n')[0] }))
            : await draw(shot, into);
          if (drawn.failed) {
            stale.push(basename(guide));
            say(`${shot.name}: its line no longer draws (${drawn.failed})`);
            continue;
          }
          const kept = join(shotsDir, `${shot.name}.txt`);
          if (check) {
            const was = existsSync(kept) ? readFileSync(kept, 'utf8') : '';
            if (was === drawn.text) say(`${shot.name}: as it was`);
            else {
              stale.push(basename(guide));
              say(`${shot.name}: its page has changed\n${differs(was, drawn.text)}`);
            }
            continue;
          }
          copyFileSync(drawn.light, join(shotsDir, `${shot.name}.png`));
          copyFileSync(drawn.dark, join(shotsDir, `${shot.name}-dark.png`));
          writeFileSync(kept, drawn.text);
          say(`${shot.name}: docs/guides/shots/${shot.name}.png, -dark.png and .txt`);
        }
      } finally {
        await w.stop();
      }
    }
  } finally {
    rmSync(into, { recursive: true, force: true });
  }
  if (stale.length) {
    const which = [...new Set(stale)].map((g) => `docs/guides/${g}`).join(' ');
    throw new Error(
      `the console has moved on under ${which}: \`just shots ${which}\`, then read each guide against its new pictures`
    );
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
  // Exits when done, whatever a world left open behind it.
  main().then(
    () => process.exit(0),
    (e) => {
      say(e.message);
      process.exit(2);
    }
  );
