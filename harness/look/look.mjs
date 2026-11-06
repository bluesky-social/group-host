// A look at the console's pages as this checkout draws them, in any state,
// as pictures to open: not whether a page works, as the e2e flows say,
// but what it looks like.
//
//   node harness/look/look.mjs <path>... [options]    `just look`
//   node harness/look/look.mjs --up                   `just look-up`
//
// The pages are drawn over a world (../world.mjs): the reference PDS with
// the accounts below, the host, and the console served by its dev server,
// so what is drawn is the source as it is now, with no build. Starting one
// takes a while; `--up` starts one and keeps it, until ctrl-c, for every
// look after to use. A look with none up starts its own, and stops it.
// A world starts with Riders (riders.groups.test), alice's, and nothing
// else in it: a seed (--seed) puts in what a page should show.
//
// Options:
//
//   --as <name>             signed in as that account (alice, bob, ...),
//                           at its PDS's own page, as a person signs in;
//                           signed out without it. Several, a comma
//                           between (alice,bob), adds each in turn, the
//                           last the one in use, the others to switch to
//   --seed <name|file>      runs a seed first: one of harness/look/seeds
//                           by name (full, long, waiting, apps, and the
//                           guides' own), or a file of your own. On a
//                           kept world each runs once
//   --click <selector>      click it first (a button that opens a dialog
//                           or a menu), in turn; then only the screen is
//                           shot, not the whole page
//   --type <selector>=<text>  type into it, emptied, in turn with the clicks
//   --press <key>           press it, in turn with the clicks (Escape)
//   --element <selector>    only it, not the whole page
//   --desktop --phone --light --dark   only those; desktop light and
//                           phone dark if none is given
//   --all                   all four
//   --out <dir>             where the pictures go; target/look
//   --name <name>           what the pictures are called, in place of
//                           the path's
//   --text                  beside each size's pictures, what they show
//                           as text: the accessibility tree of the page,
//                           or of --element, as <name>-<size>.txt
//
// A seed is a module whose default export is given `tools` below and
// writes what a page should show: `opensocial(name, ...args)` runs the
// command as that account; `ask(name, handle, message?)` asks to join a
// group as them; `exists(handle)` says whether a group is here;
// `page(name)` is a page of the console signed in as them, for what only
// the console writes (a group's pictures); `picture(w, h)` a PNG to give
// it; `world` and `who` say where everything is, and each account.
//
// A page wider than the screen is said, with what runs past its edge.
// Each picture's path is printed, to open; a run writes over the pictures
// of the same name and project, and no others.

import { chromium } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { crc32, deflateSync } from 'node:zlib';
import { resolving, root, serviceAuth, start, tlsFront } from '../world.mjs';

const say = (line) => console.log(`==> ${line}`);
const fail = (why) => {
  say(why);
  process.exit(2);
};

/// The accounts every look's world has: enough to fill a group past a
/// page of its members, harbor to move here, and one with a handle as long as the PDS takes.
export const ACCOUNTS = [
  'alice',
  'bob',
  'carol',
  'dan',
  'erin',
  'frank',
  'harbor',
  'maximilianaquinces'
];

const PROJECTS = {
  desktop: { viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 },
  phone: {
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2,
    isMobile: true,
    hasTouch: true
  }
};

const kept = join(root, 'target/look/world.json');

const targets = [];
const steps = [];
const seeds = [];
const sides = new Set();
let as = [];
let element;
let up = false;
let out = join(root, 'target/look');
let name;
let text = false;
const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  const next = () => argv[++i] ?? fail(`${a} needs a value`);
  if (a === '--up') up = true;
  else if (a === '--as')
    as = next()
      .split(',')
      .map((n) => n.trim().replace(/^@/, '').split('.')[0]);
  else if (a === '--seed') seeds.push(next());
  else if (a === '--click') steps.push({ click: next() });
  else if (a === '--type') {
    const [, sel, text] =
      next().match(/^((?:\[[^\]]*\]|[^=[])+)=(.*)$/s) ?? fail('--type takes <selector>=<text>');
    steps.push({ sel, text });
  } else if (a === '--press') steps.push({ key: next() });
  else if (a === '--element') element = next();
  else if (a === '--out') out = resolve(next());
  else if (a === '--name') name = next();
  else if (a === '--text') text = true;
  else if (a === '--all') for (const s of ['desktop', 'phone', 'light', 'dark']) sides.add(s);
  else if (/^--(desktop|phone|light|dark)$/.test(a)) sides.add(a.slice(2));
  else if (a.startsWith('--')) fail(`${a}: no such option`);
  else targets.push(a);
}
if (!up && !targets.length) fail('which page? a path, as / or /g/riders.groups.test/settings');
for (const t of targets) if (!t.startsWith('/')) fail(`${t}: a page is its path, starting with /`);
for (const n of as)
  if (!ACCOUNTS.includes(n)) fail(`--as ${n}: no such account; one of ${ACCOUNTS.join(', ')}`);

