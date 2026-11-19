// What people do on the console, in order, as alice: sign in at her own
// PDS, see her groups, sign an app in as one of them, and decline one;
// and what the app then does as the group, read back from the host.

import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import {
  App,
  McpClient,
  approve,
  didKey,
  hydrating,
  joins,
  signIn,
  signInDialog,
  signedInAs,
  suggests,
  url,
  who,
  world
} from './pages.mjs';

test.describe.configure({ mode: 'serial' });

let page;
// Bob's browser, apart from alice's.
let bob;

test.beforeAll(async ({ browser }) => {
  page = await browser.newPage();
  bob = await (await browser.newContext()).newPage();
  hydrating(page);
  hydrating(bob);
  await suggests(page.context());
  await suggests(bob.context());
});

test.afterAll(async () => {
  await page.close();
  await bob.context().close();
});

test('coming to the host signed out', async () => {
  await page.goto(url('/'));
  // The host by its address, what signing in is for, and no groups.
  const host = new URL(world.base).hostname;
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(`Groups on ${host}`);
  await expect(
    page.getByText('Sign in with your Atmosphere account to see your groups, or to start one.')
  ).toBeVisible();
  await expect(page.locator('.group-row')).toHaveCount(0);
  await expect(page.getByRole('contentinfo')).toHaveText(`${host} runs opensocial`);
  // Its one button opens the dialog: the apps of the Atmosphere, what an
  // account is, the handle, and where to make one.
  await page.getByRole('main').getByRole('button', { name: 'Sign in' }).click();
  const dialog = signInDialog(page);
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('.apps img')).toHaveCount(40);
  await expect(
    dialog.getByRole('link', { name: 'What is an Atmosphere account?' })
  ).toHaveAttribute('href', 'https://atmosphereaccount.com/');
  await expect(dialog.getByRole('combobox', { name: 'Handle' })).toBeFocused();
  // Made at the host's sign-up PDS, coming back here signed in.
  await expect(dialog.getByRole('link', { name: 'Create one' })).toHaveAttribute(
    'href',
    '/console/login?create&back=%2F'
  );
  await expect(dialog.getByRole('combobox', { name: 'Handle' })).toHaveAttribute(
    'placeholder',
    'you.bsky.social'
  );
  // Two letters of a handle suggest the accounts it is part of; one, none.
  const handle = dialog.getByRole('combobox', { name: 'Handle' });
  await handle.pressSequentially('a');
  await expect(handle).toHaveAttribute('aria-expanded', 'false');
  await handle.pressSequentially('l');
  await expect(dialog.getByRole('option')).toHaveText(['Alice @alice.test']);
  await expect(handle).toHaveAttribute('aria-expanded', 'true');
  // Escape closes the list, not the dialog; what the arrows chose is let
  // go, so Enter after more typing sends what is typed.
  await handle.press('ArrowDown');
  await expect(dialog.getByRole('option')).toHaveAttribute('aria-selected', 'true');
  await handle.press('Escape');
  await expect(dialog.getByRole('listbox')).toBeHidden();
  await expect(dialog).toBeVisible();
  await handle.pressSequentially('i');
  await expect(dialog.getByRole('option')).toHaveAttribute('aria-selected', 'false');
  await handle.press('Escape');
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toBeHidden();
});

test("a group's page signed out", async () => {
  // Its handle alone, and the way in, wherever its access record lets
  // anyone signed in read it.
  await page.goto(url('/g/riders.groups.test'));
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('riders.groups.test');
  await expect(page.getByText('Sign in to see this group.')).toBeVisible();
  await expect(page.locator('.identity, .rules, .standing')).toHaveCount(0);
  // Nor does the host tell anyone not signed in more of it.
  const { group } = await fetch(
    `${world.host}/xrpc/fyi.opensocial.getGroup?group=${world.riders}`
  ).then((r) => r.json());
  expect(group).toEqual({ did: world.riders, handle: 'riders.groups.test' });
});

