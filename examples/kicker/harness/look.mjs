// `just look`: the site's pages as pictures in target/look, signed in as
// `--as` (a handle's first label, alice by default), over a world of their
// own with the place made in it: desktop in light, phone in dark.
//
//   node harness/look.mjs [--as alice]

import { chromium } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { resolving, root, siteFront, start, tlsFront } from './world.mjs';
import { seed } from './seed.mjs';

const at = process.argv.indexOf('--as');
const name = at === -1 ? 'alice' : process.argv[at + 1];
const out = join(root, 'target/look');
mkdirSync(out, { recursive: true });

const w = await start({ accounts: ['alice', 'bob', 'carol', 'dan', 'erin'], front: tlsFront });
try {
  const { handle } = await seed(w);
  const site = await siteFront(w.net, new URL(w.host).port, 'kicker.group-host.internal', handle);
  w.stop.proxy = site.proxy;
  const account = w.net.accounts.find((a) => a.handle.startsWith(`${name}.`));
  const browser = await chromium.launch({ args: [resolving] });
  for (const [view, options] of [
    ['desktop-light', { viewport: { width: 1200, height: 900 }, colorScheme: 'light' }],
    ['phone-dark', { viewport: { width: 390, height: 844 }, colorScheme: 'dark', isMobile: true }]
  ]) {
    const page = await (
      await browser.newContext({ ...options, ignoreHTTPSErrors: true })
    ).newPage();
    await page.goto(site.url);
    const frame = page.frameLocator('#os-page');
    // The landing, as anyone signed out sees it.
    await frame.getByRole('button', { name: 'Sign in' }).waitFor();
    await page.waitForTimeout(300);
    const landing = join(out, `landing-${view}.png`);
    await page.screenshot({ path: landing });
    console.log(landing);
    await frame.getByRole('button', { name: 'Sign in' }).click();
    const dialog = page.getByRole('dialog', { name: 'Sign in with your Atmosphere account' });
    await dialog.getByLabel('Handle').fill(account.handle);
    await dialog.getByRole('button', { name: 'Sign in' }).click();
    await page.waitForURL((u) => u.origin === new URL(w.net.pds).origin);
    await page.getByRole('textbox', { name: 'Password' }).fill(account.password);
    await page.getByRole('button', { name: 'Sign in' }).click();
    await page.getByRole('button', { name: /^(accept|authorize|allow)$/i }).click();
    await page.waitForURL((u) => u.origin === new URL(site.url).origin);
    await frame.getByRole('button', { name: 'Sign out' }).waitFor();
    const shoot = async (shot) => {
      await page.waitForTimeout(300);
      const file = join(out, `${shot}-as-${name}-${view}.png`);
      await page.screenshot({ path: file });
      console.log(file);
    };
    // Someone not let in yet (erin): waiting, then asking, its dialog
    // open, then waiting again as they started.
    if (await frame.getByText('Your request is waiting').isVisible()) {
      await shoot('waiting');
      await frame.getByRole('button', { name: 'Cancel request' }).click();
      await frame.getByRole('button', { name: 'Ask to join' }).waitFor();
      await shoot('asking');
      await frame.getByRole('button', { name: 'Ask to join' }).click();
      await frame.locator('dialog[open]').waitFor();
      await shoot('ask-dialog');
      await frame.getByRole('button', { name: 'Send request' }).click();
      await frame.getByText('Your request is waiting').waitFor();
      continue;
    }
    // Each room whole, then a discussion opened from its list.
    const whole = async (file, ready) => {
      await ready();
      await page.waitForTimeout(300);
      // The frame fills the window; its page scrolls inside it.
      const height = await frame.locator('body').evaluate((b) => b.scrollHeight);
      await page.setViewportSize({
        width: options.viewport.width,
        height: Math.max(height, options.viewport.height)
      });
      await page.screenshot({ path: file });
      await page.setViewportSize(options.viewport);
      console.log(file);
    };
    for (const path of ['', 'events/', 'discussion/', 'people/']) {
      await page.goto(`${site.url}/${path}`);
      const file = join(out, `${path.replace('/', '') || 'board'}-as-${name}-${view}.png`);
      await whole(file, () => frame.locator('h1').waitFor());
    }
    await page.goto(`${site.url}/discussion/`);
    await frame.getByRole('link', { name: 'Should we build a mini ramp in the back lot?' }).click();
    await whole(join(out, `thread-as-${name}-${view}.png`), () =>
      frame.locator('li.reply').first().waitFor()
    );
    // The dialogs, as seen: an event opened, pinning a note, and leaving.
    for (const [path, open, shot] of [
      ['events/', () => frame.locator('li', { hasText: 'Curb jam' }).getByRole('button'), 'event'],
      ['', () => frame.getByRole('button', { name: 'Pin a note' }), 'pin'],
      ['people/', () => frame.getByRole('button', { name: 'Leave' }), 'leave']
    ]) {
      await page.goto(`${site.url}/${path}`);
      await frame.locator('h1').waitFor();
      await open().click();
      await frame.locator('dialog[open]').waitFor();
      await page.waitForTimeout(300);
      const file = join(out, `${shot}-dialog-as-${name}-${view}.png`);
      await page.screenshot({ path: file });
      console.log(file);
    }
    // A note's menu, opened: the off-topic one, which every view has.
    await page.goto(site.url);
    const marked = frame.locator('li.note', { hasText: 'Off-topic' });
    await marked.getByRole('button', { name: 'What to do with this note' }).click();
    await frame.getByRole('menu').waitFor();
    const menu = join(out, `menu-as-${name}-${view}.png`);
    await page.screenshot({ path: menu });
    console.log(menu);
  }
  await browser.close();
  site.proxy.kill('SIGTERM');
} finally {
  w.stop();
}
process.exit(0);