/// The projects asked for: each of the sizes and schemes given, or
/// desktop light and phone dark.
const projects = (() => {
  const sizes = ['desktop', 'phone'].filter((s) => sides.has(s));
  const schemes = ['light', 'dark'].filter((s) => sides.has(s));
  if (!sizes.length && !schemes.length)
    return [
      ['desktop', 'light'],
      ['phone', 'dark']
    ];
  return (sizes.length ? sizes : ['desktop', 'phone']).flatMap((size) =>
    (schemes.length ? schemes : ['light']).map((scheme) => [size, scheme])
  );
})();

/// A kept world, if one is up and answering.
async function keptWorld() {
  if (!existsSync(kept)) return null;
  const w = JSON.parse(readFileSync(kept, 'utf8'));
  const alive = await fetch(`${w.host}/xrpc/_health`).then(
    () => true,
    () => false
  );
  return alive ? w : null;
}

/// A world of its own: started, and how to stop it.
async function newWorld() {
  say('starting a world: the PDS, the host, the console (`just look-up` keeps one)');
  const w = await start({ accounts: ACCOUNTS, front: tlsFront, live: true });
  const { net } = w;
  return {
    world: {
      base: w.base,
      host: w.host,
      did: w.did,
      pds: net.pds,
      mail: net.mail,
      accounts: net.accounts,
      riders: w.riders,
      seeded: [],
      signedIn: {}
    },
    stop: w.stop
  };
}

if (up) {
  if (await keptWorld()) fail(`a world is up already (${kept}); ctrl-c it first`);
  const { world, stop } = await newWorld();
  mkdirSync(join(root, 'target/look'), { recursive: true });
  writeFileSync(kept, JSON.stringify(world, null, 2));
  const end = () => {
    rmSync(kept, { force: true });
    stop();
    process.exit(0);
  };
  process.on('SIGINT', end);
  process.on('SIGTERM', end);
  // A browser to ready it with: the console's pages compiled once, so the
  // first look is no slower than the rest.
  const browser = await chromium.launch({ args: [resolving] });
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await page.goto(world.base, { waitUntil: 'networkidle' });
  await browser.close();
  say(`a world is up at ${world.base}, for each \`just look\` until ctrl-c`);
  await new Promise(() => {});
}

const own = (await keptWorld()) ? null : await newWorld();
const world = own?.world ?? (await keptWorld());
const save = () => {
  if (!own) writeFileSync(kept, JSON.stringify(world, null, 2));
};
const who = Object.fromEntries(world.accounts.map((a) => [a.handle.split('.')[0], a]));

const browser = await chromium.launch({ args: [resolving] });