test("signing in at her own PDS, from a group's page", async () => {
  // Signed in from the page by picking her account as she types it, with
  // the arrows and Enter; back on it, as she may see it.
  await page.getByRole('main').getByRole('button', { name: 'Sign in' }).click();
  const handle = signInDialog(page).getByRole('combobox', { name: 'Handle' });
  await handle.pressSequentially('alic');
  await expect(signInDialog(page).getByRole('option', { name: 'Alice @alice.test' })).toBeVisible();
  await handle.press('ArrowDown');
  await expect(
    signInDialog(page).getByRole('option', { name: 'Alice @alice.test' })
  ).toHaveAttribute('aria-selected', 'true');
  await handle.press('Enter');
  await approve(page, 'alice');
  await page.waitForURL(url('/g/riders.groups.test'));
  await expect(page).toHaveURL(url('/g/riders.groups.test'));
  await expect.poll(() => signedInAs(page)).toBe('@alice.test');
  await expect(page.getByText('Sign in to see this group.')).toHaveCount(0);
  await expect(page.getByText(/You're a member/)).toBeVisible();
  await page.goto(url('/'));
});

test('seeing her groups', async () => {
  await expect(page.getByRole('heading', { name: 'Your groups' })).toBeVisible();
  await expect(page.locator('.group-row', { hasText: 'Riders' }).first()).toBeVisible();
});

test('coming back to her groups by a link, the page drawn in the browser', async () => {
  await page.goto(url('/new'));
  await page.getByRole('link', { name: 'opensocial' }).click();
  await page.waitForURL(url('/'));
  await expect(page.getByRole('heading', { name: 'Your groups' })).toBeVisible();
  await expect(page.locator('.group-row', { hasText: 'Riders' }).first()).toBeVisible();
});

test('starting a group', async () => {
  await page.getByRole('link', { name: 'Start a group' }).click();
  // Its address made from its name, under the host's domain.
  await page.getByRole('textbox', { name: 'Name' }).pressSequentially('Hikers');
  await expect(page.getByRole('textbox', { name: 'Its address' })).toHaveValue('hikers');
  await expect(page.getByText('.groups.test', { exact: true })).toBeVisible();
  await page.getByRole('textbox', { name: 'What it is for' }).fill('Walks on Sundays.');
  await page.getByRole('button', { name: 'Start the group' }).click();
  await page.waitForURL(url('/g/hikers.groups.test'));
  await expect(page.getByRole('heading', { name: 'Hikers' })).toBeVisible();
  await expect(page.getByText('Walks on Sundays.')).toBeVisible();
  await expect(page.getByText(/You're a member/)).toBeVisible();
  // An address taken already is said as it is typed, with the next free
  // one to take; and the host's to refuse, sent all the same.
  await page.goto(url('/new'));
  await page.getByRole('textbox', { name: 'Name' }).pressSequentially('Hikers');
  await expect(page.getByText('hikers.groups.test is taken.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Use hikers-2.groups.test' })).toBeVisible();
  // Its answer waited for: the note that it is taken is p.danger already,
  // and a page left before the answer lands is drawn over by it.
  const answered = page.waitForResponse(
    (r) => r.request().method() === 'POST' && r.url().includes('/new')
  );
  await page.getByRole('button', { name: 'Start the group' }).click();
  await answered;
  await page.waitForLoadState();
  await expect(page.locator('p.danger')).toBeVisible();
  // Its address changed by hand stays as it was put, whatever the name.
  await page.goto(url('/new'));
  await page.getByRole('textbox', { name: 'Its address' }).pressSequentially('Sunday Walks!');
  await page.getByRole('textbox', { name: 'Name' }).pressSequentially('Hikers');
  await expect(page.getByRole('textbox', { name: 'Its address' })).toHaveValue('sunday-walks-');
});

test('asking to join with a note, and being approved', async () => {
  await bob.goto(url('/g/hikers.groups.test'));
  await signIn(bob, 'bob');
  // The group, and how joining it works: someone looks at each request.
  await expect(bob.getByRole('heading', { name: 'Hikers', exact: true })).toBeVisible();
  await expect(bob.getByText('Joining is by request.')).toBeVisible();
  await expect(bob.getByRole('heading', { name: 'Ask to join' })).toBeVisible();
  const note = bob.getByRole('textbox', { name: 'Note' });
  await note.fill('I walk most Sundays.');
  await bob.getByRole('button', { name: 'Send request' }).click();
  await expect(bob.getByRole('heading', { name: 'Your request is waiting' })).toBeVisible();
  // Withdrawn, and asked again.
  await bob.getByRole('button', { name: 'Cancel request' }).click();
  await expect(bob.getByRole('heading', { name: 'Ask to join' })).toBeVisible();
  await note.fill('I walk most Sundays.');
  await bob.getByRole('button', { name: 'Send request' }).click();
  await expect(bob.getByRole('heading', { name: 'Your request is waiting' })).toBeVisible();
  // Alice sees it, with his note, and lets him in.
  await page.goto(url('/g/hikers.groups.test'));
  const asking = page.locator('.requests li', { hasText: 'bob.test' });
  await expect(asking).toContainText('I walk most Sundays.');
  await asking.getByRole('button', { name: 'Approve' }).click();
  await expect(page.locator('.members', { hasText: 'bob.test' })).toBeVisible();
  await expect(page.getByText('Nobody is asking to join')).toBeVisible();
  await bob.goto(url('/g/hikers.groups.test'));
  await expect(bob.getByText(/You're a member/)).toBeVisible();
});

test('leaving a group', async () => {
  // Asked to be sure of; cancelled, he is still in.
  const sure = bob.getByRole('dialog', { name: 'Leave this group?' });
  await bob.getByRole('button', { name: 'Leave the group' }).click();
  await sure.getByRole('button', { name: 'Cancel' }).click();
  await expect(sure).toBeHidden();
  await bob.getByRole('button', { name: 'Leave the group' }).click();
  await sure.getByRole('button', { name: 'Leave the group' }).click();
  await expect(bob.getByRole('heading', { name: 'Ask to join' })).toBeVisible();
});

/** Alice invites bob to Hikers from its page, picking him as she types, with a note. */
async function send() {
  const dialog = page.getByRole('dialog', { name: 'Invite someone' });
  await page.getByRole('button', { name: 'Invite someone' }).click();
  const handle = dialog.getByRole('combobox', { name: 'Their handle' });
  await handle.pressSequentially('bo');
  await dialog.getByRole('option', { name: 'Bob @bob.test' }).click();
  await expect(handle).toHaveValue('bob.test');
  await expect(dialog.getByRole('listbox')).toBeHidden();
  await dialog.getByRole('textbox', { name: 'Note' }).fill('Come walk with us.');
  await dialog.getByRole('button', { name: 'Send invite' }).click();
  await expect(page.locator('.invites', { hasText: 'bob.test' })).toBeVisible();
}

test('inviting someone from the dialog', async () => {
  await page.goto(url('/g/hikers.groups.test'));
  const dialog = page.getByRole('dialog', { name: 'Invite someone' });
  const invite = page.getByRole('button', { name: 'Invite someone' });
  await invite.click();
  await expect(
    dialog.getByText(
      "They'll see it the next time they sign in, here or in any app that shows invites."
    )
  ).toBeVisible();
  await dialog.getByRole('combobox', { name: 'Their handle' }).fill('nobody.test');
  await dialog.getByRole('button', { name: 'Send invite' }).click();
  // Refused, it stays open, saying why, with what was typed.
  await expect(dialog.getByText("We can't find nobody.test.")).toBeVisible();
  await expect(dialog.getByRole('combobox', { name: 'Their handle' })).toHaveValue('nobody.test');
  // Cancelled, it sends nothing.
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toBeHidden();
  await expect(page.locator('.invites')).toHaveCount(0);
  // Sent, with a note, picked from the accounts suggested as part of his
  // handle is typed; cancelled; and sent again.
  await send();
  await page.getByRole('button', { name: 'Cancel invite' }).click();
  await expect(page.locator('.invites', { hasText: 'bob.test' })).toHaveCount(0);
  await send();
});

test("turning down an invitation from the group's page", async () => {
  // Marked on every page: a dot on his picture, and a count in his menu,
  // leading to it on his own page, with its note.
  await bob.goto(url('/new'));
  await expect(bob.locator('.user-menu .waiting.dot')).toBeVisible();
  await bob.locator('.user-menu summary').click();
  await bob.locator('.menu').getByRole('link', { name: 'Invites 1' }).click();
  await bob.waitForURL(url('/#invites'));
  await expect(
    bob.locator('#invites').getByRole('heading', { name: 'Invites waiting' })
  ).toBeVisible();
  await expect(bob.locator('#invites').getByText('Come walk with us.')).toBeVisible();
  // Declined on the group's page, asked nothing: it then says how
  // joining works for anyone not invited.
  await bob.goto(url('/g/hikers.groups.test'));
  await expect(bob.getByRole('heading', { name: "You're invited" })).toBeVisible();
  await bob.getByRole('button', { name: 'Decline' }).click();
  await expect(bob.getByRole('heading', { name: 'Ask to join' })).toBeVisible();
  await expect(bob.locator('.user-menu .waiting.dot')).toHaveCount(0);
  // None waiting: his own page says so, and his menu counts none.
  await bob.goto(url('/'));
  await expect(bob.locator('#invites').getByText('No invites waiting')).toBeVisible();
  await bob.locator('.user-menu summary').click();
  await expect(
    bob.locator('.menu').getByRole('link', { name: 'Invites', exact: true })
  ).toBeVisible();
  await page.goto(url('/g/hikers.groups.test'));
  await expect(page.locator('.invites', { hasText: 'bob.test' })).toHaveCount(0);
});

test('taking up an invitation', async () => {
  await send();
  // On his own page, with its note; and on the group's.
  await bob.goto(url('/'));
  await expect(bob.locator('#invites').getByText('Come walk with us.')).toBeVisible();
  await bob.goto(url('/g/hikers.groups.test'));
  await expect(bob.getByRole('heading', { name: "You're invited" })).toBeVisible();
  await expect(bob.getByText('Someone in Hikers invited you to join.')).toBeVisible();
  await bob.getByRole('button', { name: 'Join', exact: true }).click();
  await expect(bob.getByRole('heading', { name: "You're in" })).toBeVisible();
  await expect(bob.getByText('Welcome to Hikers.')).toBeVisible();
  await expect(bob.getByText(/You're a member/)).toBeVisible();
  await bob.goto(url('/'));
  await expect(bob.locator('#invites').getByText('No invites waiting')).toBeVisible();
  await expect(bob.locator('.group-row', { hasText: 'Hikers' }).first()).toBeVisible();
  await page.goto(url('/g/hikers.groups.test'));
  await expect(page.locator('.members', { hasText: 'bob.test' })).toBeVisible();
});

test('a member with nothing to change sees no settings', async () => {
  // Bob, a member and no more: no way to its settings on its page.
  await bob.goto(url('/g/hikers.groups.test'));
  await expect(bob.getByText(/You're a member/)).toBeVisible();
  await expect(bob.getByRole('link', { name: 'Settings' })).toHaveCount(0);
  // At their addresses all the same, nothing is his, and the way back.
  for (const part of ['', '/rules', '/apps']) {
    await bob.goto(url(`/g/hikers.groups.test/settings${part}`));
    await expect(bob.getByText('Nothing here is yours to change')).toBeVisible();
  }
  await bob.getByRole('link', { name: 'Back to Hikers' }).click();
  await bob.waitForURL(url('/g/hikers.groups.test'));
});

test('giving roles, and putting someone out', async () => {
  const row = () => page.locator('.member', { hasText: 'bob.test' });
  // His roles in a dialog, those she may give to tick.
  // From the "⋯" at the end of his row.
  await row().getByLabel('More for bob.test').click();
  await row().getByRole('button', { name: 'Change roles' }).click();
  const roles = page.getByRole('dialog', { name: 'Change roles' }).filter({ visible: true });
  await expect(roles.getByRole('checkbox', { name: 'member' })).toBeChecked();
  // Saved in place: the dialog closes, and the page stays where it was.
  const at = await page.evaluate(() => scrollY);
  expect(at).toBeGreaterThan(0);
  await roles.getByRole('button', { name: 'Save' }).click();
  await expect(roles).toBeHidden();
  await expect(row()).toContainText('member');
  expect(await page.evaluate(() => scrollY)).toBe(at);
  // Removed, once she is sure.
  await row().getByLabel('More for bob.test').click();
  await row().getByRole('button', { name: 'Remove from the group' }).click();
  const sure = page.getByRole('dialog', { name: 'Remove this member?' }).filter({ visible: true });
  await expect(sure).toContainText('bob.test');
  await expect(sure.getByText('They can ask to join again.')).toBeVisible();
  await sure.getByRole('button', { name: 'Remove', exact: true }).click();
  await expect(page.locator('.members', { hasText: 'bob.test' })).toHaveCount(0);
});

test('being told why the host refused', async () => {
  // The group's only admin may not leave it: the page says so.
  await page.getByRole('button', { name: 'Leave the group' }).click();
  await page
    .getByRole('dialog', { name: 'Leave this group?' })
    .getByRole('button', { name: 'Leave the group' })
    .click();
  await expect(page.locator('p.danger')).toBeVisible();
  await expect(page.getByText(/You're a member/)).toBeVisible();
});

test('joining a group anyone can join', async () => {
  // Alice opens it to anyone.
  const settings = url('/g/hikers.groups.test/settings');
  await page.goto(settings);
  await page.getByRole('combobox', { name: 'How people join' }).selectOption('open');
  const profile = page.locator('form[action="?/profile"]');
  await profile.getByRole('button', { name: 'Save' }).click();
  await expect(profile.getByRole('status')).toHaveText('Saved');
  // Bob, removed, joins it at once.
  await bob.goto(url('/g/hikers.groups.test'));
  await expect(bob.getByRole('heading', { name: 'Join Hikers' })).toBeVisible();
  await expect(bob.locator('.join')).toContainText('Anyone can join.');
  await bob.getByRole('button', { name: 'Join', exact: true }).click();
  await expect(bob.getByRole('heading', { name: "You're in" })).toBeVisible();
  await expect(bob.getByText(/You're a member/)).toBeVisible();
});

test('finding a member, and showing more', async () => {
  // Six in all: more than a page of five.
  for (const name of ['carol', 'dan', 'erin', 'frank']) await joins(name, 'hikers.groups.test');
  await page.goto(url('/g/hikers.groups.test'));
  await expect(page.getByRole('heading', { name: 'Members' })).toContainText('6');
  const rows = page.locator('.members > li');
  await expect(rows).toHaveCount(5);
  await page.getByRole('button', { name: 'Show more' }).click();
  await expect(rows).toHaveCount(6);
  await expect(page.getByRole('button', { name: 'Show more' })).toHaveCount(0);
  // Found by part of a handle, as it is typed.
  const find = page.getByRole('searchbox', { name: 'Find a member' });
  await find.fill('erin');
  await expect(rows).toHaveCount(1);
  await expect(rows).toContainText('erin.test');
  await expect(find).toBeFocused();
  await find.fill('nobody');
  await expect(page.getByText("No member's handle has that in it")).toBeVisible();
});

test('connecting an AI app is shown to those who run the group', async () => {
  await page.goto(url('/g/hikers.groups.test'));
  const connect = page.locator('section', {
    has: page.getByRole('heading', { name: 'Connect an AI app' })
  });
  await expect(connect).toContainText(
    'It acts for you in Hikers, with what your roles let you do.'
  );
  await expect(connect.locator('code')).toHaveText(`${world.base}/mcp`);
  await expect(connect).toContainText('When it asks, sign in here and approve it');
  await expect(connect.locator('.asks li')).toHaveCount(3);
  // Not to a plain member.
  await bob.goto(url('/g/hikers.groups.test'));
  await expect(bob.getByText(/You're a member/)).toBeVisible();
  await expect(bob.getByRole('heading', { name: 'Connect an AI app' })).toHaveCount(0);
});

test("changing a group's profile", async () => {
  await page.goto(url('/g/hikers.groups.test'));
  await page.getByRole('link', { name: 'Settings' }).click();
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Hikers of the Hills');
  // Where it meets: a town asks for its country.
  await page.getByRole('textbox', { name: 'Town or city it meets in' }).fill('Kraków');
  await expect(page.getByRole('combobox', { name: 'Country' })).toHaveAttribute('required', '');
  await page.getByRole('combobox', { name: 'Country' }).selectOption({ label: 'Poland' });
  await page.getByRole('checkbox', { name: 'It meets online' }).check();
  await page.getByRole('textbox', { name: 'Topics' }).fill('hiking, , ridge walks, hiking');
  // Its languages: typed and picked from what matches, by Enter or by a
  // click, the field cleared for the next; each taken off by its button.
  const languages = page.getByRole('combobox', { name: 'Languages it uses' });
  await languages.fill('pol');
  await languages.press('Enter');
  await expect(languages).toHaveValue('');
  await languages.fill('eng');
  await page.getByRole('option', { name: 'English' }).click();
  await languages.fill('Germ');
  await languages.press('Enter');
  await expect(page.getByText('Choose up to five.')).toBeVisible();
  await page.getByRole('button', { name: 'Remove German' }).click();
  // Focus comes back to the field, but its list stays shut, off the form below.
  await expect(languages).toBeFocused();
  await expect(page.getByRole('listbox', { name: 'Languages it uses' })).toHaveCount(0);
  // Changed, it says so beside Save, and Save is the thing to press.
  const profile = page.locator('form[action="?/profile"]');
  const said = profile.getByRole('status');
  const save = profile.getByRole('button', { name: 'Save' });
  await expect(said).toHaveText('Unsaved changes');
  await expect(save).not.toHaveClass(/quiet/);
  await save.click();
  // Saved, it says that, until something is changed again.
  await expect(said).toHaveText('Saved');
  await expect(save).toHaveClass(/quiet/);
  // Drawn again as saved: each topic once, none blank.
  await expect(page.getByRole('textbox', { name: 'Topics' })).toHaveValue('hiking, ridge walks');
  await expect(page.getByRole('combobox', { name: 'Country' })).toHaveValue('PL');
  await expect(page.getByRole('list', { name: 'Chosen' }).getByRole('button')).toHaveText([
    /^Polish/,
    /^English/
  ]);
  // Its links, each added from a dialog: with what it is, or without.
  const addLink = async (address, title) => {
    await page.getByRole('button', { name: 'Add a link' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add a link' });
    await dialog.getByRole('textbox', { name: 'Its address' }).fill(address);
    if (title) await dialog.getByRole('textbox', { name: 'Title' }).fill(title);
    await dialog.getByRole('button', { name: 'Add', exact: true }).click();
    await expect(page.locator('.link-list', { hasText: address })).toBeVisible();
  };
  await addLink('https://chat.hikers.example', 'Our chat');
  await addLink('https://walks.example/');
  await addLink('https://gone.example/');
  await page.getByRole('button', { name: 'Remove gone.example' }).click();
  await expect(page.locator('.link-list', { hasText: 'gone.example' })).toHaveCount(0);
  // Each by its site's icon, kept as the group's own, where the site has one.
  await expect(
    page.locator('.link-list li', { hasText: 'Our chat' }).locator('img.link-icon')
  ).toHaveAttribute('src', /^\/img\/did:/);
  await expect(
    page.locator('.link-list li', { hasText: 'walks.example' }).locator('img')
  ).toHaveCount(0);
  await page.goto(url('/g/hikers.groups.test'));
  await expect(page.getByRole('heading', { name: 'Hikers of the Hills' })).toBeVisible();
  await expect(page.getByText('Walks on Sundays.')).toBeVisible();
  // Where it meets, its languages and what it is about, under its words.
  await expect(page.locator('.facts')).toHaveText(
    'Meets in Kraków, Poland, and online. Speaks Polish and English.'
  );
  await expect(page.locator('.group-card .tag')).toHaveText(['hiking', 'ridge walks']);
  // Its links, in a row: by title, or by address.
  const links = page.getByRole('navigation', { name: "Hikers of the Hills's links" });
  await expect(links.getByRole('link', { name: 'Our chat' })).toHaveAttribute(
    'href',
    'https://chat.hikers.example'
  );
  await expect(links.getByRole('link', { name: 'walks.example' })).toHaveAttribute(
    'href',
    'https://walks.example/'
  );
  const icon = links.getByRole('link', { name: 'Our chat' }).locator('img.link-icon');
  await expect(icon).toHaveJSProperty('naturalWidth', 16);
  await expect(
    links.getByRole('link', { name: 'walks.example' }).locator('svg.icon')
  ).toBeVisible();
});

// A one-pixel PNG.
const PIXEL = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64'
);

test('being asked before leaving changes unsaved', async () => {
  const settings = url('/g/hikers.groups.test/settings');
  await page.goto(settings);
  const name = page.getByRole('textbox', { name: 'Name', exact: true });
  const was = await name.inputValue();
  await name.fill('Not saved');
  // Going to another page asks first, and staying keeps what was typed.
  await page.getByRole('link', { name: 'Rules' }).click();
  const ask = page.getByRole('dialog', { name: 'Leave without saving?' });
  await ask.getByRole('button', { name: 'Keep editing' }).click();
  await expect(name).toHaveValue('Not saved');
  // Put back as it was, there is nothing to ask about.
  await name.fill(was);
  await expect(page.locator('form[action="?/profile"]').getByRole('status')).toHaveText('');
  await name.fill('Not saved');
  await page.getByRole('link', { name: 'Rules' }).click();
  await ask.getByRole('button', { name: 'Leave' }).click();
  await expect(page).toHaveURL(/\/settings\/rules$/);
  await page.goto(settings);
  await expect(name).toHaveValue(was);
});

test('giving a group a picture, and taking it away', async () => {
  await page.goto(url('/g/hikers.groups.test/settings'));
  await page.getByRole('button', { name: 'Add the avatar' }).click();
  await page
    .getByLabel('Avatar', { exact: true })
    .setInputFiles({ name: 'face.png', mimeType: 'image/png', buffer: PIXEL });
  const adding = page.getByRole('dialog', { name: 'Add the avatar' });
  await adding.getByRole('button', { name: 'Add the avatar' }).click();
  // Sent in place: done, the dialog closes and the page offers to remove it.
  await expect(adding).toBeHidden();
  await expect(page.getByRole('button', { name: 'Remove the avatar' })).toBeVisible();
  // Drawn on its page, from the host, as the image it is: one pixel wide.
  await page.goto(url('/g/hikers.groups.test'));
  const avatar = page.locator('.identity img.av');
  await expect(avatar).toBeVisible();
  await expect.poll(() => avatar.evaluate((img) => img.naturalWidth)).toBe(1);
  await page.goto(url('/g/hikers.groups.test/settings'));
  await page.getByRole('button', { name: 'Remove the avatar' }).click();
  await expect(page.getByRole('button', { name: 'Add the avatar' })).toBeVisible();
  await page.goto(url('/g/hikers.groups.test'));
  await expect(page.locator('.identity img.av')).toHaveCount(0);
});

test('publishing a site from a folder, and deleting it', async () => {
  // A folder as people keep a site: a front page, a page under it, a
  // page for addresses not found, a picture, and a file of their tools',
  // which is left out.
  const dir = mkdtempSync(join(tmpdir(), 'e2e-site-'));
  mkdirSync(join(dir, 'rides'));
  writeFileSync(join(dir, 'index.html'), '<h1>Hikers</h1>');
  writeFileSync(join(dir, 'rides/index.html'), '<h1>Walks</h1>');
  writeFileSync(join(dir, '404.html'), 'Lost');
  writeFileSync(join(dir, 'face.png'), PIXEL);
  writeFileSync(join(dir, '.DS_Store'), 'tools');
  await page.goto(url('/g/hikers.groups.test/settings'));
  await page
    .getByRole('navigation', { name: 'Settings' })
    .getByRole('link', { name: 'Site' })
    .click();
  await expect(page.getByText('Nothing published yet')).toBeVisible();
  await page.getByRole('button', { name: 'Publish a site' }).click();
  const publishing = page.getByRole('dialog', { name: 'Publish a site' });
  await publishing.getByLabel('Folder').setInputFiles(dir);
  await publishing.getByRole('button', { name: 'Publish' }).click();
  await expect(publishing).toBeHidden();
  await expect(page.getByText(/^4 files, published /)).toBeVisible();

  // Kept as the group's, each file at its path under the folder, typed by
  // its extension: a page sent as text, the picture as its bytes, whole.
  const { did } = await fetch(
    `${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=hikers.groups.test`
  ).then((r) => r.json());
  const { value } = await fetch(
    `${world.host}/xrpc/com.atproto.repo.getRecord?repo=${did}&collection=fyi.opensocial.site&rkey=self`
  ).then((r) => r.json());
  const files = Object.fromEntries(
    value.files.map((f) => [f.path, [f.blob.mimeType, f.blob.size]])
  );
  expect(files).toEqual({
    'index.html': ['text/html', 15],
    'rides/index.html': ['text/html', 14],
    '404.html': ['text/html', 4],
    'face.png': ['image/png', PIXEL.length]
  });
  // Its 404.html, found, is its page for addresses that aren't.
  expect(value.notFound).toBe('404.html');

  // Its page links to it, at its address.
  await page.goto(url('/g/hikers.groups.test'));
  const website = page
    .getByRole('navigation', { name: 'The group' })
    .getByRole('link', { name: 'Website' });
  await expect(website).toHaveAttribute('href', 'https://hikers.groups.test/');

  await page.goto(url('/g/hikers.groups.test/settings/site'));
  await page.getByRole('button', { name: 'Delete the site' }).click();
  await page
    .getByRole('dialog', { name: 'Delete this site?' })
    .getByRole('button', { name: 'Delete' })
    .click();
  await expect(page.getByText('Nothing published yet')).toBeVisible();
  await page.goto(url('/g/hikers.groups.test'));
  await expect(page.getByRole('link', { name: 'Website' })).toHaveCount(0);
});

test("a site's page reaches the console without the session of whoever opens it", async () => {
  // A page anyone with group.configure could publish: it asks the host as
  // whoever is looking, then posts a form signing them out. The site is
  // at a name beside the console's, as a deployment's group addresses
  // are beside its own name, so only its sandbox keeps the session back.
  const consoleOrigin = new URL(world.base).origin;
  const dir = mkdtempSync(join(tmpdir(), 'e2e-site-'));
  writeFileSync(
    join(dir, 'index.html'),
    `<form id="out" method="post" action="${consoleOrigin}/console/logout"></form>
<script>
  fetch('${consoleOrigin}/xrpc/fyi.opensocial.getViewer', { credentials: 'include' })
    .catch(() => {})
    .finally(() => document.getElementById('out').submit());
</script>`
  );
  await page.goto(url('/g/hikers.groups.test/settings/site'));
  await page.getByRole('button', { name: 'Publish a site' }).click();
  const publishing = page.getByRole('dialog', { name: 'Publish a site' });
  await publishing.getByLabel('Folder').setInputFiles(dir);
  await publishing.getByRole('button', { name: 'Publish' }).click();
  await expect(publishing).toBeHidden();

  // Each request the browser sends the console, with its cookies, until
  // the one signing out.
  const sent = [];
  let out = false;
  const watch = (req) => {
    const at = new URL(req.url());
    if (out || at.origin !== consoleOrigin) return;
    out = at.pathname === '/console/logout';
    sent.push(req.allHeaders().then((h) => ({ path: at.pathname, cookie: h.cookie ?? '' })));
  };
  const session = /group_host_session=/;
  page.on('request', watch);
  // From the console's own page, its session goes with it.
  await page.evaluate(() => fetch('/xrpc/fyi.opensocial.getViewer'));
  // From the site's, nothing of it does.
  await page.goto(world.hikersSite);
  await page.waitForURL((u) => u.origin === consoleOrigin);
  page.off('request', watch);
  const [own, ...fromSite] = await Promise.all(sent);
  expect(own.path).toBe('/xrpc/fyi.opensocial.getViewer');
  expect(own.cookie).toMatch(session);
  expect(fromSite.map((r) => r.path)).toEqual([
    '/xrpc/fyi.opensocial.getViewer',
    '/console/logout'
  ]);
  for (const r of fromSite) expect(r.cookie, r.path).not.toMatch(session);
  // So she is signed in still.
  await page.goto(url('/'));
  await expect.poll(() => signedInAs(page)).toBe('@alice.test');

  await page.goto(url('/g/hikers.groups.test/settings/site'));
  await page.getByRole('button', { name: 'Delete the site' }).click();
  await page
    .getByRole('dialog', { name: 'Delete this site?' })
    .getByRole('button', { name: 'Delete' })
    .click();
  await expect(page.getByText('Nothing published yet')).toBeVisible();
});

test('a site showing its members to whoever signs in on it, and writing for them', async () => {
  // A front page that lists the members to those who may see them, with
  // the kit's sign-in button, a page under it, and a button taking up the
  // visitor's place: their acceptance, written in their own repo in the
  // members space.
  const dir = mkdtempSync(join(tmpdir(), 'e2e-site-'));
  mkdirSync(join(dir, 'rides'));
  writeFileSync(
    join(dir, 'index.html'),
    `<!doctype html><title>Riders' rides</title>
<script src="/_os/kit.js"></script>
<os-sign-in></os-sign-in>
<a href="rides/">Rides</a>
<ul id="members"></ul>
<p id="place"></p>
<button id="accept" type="button">Take up your place</button>
<button id="upload" type="button">Upload a picture</button>
<p id="uploaded"></p>
<script>
  const list = document.getElementById('members');
  const place = document.getElementById('place');
  const line = (text) => Object.assign(document.createElement('li'), { textContent: text });
  const draw = async () => {
    const { did } = await os.group();
    try {
      const { members } = await os.call('fyi.opensocial.listMembers', { group: did });
      list.replaceChildren(...members.map((m) => line('@' + m.actor.handle)));
      const { viewer } = (await os.call('fyi.opensocial.getGroup', { group: did })).group;
      place.textContent = viewer?.accepted ? 'Your place is taken up.' : '';
      return viewer?.accepted;
    } catch (e) {
      list.replaceChildren(line(e.locked ? 'Members only.' : e.message));
    }
  };
  document.getElementById('accept').onclick = async () => {
    const { did } = await os.group();
    const members = os.space('at://' + did + '/space/fyi.opensocial.members/self');
    const value = { createdAt: new Date().toISOString() };
    try {
      await members.put('fyi.opensocial.acceptance', value, { rkey: 'self' });
    } catch (e) {
      place.textContent = 'Not written: ' + e.message;
      return;
    }
    // The host hears of it from her PDS a moment after.
    for (let i = 0; i < 40 && !(await draw()); i++) await new Promise((r) => setTimeout(r, 250));
  };
  // A one-pixel PNG, uploaded to her own PDS for a record to point to.
  document.getElementById('upload').onclick = async () => {
    const png = Uint8Array.from(
      atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=='),
      (c) => c.charCodeAt(0)
    );
    const out = document.getElementById('uploaded');
    try {
      const blob = await os.upload(new Blob([png], { type: 'image/png' }));
      out.textContent = 'Uploaded ' + blob.mimeType + ', ' + blob.size + ' bytes';
    } catch (e) {
      out.textContent = 'Not uploaded: ' + e.message;
    }
  };
  os.onChange(draw);
  draw();
</script>`
  );
  writeFileSync(
    join(dir, 'rides/index.html'),
    '<!doctype html><title>Rides</title><script src="/_os/kit.js"></script><h1>Saturday rides</h1>'
  );
  const writes = 'fyi.opensocial.members:fyi.opensocial.acceptance';
  await page.goto(url('/g/riders.groups.test/settings/site'));
  await page.getByRole('button', { name: 'Publish a site' }).click();
  const publishing = page.getByRole('dialog', { name: 'Publish a site' });
  await publishing.getByLabel('Folder').setInputFiles(dir);
  await publishing.getByText('Advanced').click();
  await publishing.getByLabel('Who the members are').check();
  await publishing.getByLabel("The group's profile and rules").check();
  await publishing.getByLabel('Kinds of record, one per line').fill(writes);
  await publishing.getByRole('button', { name: 'Publish' }).click();
  await expect(publishing).toBeHidden();
  // Published again, the dialog has it as the site does, Advanced open.
  await page.getByRole('button', { name: 'Publish a site' }).click();
  await expect(publishing.getByLabel('Who the members are')).toBeChecked();
  await expect(publishing.getByLabel("People's profiles")).not.toBeChecked();
  await expect(publishing.getByLabel('Kinds of record, one per line')).toHaveValue(writes);
  await publishing.getByRole('button', { name: 'Cancel' }).click();

  // Signed out, the page shows what it may, and the tab its title.
  await page.goto(world.ridersSite);
  const site = page.frameLocator('#os-page');
  await expect(site.getByText('Members only.')).toBeVisible();
  await expect(page).toHaveTitle("Riders' rides");

  // Signed in on the site with the console's own dialog, at her PDS.
  await site.getByRole('button', { name: 'Sign in' }).click();
  const signing = page.getByRole('dialog', { name: 'Sign in with your Atmosphere account' });
  await signing.getByLabel('Handle').fill('alice.test');
  await signing.getByRole('button', { name: 'Sign in' }).click();
  await approve(page, 'alice');
  await page.waitForURL((u) => u.origin === new URL(world.ridersSite).origin);
  await expect(site.getByRole('listitem').filter({ hasText: '@alice.test' })).toBeVisible();
  await expect(site.getByRole('button', { name: 'Sign out' })).toBeVisible();

  // She takes up her place from the site: written in her own repo in
  // the members space, at her PDS, as her sign-in there let it. A PDS
  // that does not speak the space alpha writes in no space.
  if (world.spaces) {
    await expect(site.getByText('Your place is taken up.')).toBeHidden();
    await site.getByRole('button', { name: 'Take up your place' }).click();
    await expect(site.getByText('Your place is taken up.')).toBeVisible();
  }

  // She uploads a picture to her own PDS, which a record the site writes
  // could point to, as her sign-in there let it.
  await site.getByRole('button', { name: 'Upload a picture' }).click();
  await expect(site.getByText('Uploaded image/png, 70 bytes')).toBeVisible();

  // Its page under it is in the address bar and the tab, so a reload
  // opens it, signed in still.
  await site.getByRole('link', { name: 'Rides' }).click();
  await expect(page).toHaveURL(`${world.ridersSite}/rides/`);
  await expect(page).toHaveTitle('Rides');
  await page.reload();
  await expect(site.getByRole('heading', { name: 'Saturday rides' })).toBeVisible();

  // Signed out, in place.
  await page.goto(world.ridersSite);
  await site.getByRole('button', { name: 'Sign out' }).click();
  await expect(site.getByText('Members only.')).toBeVisible();

  await page.goto(url('/g/riders.groups.test/settings/site'));
  await page.getByRole('button', { name: 'Delete the site' }).click();
  await page
    .getByRole('dialog', { name: 'Delete this site?' })
    .getByRole('button', { name: 'Delete' })
    .click();
  await expect(page.getByText('Nothing published yet')).toBeVisible();
});

test("hiding a group's profile, and showing it again", async () => {
  // Found on the network while its declaration is in its public repo,
  // which anyone may read.
  const listed = async () =>
    bob.evaluate(async () => {
      const { did } = await fetch(
        '/xrpc/com.atproto.identity.resolveHandle?handle=hikers.groups.test'
      ).then((r) => r.json());
      const res = await fetch(
        `/xrpc/com.atproto.repo.getRecord?repo=${did}&collection=fyi.opensocial.declaration&rkey=self`
      );
      return res.ok;
    });
  expect(await listed()).toBe(true);
  await page.goto(url('/g/hikers.groups.test/settings'));
  // Saved, and the page opened afresh as the host has it: not reloaded,
  // which would send the form's post again.
  const settings = url('/g/hikers.groups.test/settings');
  const save = async (label) => {
    await page.goto(settings);
    await page.getByLabel(label).check();
    const seen = page.locator('form[action="?/visibility"]');
    await seen.getByRole('button', { name: 'Save' }).click();
    await expect(seen.getByRole('status')).toHaveText('Saved');
    await page.goto(settings);
    await expect(page.getByLabel(label)).toBeChecked();
  };
  await save('Members only. Others see just its handle.');
  await expect.poll(listed).toBe(false);
  await save('Anyone, and it can be found');
  await expect.poll(listed).toBe(true);
});

test("asking for a handle on a domain that doesn't point to the group yet", async () => {
  await page.goto(url('/g/hikers.groups.test/settings'));
  const handles = page.locator('.handles');
  await expect(handles.getByText('hikers.groups.test', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Use your own domain' }).click();
  const using = page.getByRole('dialog', { name: 'Use your own domain' });
  // The record to add points to the group, at the domain as it is typed.
  const { did } = await fetch(
    `${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=hikers.groups.test`
  ).then((r) => r.json());
  await expect(using.getByText(`did=${did}`, { exact: true })).toBeVisible();
  await using.getByLabel('Domain').fill('Hikers.test');
  const name = using.getByText('_atproto.hikers.test', { exact: true });
  await expect(name).toBeVisible();
  // Refused in the reader's words, the dialog open with what was typed,
  // and the record's name with it.
  await using.getByRole('button', { name: 'Use', exact: true }).click();
  await expect(
    using.getByText("hikers.test doesn't point to this group yet.", { exact: false })
  ).toBeVisible();
  await expect(using.getByLabel('Domain')).toHaveValue('Hikers.test');
  await expect(name).toBeVisible();
});

test('adding a rule, and taking it away', async () => {
  // Settings has a page for each part, reached by its tabs.
  await page.goto(url('/g/hikers.groups.test/settings'));
  await page
    .getByRole('navigation', { name: 'Settings' })
    .getByRole('link', { name: 'Rules' })
    .click();
  await expect(page).toHaveURL(url('/g/hikers.groups.test/settings/rules'));
  await expect(page.getByText('No rules yet')).toBeVisible();
  await page.getByRole('button', { name: 'Add a rule' }).click();
  await page.getByRole('textbox', { name: 'Rule' }).fill('Leave no trace');
  await page
    .getByRole('dialog', { name: 'Add a rule' })
    .getByRole('button', { name: 'Add', exact: true })
    .click();
  await expect(page.locator('.rule-rows', { hasText: 'Leave no trace' })).toBeVisible();
  // On the group's page, folded under how many there are.
  await page.goto(url('/g/hikers.groups.test'));
  await page.locator('summary', { hasText: 'Rules' }).click();
  await expect(page.locator('.rules .count')).toHaveText('1');
  await expect(page.getByText('Leave no trace')).toBeVisible();
  await page.goto(url('/g/hikers.groups.test/settings/rules'));
  await page.getByRole('button', { name: 'Delete “Leave no trace”' }).click();
  await page
    .getByRole('dialog', { name: 'Delete this rule?' })
    .filter({ visible: true })
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page.locator('.rule-rows')).toHaveCount(0);
});

test('defining a role', async () => {
  await page.goto(url('/g/hikers.groups.test/settings/roles'));
  await page.getByRole('button', { name: 'Create a role' }).click();
  await page.getByRole('textbox', { name: 'Its key' }).fill('leader');
  await page.getByRole('textbox', { name: 'Its name' }).fill('Ride leader');
  await page.getByRole('checkbox', { name: 'invite' }).check();
  await page
    .getByRole('dialog', { name: 'Create a role' })
    .getByRole('button', { name: 'Create', exact: true })
    .click();
  await expect(page.locator('.roles li', { hasText: 'Ride leader' })).toContainText('invite');
  await page.getByRole('button', { name: 'Delete Ride leader' }).click();
  await page
    .getByRole('dialog', { name: 'Delete this role?' })
    .filter({ visible: true })
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page.locator('.roles li', { hasText: 'Ride leader' })).toHaveCount(0);
});

test('creating an app password, and revoking what it signed in', async () => {
  await page.goto(url('/g/hikers.groups.test/settings/apps'));
  await page.getByRole('button', { name: 'Create an app password' }).click();
  await page.getByRole('textbox', { name: 'Where it is used' }).fill('Phone');
  await page
    .getByRole('dialog', { name: 'Create an app password' })
    .getByRole('button', { name: 'Create', exact: true })
    .click();
  const password = (await page.locator('code.password').textContent()).trim();
  const signIn = () =>
    fetch(`${world.host}/xrpc/com.atproto.server.createSession`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ identifier: 'hikers.groups.test', password })
    });
  expect((await signIn()).status).toBe(200);
  await page.goto(url('/g/hikers.groups.test/settings/apps'));
  await expect(page.locator('.passwords', { hasText: 'Phone' })).toBeVisible();
  // Whose it is, and who did what lately, by handle; what was done said
  // as what it is, not as the records' addresses.
  await expect(page.locator('.passwords li', { hasText: 'Phone' })).toContainText('alice.test');
  await expect(
    page.locator('.activity li', { hasText: '@alice.test deleted a role' }).first()
  ).toBeVisible();
  await expect(page.locator('.activity')).not.toContainText('at://');
  await expect(page.locator('.activity')).not.toContainText('did:');
  // Its sessions a page at a time: shown all, to find it.
  while (await page.getByRole('button', { name: 'Show more' }).count())
    await page.getByRole('button', { name: 'Show more' }).click();
  await page.getByRole('button', { name: "Revoke app-password:Phone's access" }).click();
  await page
    .getByRole('dialog', { name: "Revoke this app's access?" })
    .filter({ visible: true })
    .getByRole('button', { name: 'Revoke', exact: true })
    .click();
  // The console's own token, from inviting, stays.
  await expect(page.locator('.sessions', { hasText: 'With an app password' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Revoke “Phone”' }).click();
  await page
    .getByRole('dialog', { name: 'Revoke this app password?' })
    .filter({ visible: true })
    .getByRole('button', { name: 'Revoke', exact: true })
    .click();
  await expect(page.locator('.passwords')).toHaveCount(0);
  expect((await signIn()).status).toBe(401);
});

test('adding a recovery key, and removing it', async () => {
  const key = didKey();
  await page.goto(url('/g/hikers.groups.test/settings/recovery'));
  await expect(page.locator('.recovery-keys li')).toHaveCount(1);
  await page.getByRole('button', { name: 'Add a recovery key' }).click();
  await page.getByRole('textbox', { name: 'A key of yours, as a did:key' }).fill(key);
  await page
    .getByRole('dialog', { name: 'Add a recovery key' })
    .getByRole('button', { name: 'Add', exact: true })
    .click();
  const keys = page.locator('.recovery-keys li');
  await expect(keys).toHaveCount(2);
  await expect(keys.first()).toContainText(key);
  await expect(keys.last()).toContainText('This host');
  await keys.first().getByRole('button', { name: 'Remove' }).click();
  await page
    .getByRole('dialog', { name: 'Remove this key?' })
    .filter({ visible: true })
    .getByRole('button', { name: 'Remove', exact: true })
    .click();
  await expect(page.locator('.recovery-keys li')).toHaveCount(1);
});

test('moderating: a label, and the queue', async () => {
  // A label of the group's own, defined in its settings.
  await page.goto(url('/g/hikers.groups.test/settings/labels'));
  await expect(page.getByText('No labels yet')).toBeVisible();
  await page.getByRole('button', { name: 'Create a label' }).click();
  const defining = page.getByRole('dialog', { name: 'Create a label' });
  await defining.getByRole('textbox', { name: 'Name' }).fill('Off-topic');
  await defining.getByRole('textbox', { name: 'Value' }).fill('off-topic');
  await defining.getByRole('textbox', { name: 'What it means' }).fill('Not about hiking.');
  await defining.getByRole('combobox', { name: 'How apps show it' }).selectOption('hide');
  await defining.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page.locator('.label-rows')).toContainText('Not about hiking.');
  await expect(page.locator('.label-rows')).toContainText('Apps hide it until someone chooses');
  await page.goto(url('/g/hikers.groups.test'));
  await page.getByRole('link', { name: 'Moderation' }).click();
  // A subject's row, by its handle, not those whose history names it.
  const row = (handle) =>
    page
      .locator('li.subject')
      .filter({ has: page.locator(':scope > .row-main .who', { hasText: handle }) });
  const review = page.getByRole('dialog', { name: 'Review' }).filter({ visible: true });
  // Putting him out is already in the queue, on him.
  await expect(row(who.bob.handle).locator(':scope > .history .events')).toContainText('Removed');
  // Found by his handle, he is reviewed where his row is.
  await page.getByRole('combobox', { name: 'Find an account' }).fill(who.bob.handle);
  await page.getByRole('search').getByRole('button', { name: 'Review' }).click();
  // The group's own label is offered by its name.
  await expect(review.getByRole('button', { name: 'Add the label off-topic' })).toHaveText(
    'Off-topic'
  );
  await review.getByRole('textbox', { name: 'Add a label' }).fill('troll');
  await review.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(review.locator('.labels')).toContainText('troll');
  // Taken off, it is a label the group has used, to put back with a press.
  await review.getByRole('button', { name: 'Remove the label troll' }).click();
  await expect(review.locator('.labels')).toHaveCount(0);
  await review.getByRole('button', { name: 'Add the label troll' }).click();
  await expect(review.locator('.labels')).toContainText('troll');
  // Taking him down asks first.
  await review.getByRole('button', { name: 'Take down' }).click();
  await page
    .getByRole('dialog', { name: 'Take this account down?' })
    .getByRole('button', { name: 'Take down' })
    .click();
  await expect(review.locator('.labels')).toContainText('!takedown');
  // Escalated with a note, it leaves the open queue for the escalated.
  await review.getByRole('textbox', { name: 'Note' }).fill('Asked him to stop');
  await review.getByRole('button', { name: 'Escalate' }).click();
  await expect(row(who.bob.handle)).toHaveCount(0);
  await page
    .getByRole('navigation', { name: 'The queue' })
    .getByRole('link', { name: 'Escalated' })
    .click();
  await expect(row(who.bob.handle).locator(':scope > .history .events')).toContainText(
    /Escalated by[\s\S]*Asked him to stop/
  );
  await row(who.bob.handle).getByRole('button', { name: 'Review' }).click();
  await review.getByRole('button', { name: 'Resolve' }).click();
  await expect(review).toHaveCount(0);
  await page.goto(url('/g/hikers.groups.test/moderation?status=resolved'));
  await expect(row(who.bob.handle).locator(':scope > .history .events')).toContainText('Resolved');
  // Someone nobody reported is reported from their review, into the open queue.
  await page.goto(url('/g/hikers.groups.test/moderation'));
  await page.getByRole('combobox', { name: 'Find an account' }).fill(who.dan.handle);
  await page.getByRole('search').getByRole('button', { name: 'Review' }).click();
  await expect(review).toContainText('Not in the queue');
  await review.getByRole('button', { name: 'Report this account' }).click();
  const reporting = page.getByRole('dialog', { name: 'Report an account' });
  await reporting.getByRole('radio', { name: 'Rude' }).check();
  await reporting.getByRole('textbox', { name: 'What happened' }).fill('Shouted at a newcomer.');
  await reporting.getByRole('button', { name: 'Report', exact: true }).click();
  await expect(row(who.dan.handle)).toContainText('Shouted at a newcomer.');
  // The group's own label, no longer defined, is no longer offered.
  await page.goto(url('/g/hikers.groups.test/settings/labels'));
  await page.getByRole('button', { name: 'Delete the label off-topic' }).click();
  await page
    .getByRole('dialog', { name: 'Delete this label?' })
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page.getByText('No labels yet')).toBeVisible();
});

test('signing an app in as her group', async () => {
  const app = await new App('atproto repo:app.bsky.feed.post').listen();
  await page.goto(await app.push());
  await expect(page.getByRole('heading', { name: 'Sign in an app' })).toBeVisible();
  // Said as her PDS says it: Bluesky, and in its details, the posts.
  const riders = page.locator('form', { hasText: '@riders.groups.test' });
  await expect(riders.getByText("Use parts of the group's Bluesky account")).toBeVisible();
  await riders.getByRole('button', { name: 'Details' }).click();
  const details = page.getByRole('dialog', { name: 'Bluesky' }).filter({ visible: true });
  await expect(details.getByRole('row', { name: /app\.bsky\.feed\.post/ })).toBeVisible();
  await details.getByRole('button', { name: 'Close' }).click();
  await page.getByRole('button', { name: 'Technical details' }).click();
  await expect(page.getByRole('dialog', { name: 'Technical details' })).toContainText(
    'repo:app.bsky.feed.post'
  );
  await page
    .getByRole('dialog', { name: 'Technical details' })
    .getByRole('button', { name: 'Close' })
    .click();
  await page.getByRole('button', { name: 'Sign it in as Riders' }).click();
  const back = await app.back;
  app.close();
  expect(new URL(back).searchParams.get('state')).toBe('st4te');
  const tokens = await app.exchange(back);
  expect(tokens.sub).toBe(world.riders);
  expect(tokens.scope).toBe('atproto repo:app.bsky.feed.post');

  // The app writes as the group, and it is there.
  const record = {
    $type: 'app.bsky.feed.post',
    text: 'Ride on Saturday',
    createdAt: new Date().toISOString()
  };
  const made = await app.call('com.atproto.repo.createRecord', tokens.access_token, {
    repo: world.riders,
    collection: 'app.bsky.feed.post',
    record
  });
  expect(made.status, JSON.stringify(made.body)).toBe(200);
  const rkey = made.body.uri.split('/').pop();
  const read = await fetch(
    `${world.host}/xrpc/com.atproto.repo.getRecord?repo=${world.riders}&collection=app.bsky.feed.post&rkey=${rkey}`
  );
  expect((await read.json()).value.text).toBe('Ride on Saturday');
});

test('connecting an AI app, and disconnecting it', async () => {
  const client = await new McpClient().listen();
  await page.goto(await client.authorize('Pocket helper'));
  await expect(page.getByText('wants to act as you in one of your groups')).toBeVisible();
  await expect(page.getByText("The app chose this name, and it isn't checked")).toBeVisible();
  await page.getByRole('button', { name: 'Connect it to Riders' }).click();
  const back = await client.back;
  client.close();
  expect(new URL(back).searchParams.get('state')).toBe('st4te');
  const tokens = await client.exchange(back);

  // It acts as her, in Riders, without saying which.
  const got = await client.tool(tokens.access_token, 'group_show', {});
  expect(got.status).toBe(200);
  expect(got.result.structuredContent.group.did).toBe(world.riders);

  await page.goto(url('/g/riders.groups.test'));
  await expect(page.locator('.ai-apps', { hasText: 'Pocket helper' })).toBeVisible();
  await page
    .locator('.ai-apps', { hasText: 'Pocket helper' })
    .getByRole('button', { name: 'Disconnect' })
    .click();
  await page
    .getByRole('dialog', { name: 'Disconnect this app?' })
    .filter({ visible: true })
    .getByRole('button', { name: 'Disconnect', exact: true })
    .click();
  await expect(page.getByText('No AI app connected yet')).toBeVisible();
  expect((await client.tool(tokens.access_token, 'group_show', {})).status).toBe(401);
});

test('declining an app', async () => {
  const app = await new App('atproto').listen();
  await page.goto(await app.push());
  await page.getByRole('button', { name: 'Decline' }).click();
  const back = await app.back;
  app.close();
  expect(new URL(back).searchParams.get('error')).toBe('access_denied');
});

test('coming to a request that has lapsed', async () => {
  await page.goto(
    url('/authorize?request_uri=urn%3Aietf%3Aparams%3Aoauth%3Arequest_uri%3Areq-gone')
  );
  await expect(page.getByText('This sign-in request has expired')).toBeVisible();
});

test('switching a group off, back on, and deleting it', async () => {
  await page.goto(url('/new'));
  await page.getByRole('textbox', { name: 'Name' }).pressSequentially('Tandem');
  await page.getByRole('button', { name: 'Start the group' }).click();
  await page.waitForURL(url('/g/tandem.groups.test'));
  // Off from its settings, asked to be sure of; its page then says so.
  await page.goto(url('/g/tandem.groups.test/settings'));
  await page.getByRole('button', { name: 'Switch off' }).click();
  await page
    .getByRole('dialog', { name: 'Switch this group off?' })
    .getByRole('button', { name: 'Switch off' })
    .click();
  await page.waitForURL(url('/g/tandem.groups.test'));
  await expect(page.getByText('This group is switched off')).toBeVisible();
  // Bob, who never ran it, sees only that.
  await bob.goto(url('/g/tandem.groups.test'));
  await expect(bob.getByText('This group is switched off')).toBeVisible();
  await expect(bob.getByRole('button', { name: 'Switch back on' })).toHaveCount(0);
  // Back on, from its page.
  await page.getByRole('button', { name: 'Switch back on' }).click();
  await expect(page.getByText(/You're a member/)).toBeVisible();
  // Deleted, it is gone, and so is its page.
  await page.goto(url('/g/tandem.groups.test/settings'));
  await page.getByRole('button', { name: 'Delete the group' }).click();
  await page
    .getByRole('dialog', { name: 'Delete this group?' })
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await page.waitForURL(url('/'));
  const gone = await page.goto(url('/g/tandem.groups.test'));
  expect(gone.status()).toBe(404);
});

test('moving an account here to become a group', async () => {
  await page.goto(url('/new'));
  await page.getByRole('link', { name: 'move an account you already have' }).click();
  const moving = page.getByRole('combobox', { name: 'The account to move' });
  await moving.fill(who.harbor.handle);
  await moving.press('Enter');
  // At its own server, as itself; then back, still alice.
  await approve(page, 'harbor');
  await page.waitForURL(url('/move'));
  await expect.poll(() => signedInAs(page)).toBe('@alice.test');
  await expect(page.getByText('Here it will be')).toContainText('@harbor.groups.test');
  await page.getByRole('button', { name: 'Copy it here' }).click();
  const code = page.getByRole('textbox', { name: 'The code' });
  await expect(code).toBeVisible();
  // The code its server emailed, from its inbox.
  const mail = await fetch(`${world.mail}?to=harbor@test.com`).then((r) => r.json());
  await code.fill(mail.code);
  await page.getByRole('button', { name: 'Finish moving it' }).click();
  await page.waitForURL(url('/g/harbor.groups.test'));
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
});

test('signing out', async () => {
  await page.goto(url('/'));
  // From her menu, under her handle: what is hers, and signing out last.
  await page.getByRole('banner').getByText('@alice.test', { exact: true }).click();
  const menu = page.locator('.user-menu .menu');
  await expect(menu.getByRole('link')).toHaveText([
    'Invites',
    'Your groups',
    'Start a group',
    'Move an account here'
  ]);
  await menu.getByRole('button', { name: 'Sign out' }).click();
  await expect(page.getByRole('banner').getByRole('button', { name: 'Sign in' })).toBeVisible();
  await expect.poll(() => signedInAs(page)).toBe('');
});
