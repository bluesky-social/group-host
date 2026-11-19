// The site as people use it, with nothing but the group host behind it:
// each signs in on it with OAuth at their own PDS, granting the writes the
// site declares, and each write here is made at their PDS, in their own
// repo in the place's spaces, and read back through the host.

import { test, expect } from '@playwright/test';
import { portrait, poster } from '../posters.mjs';
import { serviceAuth } from '../world.mjs';

const world = JSON.parse(process.env.E2E_WORLD);
const who = Object.fromEntries(world.accounts.map((a) => [a.handle.split('.')[0], a]));

test.describe.configure({ mode: 'serial' });

/// How wide the picture `img` was drawn, once loaded: 0 until it is.
const drawn = (img) => img.evaluate((el) => (el.complete ? el.naturalWidth : 0)).catch(() => 0);

/// `page` signed in on the site as `name`, with the host's dialog, at
/// their own PDS, approving what the site asks.
const signIn = async (page, name) => {
  await page.goto(world.site);
  const site = page.frameLocator('#os-page');
  await site.getByRole('button', { name: 'Sign in' }).click();
  const dialog = page.getByRole('dialog', { name: 'Sign in with your Atmosphere account' });
  await dialog.getByLabel('Handle').fill(who[name].handle);
  await dialog.getByRole('button', { name: 'Sign in' }).click();
  await page.waitForURL((u) => u.origin === new URL(world.pds).origin);
  await page.getByRole('textbox', { name: 'Password' }).fill(who[name].password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await page.getByRole('button', { name: /^(accept|authorize|allow)$/i }).click();
  await page.waitForURL((u) => u.origin === new URL(world.site).origin);
  await expect(site.getByRole('button', { name: 'Sign out' })).toBeVisible();
  return site;
};

test('signed out, the place shows its landing, and the way in', async ({ page }) => {
  await page.goto(world.site);
  const site = page.frameLocator('#os-page');
  await expect(site.getByText('214 Lantern Alley')).toBeVisible();
  await expect(site.getByRole('button', { name: 'Sign in' })).toBeVisible();
  await expect(site.getByRole('link', { name: 'Events' })).toHaveCount(0);
  await expect(page).toHaveTitle('Board');
});

test('someone asking to join is not let in yet', async ({ page }) => {
  const site = await signIn(page, 'erin');
  await expect(site.getByText('Not a member yet')).toBeVisible();
  await expect(site.getByText('@erin.test')).toBeVisible();
  await expect(site.getByRole('link', { name: 'Events' })).toHaveCount(0);
});

test('a member pins a note, answers an event and writes her card', async ({ page }) => {
  const site = await signIn(page, 'dan');
  await expect(site.getByText("Beginners' Sunday is back")).toBeVisible();

  await site.getByRole('button', { name: 'Pin a note' }).click();
  const pinning = site.getByRole('dialog', { name: 'Pin a note' });
  await pinning.getByLabel('Note').fill('Spare bolts live in the second drawer.');
  await pinning.getByRole('button', { name: 'Pin', exact: true }).click();
  await expect(pinning).toBeHidden();
  await expect(site.getByText('Spare bolts live in the second drawer.')).toBeVisible();

  // Moving between rooms keeps the page: the app switches the room in
  // place, loading nothing again.
  const app = page.frames().find((f) => f.url().includes('/_site/'));
  await app.evaluate(() => (window.stayed = true));
  await site.getByRole('link', { name: 'Events' }).click();
  await expect(page).toHaveTitle('Events');
  await expect(page).toHaveURL(/\/events\/$/);
  expect(await app.evaluate(() => window.stayed)).toBe(true);
  // A member, no host, has no way to publish one.
  await expect(site.getByRole('button', { name: 'New event' })).toHaveCount(0);
  // Its poster, from its host's PDS through the place's host, is drawn.
  const card = site.locator('li', { hasText: 'Curb jam' });
  await expect.poll(() => drawn(card.getByRole('img'))).toBeGreaterThan(0);
  await card.getByRole('button').click();
  const jam = site.getByRole('dialog', { name: 'Event' });
  await expect(jam.getByRole('heading', { name: 'Curb jam' })).toBeVisible();
  await expect(jam.getByRole('img', { name: /A poster for Curb jam/ })).toBeVisible();
  await jam.getByRole('button', { name: 'Going' }).click();
  await expect(jam.getByRole('button', { name: 'Going' })).toHaveAttribute('aria-pressed', 'true');
  await expect(jam).toContainText('@dan.test');
  await jam.getByRole('button', { name: 'Close' }).click();
  await expect(jam).toBeHidden();

  await site.getByRole('link', { name: 'People' }).click();
  await site.getByRole('button', { name: 'Edit your card' }).click();
  const editing = site.getByRole('dialog', { name: 'Edit your card' });
  await editing.getByLabel('About').fill('Mostly curbs, some bowls.');
  // A picture on it, uploaded to dan's own PDS and read back through the host.
  await editing.getByLabel('Your picture').setInputFiles({
    name: 'dan.svg',
    mimeType: 'image/svg+xml',
    buffer: Buffer.from(portrait('dan', 'pink', 'blue'))
  });
  await editing.getByRole('button', { name: 'Save' }).click();
  await expect(editing).toBeHidden();
  const dan = site.locator('.rows li', { hasText: '@dan.test' });
  await expect(dan).toContainText('Mostly curbs, some bowls.');
  await expect.poll(() => drawn(dan.locator('img'))).toBeGreaterThan(0);

  // Its words changed again, its picture stays; taken off, it is gone.
  await site.getByRole('button', { name: 'Edit your card' }).click();
  await editing.getByLabel('About').fill('Curbs, bowls, and filming.');
  await editing.getByRole('button', { name: 'Save' }).click();
  await expect(dan).toContainText('Curbs, bowls, and filming.');
  await expect.poll(() => drawn(dan.locator('img'))).toBeGreaterThan(0);
  await site.getByRole('button', { name: 'Edit your card' }).click();
  await editing.getByLabel('Take my picture off').check();
  await editing.getByRole('button', { name: 'Save' }).click();
  await expect(editing).toBeHidden();
  await expect(dan.locator('img')).toHaveCount(0);
});

test('a host publishes an event', async ({ page }) => {
  const site = await signIn(page, 'bob');
  await site.getByRole('link', { name: 'Events' }).click();
  await site.getByRole('button', { name: 'New event' }).click();
  const making = site.getByRole('dialog', { name: 'New event' });
  await making.getByLabel('Name').fill('Bowl day');
  await making.getByLabel('Starts').fill('2099-05-01T18:00');
  // Its poster, uploaded to bob's own PDS with the event.
  await making.getByLabel('Poster or photo').setInputFiles({
    name: 'bowl.svg',
    mimeType: 'image/svg+xml',
    buffer: Buffer.from(poster('Bowl day', 'teal', 'pink'))
  });
  await making.getByLabel('What it shows').fill('A poster for bowl day.');
  await making.getByRole('button', { name: 'Publish' }).click();
  await expect(making).toBeHidden();
  const card = site.locator('li', { hasText: 'Bowl day' });
  await expect(card.getByRole('img', { name: 'A poster for bowl day.' })).toBeVisible();
  await expect.poll(() => drawn(card.getByRole('img'))).toBeGreaterThan(0);
});

test('a member reads a discussion, and replies to a reply in it', async ({ page }) => {
  const site = await signIn(page, 'dan');
  await site.getByRole('link', { name: 'Discussion' }).click();
  await expect(page).toHaveTitle('Discussion');
  // Each with how many replies it has, all counted in one read.
  const ramp = site.locator('li.topic', { hasText: 'Should we build a mini ramp' });
  await expect(ramp).toContainText('3 replies');
  await expect(site.locator('li.topic', { hasText: 'A video night' })).toContainText('2 replies');
  // The most lately active first: the video night's replies came last.
  await expect(site.locator('li.topic').first()).toContainText('A video night');

  await ramp.getByRole('link').click();
  await expect(page).toHaveTitle('Should we build a mini ramp in the back lot?');
  await expect(page).toHaveURL(/\/discussion\/did:[^/]+\/[^/]+$/);
  await expect(site.getByRole('heading', { name: '3 replies' })).toBeVisible();
  // Oldest first, each under what it answers: bob's under dan's, alice's not.
  const fund = site.locator('li.reply', { hasText: 'twenty for plywood' });
  await expect(fund).toContainText('coping from my old ramp');
  await expect(fund).not.toContainText('Thursday');

  const coping = site.locator('li.reply', { hasText: 'coping from my old ramp' }).last();
  await coping.getByRole('button', { name: 'Reply' }).click();
  await coping.getByLabel('Your reply').fill('Count me in for Sunday.');
  await coping.locator('.reply-form').getByRole('button', { name: 'Reply' }).click();
  await expect(coping.locator('li.reply', { hasText: 'Count me in' })).toBeVisible();
  await expect(site.getByRole('heading', { name: '4 replies' })).toBeVisible();

  await site.getByRole('link', { name: 'All discussions' }).click();
  await expect(ramp).toContainText('4 replies');
  // Replied to, it is the most lately active now, the last reply just now.
  await expect(site.locator('li.topic').first()).toContainText('Should we build a mini ramp');
  await expect(ramp).toContainText('last reply just now');
});

test('a member starts a discussion', async ({ page }) => {
  const site = await signIn(page, 'carol');
  await site.getByRole('link', { name: 'Discussion' }).click();
  await site.getByRole('button', { name: 'Start a discussion' }).click();
  const starting = site.getByRole('dialog', { name: 'Start a discussion' });
  await starting.getByLabel('What about').fill('Who has a key for the side door?');
  await starting.getByRole('button', { name: 'Start' }).click();
  await expect(starting).toBeHidden();
  const started = site.locator('li.topic', { hasText: 'Who has a key for the side door?' });
  await expect(started).toContainText('No replies yet');
});

/// `lxm` of the group host, asked as alice, who moderates the place.
const asAlice = async (lxm, params) => {
  const token = await serviceAuth(world.pds, who.alice, world.hostDid, lxm);
  const query = new URLSearchParams({ group: world.place, ...params });
  const res = await fetch(`${world.host}/xrpc/${lxm}?${query}`, {
    headers: { authorization: `Bearer ${token}` }
  });
  return await res.json();
};
/// The URI of the note on the board whose words begin `text`.
const noteUri = async (text) => {
  const space = `at://${world.place}/space/com.example.place.room/self`;
  const collection = 'com.example.place.note';
  const { records } = await asAlice('fyi.opensocial.listSpaceRecords', { space, collection });
  return records.find((r) => r.value.text.startsWith(text)).uri;
};
/// What the group host's queue holds of `uri`, of the subjects `status`.
const queued = async (uri, status = 'open') => {
  const { subjects } = await asAlice('fyi.opensocial.listSubjects', { status });
  return subjects.find((s) => s.subject.uri === uri);
};

test("a member reports a note, into the group host's queue", async ({ page }) => {
  const site = await signIn(page, 'carol');
  const bolts = site.locator('li.note', { hasText: 'Spare bolts live in the second drawer.' });
  await bolts.getByRole('button', { name: 'What to do with this note' }).click();
  await site.getByRole('menuitem', { name: 'Report' }).click();
  const reporting = site.getByRole('dialog', { name: 'Report this note' });
  await reporting.getByLabel('Why').selectOption({ label: 'Spam' });
  await reporting.getByLabel('In your words').fill('Is this an ad?');
  await reporting.getByRole('button', { name: 'Report' }).click();
  await expect(reporting.getByText('Sent to the moderators.')).toBeVisible();
  // The way to the console is a moderator's alone.
  await expect(reporting.getByRole('link', { name: 'Review it in the console' })).toHaveCount(0);
  await reporting.getByRole('button', { name: 'Done' }).click();
  const reported = await queued(await noteUri('Spare bolts live'));
  expect(reported?.reportCount).toBe(1);
});

test('a note the moderators marked is folded, and one they hid is gone', async ({ page }) => {
  const site = await signIn(page, 'dan');
  const bike = site.locator('li.note', { hasText: 'Marked by the moderators' });
  await expect(bike).toContainText('Off-topic');
  await expect(bike).not.toContainText('road bike');
  await bike.getByRole('button', { name: 'Show it' }).click();
  await expect(bike).toContainText('Selling my old road bike');
  // Hidden, it is not on the board at all, nor any sign it was.
  await expect(site.getByText('Cheap bearings')).toHaveCount(0);
  await expect(site.getByText(/hidden/i)).toHaveCount(0);
});

test('its author appeals, and a moderator reviews it in the console', async ({ page }) => {
  const uri = await noteUri('Selling my old road bike');
  expect(await queued(uri, 'resolved')).toBeTruthy();
  let site = await signIn(page, 'carol');
  const bike = site.locator('li.note', { hasText: 'Selling my old road bike' });
  await expect(bike).toContainText('Off-topic');
  await bike.getByRole('button', { name: 'What to do with this note' }).click();
  await site.getByRole('menuitem', { name: 'Appeal' }).click();
  const appealing = site.getByRole('dialog', { name: 'Appeal' });
  await appealing.getByLabel('Your appeal').fill('Plenty of us ride here; it is a fair offer.');
  await appealing.getByRole('button', { name: 'Send' }).click();
  await expect(appealing.getByText('Sent.')).toBeVisible();
  await appealing.getByRole('button', { name: 'Done' }).click();
  // The appeal opened it again, in front of the moderators.
  expect(await queued(uri)).toBeTruthy();

  await site.getByRole('button', { name: 'Sign out' }).click();
  await expect(site.getByRole('button', { name: 'Sign in' })).toBeVisible();
  site = await signIn(page, 'alice');
  const marked = site.locator('li.note', { hasText: 'Selling my old road bike' });
  await marked.getByRole('button', { name: 'What to do with this note' }).click();
  await expect(site.getByRole('menuitem', { name: 'Review in the console' })).toHaveAttribute(
    'href',
    `${world.console}/g/kicker.groups.test/moderation?review=${encodeURIComponent(uri)}`
  );
  // A moderator sees what is hidden from members, and that it is.
  const ink = site.locator('li.note', { hasText: 'Cheap bearings' });
  await expect(ink).toContainText('Hidden from members by the moderators.');
});