/// A browser signed in as each of `names` in turn, at their PDS's own
/// page, as a person does, adding each after the first from the header's
/// menu: kept as the world keeps it, so each look after need not.
async function signedIn(names) {
  const key = names.join(',');
  if (world.signedIn[key]) return world.signedIn[key];
  const before = names.length > 1 ? await signedIn(names.slice(0, -1)) : undefined;
  const name = names.at(-1);
  const context = await browser.newContext({ ignoreHTTPSErrors: true, storageState: before });
  const page = await context.newPage();
  // Once its script has run, as a person's click comes: before, the
  // form posts itself, which the page's policy stops going elsewhere.
  await page.goto(world.base, { waitUntil: 'networkidle' });
  if (before) {
    await page.locator('.user-menu summary').click();
    await page.getByRole('button', { name: 'Add account' }).click();
  } else {
    await page.getByRole('banner').getByRole('button', { name: 'Sign in' }).click();
  }
  const dialog = page.getByRole('dialog', { name: 'Sign in with your Atmosphere account' });
  // Sent as typed, under any accounts the field suggests.
  const handle = dialog.getByRole('combobox', { name: 'Handle' });
  await handle.fill(who[name].handle);
  await handle.press('Enter');
  await page.waitForURL((u) => u.origin === new URL(world.pds).origin);
  await page.getByRole('textbox', { name: 'Password' }).fill(who[name].password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await page.getByRole('button', { name: /^(accept|authorize|allow)$/i }).click();
  await page.locator('.user-menu').waitFor({ timeout: 30_000 });
  world.signedIn[key] = await context.storageState();
  await context.close();
  save();
  return world.signedIn[key];
}

/// What a seed is given: ways to write to the world as its people do.
const tools = {
  world,
  who,
  /// The opensocial command, as `name`: what it prints.
  opensocial: (name, ...args) =>
    execFileSync(join(root, 'target/debug/opensocial'), args, {
      encoding: 'utf8',
      env: {
        ...process.env,
        OPENSOCIAL_HOST: world.host,
        OPENSOCIAL_HOST_DID: world.did,
        PDS_URL: world.pds,
        OPENSOCIAL_AS: who[name].handle,
        OPENSOCIAL_PASSWORD: who[name].password
      }
    }),
  /// Whether the group `handle` is here already: a seed that failed
  /// partway, run again, starts it only if not.
  exists: (handle) =>
    fetch(`${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=${handle}`).then(
      (r) => r.ok
    ),
  /// `name` asks to join the group `handle`, with `message` if given: let
  /// in at once where anyone may join, else waiting.
  ask: async (name, handle, message) => {
    const { did } = await fetch(
      `${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=${handle}`
    ).then((r) => r.json());
    const lxm = 'fyi.opensocial.requestJoin';
    const token = await serviceAuth(world.pds, who[name], world.did, lxm);
    const res = await fetch(`${world.host}/xrpc/${lxm}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
      body: JSON.stringify({ group: did, ...(message ? { message } : {}) })
    });
    const said = await res.json();
    // A seed run again, after one that failed partway, finds them in.
    if (said.error === 'AlreadyMember') return said;
    if (!res.ok) throw new Error(`${name} asking to join ${handle}: ${JSON.stringify(said)}`);
    return said;
  },
  /// A page of the console, signed in as `name`, for what only the
  /// console writes (a group's pictures): `page.goto(world.base + path)`;
  /// closed with `page.context().close()`.
  page: async (name) => {
    const context = await browser.newContext({
      ignoreHTTPSErrors: true,
      storageState: await signedIn([name])
    });
    return context.newPage();
  },
  /// A PNG of `width` by `height`, a wash from one hue to another, to give
  /// as a picture.
  picture: (width, height, from = [184, 72, 43], to = [246, 241, 234]) =>
    png(width, height, from, to)
};

for (const seed of seeds) {
  if (world.seeded.includes(seed)) {
    say(`--seed ${seed}: seeded already, in this world`);
    continue;
  }
  const file = /[/.]/.test(seed)
    ? isAbsolute(seed)
      ? seed
      : resolve(seed)
    : join(root, 'harness/look/seeds', `${seed}.mjs`);
  if (!existsSync(file)) fail(`--seed ${seed}: no such seed (${file})`);
  say(`seeding ${seed}`);
  await (await import(file)).default(tools);
  world.seeded.push(seed);
  save();
}

const storageState = as.length ? await signedIn(as) : undefined;
const named = (t) => name ?? (t.replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, '') || 'home');

mkdirSync(out, { recursive: true });
let shot = 0;
try {
  // Each size's clicks and typing done once, and each of its schemes shot
  // from the one page: a form a step sends is sent once, not per scheme.
  const sizes = Map.groupBy(projects, ([size]) => size);
  for (const [size, these] of sizes) {
    const schemes = these.map(([, scheme]) => scheme);
    const use = PROJECTS[size];
    const context = await browser.newContext({
      ...use,
      colorScheme: schemes[0],
      locale: 'en-US',
      ignoreHTTPSErrors: true,
      storageState
    });
    for (const target of targets) {
      const page = await context.newPage();
      await page.goto(world.base + target, { waitUntil: 'networkidle' });
      // Each in the middle of the screen, so what it opens is in the picture.
      for (const s of steps) {
        if (s.key) await page.keyboard.press(s.key);
        else {
          const it = page.locator(s.click ?? s.sel).first();
          await it.evaluate((e) => e.scrollIntoView({ block: 'center' }));
          if (s.sel) {
            await it.fill('');
            await it.pressSequentially(s.text);
          } else await it.click();
        }
        await page.waitForTimeout(300);
        await page.waitForLoadState('networkidle');
      }
      // A page wider than the screen scrolls sideways, which a picture of
      // the whole page hides: said, with the innermost elements whose box or
      // text runs past the screen's edge, which is where to look.
      const wide = await page.evaluate(() => {
        const w = document.documentElement.clientWidth;
        if (document.documentElement.scrollWidth <= w) return null;
        const past = (e) => {
          const r = e.getBoundingClientRect();
          return (
            r.right > w + 1 ||
            (getComputedStyle(e).overflowX === 'visible' && r.left + e.scrollWidth > w + 1)
          );
        };
        const over = [...document.body.querySelectorAll('*')].filter((e) => past(e));
        const inner = over.filter((e) => !over.some((o) => o !== e && e.contains(o)));
        return {
          by: document.documentElement.scrollWidth - w,
          at: inner
            .slice(0, 3)
            .map(
              (e) =>
                `${e.tagName.toLowerCase()}${[...e.classList].map((c) => `.${c}`).join('')} "${e.textContent.trim().slice(0, 30)}"`
            )
        };
      });
      if (wide)
        say(
          `${named(target)}-${size}: ${wide.by}px wider than the screen, at ${wide.at.join(', ')}`
        );
      if (text) {
        const file = join(
          out,
          `${named(target)}${as.length ? `-${as.join('-')}` : ''}-${size}.txt`
        );
        const it = element ? page.locator(element).first() : page.locator('body');
        writeFileSync(file, `${await it.ariaSnapshot()}\n`);
        console.log(file);
      }
      for (const scheme of schemes) {
        await page.emulateMedia({ colorScheme: scheme });
        const file = join(
          out,
          `${named(target)}${as.length ? `-${as.join('-')}` : ''}-${size}-${scheme}.png`
        );
        const opts = { path: file, animations: 'disabled', caret: 'hide' };
        if (element) await page.locator(element).first().screenshot(opts);
        else await page.screenshot({ ...opts, fullPage: !steps.length });
        if (use.deviceScaleFactor > 1)
          writeFileSync(file, dense(readFileSync(file), use.deviceScaleFactor));
        console.log(file);
        shot++;
      }
      await page.close();
    }
    await context.close();
  }
} finally {
  await browser.close();
  own?.stop();
}
say(`${shot} pictures in ${out}`);
process.exit(0);

/** `png` saying it was made at `scale` times 72 pixels to the inch, as a
 * Mac's screenshot does: a pull it is attached to then draws it at the
 * size it was seen, not at its pixels. */
function dense(png, scale) {
  const chunk = Buffer.alloc(13);
  chunk.write('pHYs');
  const perMetre = Math.round((scale * 72) / 0.0254);
  chunk.writeUInt32BE(perMetre, 4);
  chunk.writeUInt32BE(perMetre, 8);
  chunk[12] = 1;
  const len = Buffer.alloc(4);
  len.writeUInt32BE(9);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(chunk));
  // After the signature and the header, which come first.
  return Buffer.concat([png.subarray(0, 33), len, chunk, crc, png.subarray(33)]);
}

/** A `width` by `height` PNG washing from the colour `from` at its top
 * left to `to` at its bottom right. */
function png(width, height, from, to) {
  const rows = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y++) {
    const at = y * (width * 3 + 1);
    for (let x = 0; x < width; x++) {
      const t = (x / width + y / height) / 2;
      for (let c = 0; c < 3; c++)
        rows[at + 1 + x * 3 + c] = Math.round(from[c] + (to[c] - from[c]) * t);
    }
  }
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 2;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(rows)),
    chunk('IEND', Buffer.alloc(0))
  ]);
}
