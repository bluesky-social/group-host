// What people do on the console, in order, as alice: sign in at her own
// PDS, see her groups, sign an app in as one of them, and decline one;
// and what the app then does as the group, read back from the host.

import { expect, test } from '@playwright/test'
import { App, Assistant, approve, didKey, signIn, signedInAs, url, who, world } from './pages.mjs'

test.describe.configure({ mode: 'serial' })

let page
// Bob's browser, apart from alice's.
let bob

test.beforeAll(async ({ browser }) => {
  page = await browser.newPage()
  bob = await (await browser.newContext()).newPage()
})

test.afterAll(async () => {
  await page.close()
  await bob.context().close()
})

test('signing in at her own PDS', async () => {
  await page.goto(url('/'))
  await expect(page.getByRole('heading', { name: 'Groups on this host' })).toBeVisible()
  await signIn(page, 'alice')
  await expect.poll(() => signedInAs(page)).toBe('@alice.test')
})

test('seeing her groups', async () => {
  await expect(page.getByRole('heading', { name: 'Your groups' })).toBeVisible()
  await expect(page.locator('.group-row', { hasText: 'Riders' }).first()).toBeVisible()
})

test('starting a group', async () => {
  await page.getByRole('link', { name: 'Start a group' }).click()
  await page.getByRole('textbox', { name: 'Handle' }).fill('hikers.groups.test')
  await page.getByRole('textbox', { name: 'Name' }).fill('Hikers')
  await page.getByRole('textbox', { name: 'What it is for' }).fill('Walks on Sundays.')
  await page.getByRole('button', { name: 'Start the group' }).click()
  await page.waitForURL(url('/g/hikers.groups.test'))
  await expect(page.getByRole('heading', { name: 'Hikers' })).toBeVisible()
  await expect(page.getByText('Walks on Sundays.')).toBeVisible()
  await expect(page.getByText('You are a member.')).toBeVisible()
  // A handle taken already is the form's to say.
  await page.goto(url('/new'))
  await page.getByRole('textbox', { name: 'Handle' }).fill('hikers.groups.test')
  await page.getByRole('textbox', { name: 'Name' }).fill('Hikers again')
  await page.getByRole('button', { name: 'Start the group' }).click()
  await expect(page.locator('.danger')).toBeVisible()
})

test('asking to join, and being admitted', async () => {
  await bob.goto(url('/g/hikers.groups.test'))
  await signIn(bob, 'bob')
  await bob.getByRole('button', { name: 'Ask to join' }).click()
  await expect(bob.getByText('You have asked to join.')).toBeVisible()
  await page.goto(url('/g/hikers.groups.test'))
  const asking = page.locator('.requests li', { hasText: '@bob.test' })
  await asking.getByRole('button', { name: 'Admit' }).click()
  await expect(page.locator('.members', { hasText: '@bob.test' })).toBeVisible()
  await bob.reload()
  await expect(bob.getByText('You are a member.')).toBeVisible()
})

test('leaving a group', async () => {
  await bob.getByRole('button', { name: 'Leave' }).click()
  await expect(bob.getByRole('button', { name: 'Ask to join' })).toBeVisible()
})

test('inviting someone by their handle', async () => {
  await page.goto(url('/g/hikers.groups.test'))
  await page.getByRole('textbox', { name: 'Their handle' }).fill('nobody.test')
  await page.getByRole('button', { name: 'Invite' }).click()
  await expect(page.getByText('We cannot find nobody.test.')).toBeVisible()
  await page.getByRole('textbox', { name: 'Their handle' }).fill('@bob.test')
  await page.getByRole('button', { name: 'Invite' }).click()
  await expect(page.locator('.invites', { hasText: who.bob.did })).toBeVisible()
})

test('taking up an invitation', async () => {
  await bob.goto(url('/'))
  await expect(bob.getByRole('heading', { name: 'You are invited' })).toBeVisible()
  await bob.getByRole('button', { name: 'Join Hikers' }).click()
  await expect(bob.getByRole('heading', { name: 'You are invited' })).toHaveCount(0)
  await expect(bob.locator('.group-row', { hasText: 'Hikers' }).first()).toBeVisible()
  await page.reload()
  await expect(page.locator('.members', { hasText: '@bob.test' })).toBeVisible()
})

test('giving roles, and putting someone out', async () => {
  const row = page.locator('.member', { hasText: '@bob.test' })
  await expect(row.getByRole('checkbox', { name: 'member' })).toBeChecked()
  await row.getByRole('button', { name: "Save @bob.test's roles" }).click()
  await expect(page.locator('.member', { hasText: '@bob.test' })).toContainText('member')
  await page.locator('.member', { hasText: '@bob.test' }).getByRole('button', { name: 'Put @bob.test out' }).click()
  await expect(page.locator('.members', { hasText: '@bob.test' })).toHaveCount(0)
})

test('being told why the host refused', async () => {
  // The group's only admin may not leave it: the page says so.
  await page.getByRole('button', { name: 'Leave' }).click()
  await expect(page.locator('.danger')).toBeVisible()
  await expect(page.getByText('You are a member.')).toBeVisible()
})

test("changing a group's profile", async () => {
  await page.goto(url('/g/hikers.groups.test'))
  await page.getByRole('link', { name: 'Settings' }).click()
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Hikers of the Hills')
  await page.getByRole('button', { name: 'Save the profile' }).click()
  await page.goto(url('/g/hikers.groups.test'))
  await expect(page.getByRole('heading', { name: 'Hikers of the Hills' })).toBeVisible()
  await expect(page.getByText('Walks on Sundays.')).toBeVisible()
})

test('adding a rule, and taking it away', async () => {
  await page.goto(url('/g/hikers.groups.test/settings'))
  await page.getByRole('textbox', { name: 'Rule' }).fill('Leave no trace')
  await page.getByRole('button', { name: 'Add the rule' }).click()
  await expect(page.locator('.rules', { hasText: 'Leave no trace' })).toBeVisible()
  await page.goto(url('/g/hikers.groups.test'))
  await expect(page.getByText('Leave no trace')).toBeVisible()
  await page.goto(url('/g/hikers.groups.test/settings'))
  await page.getByRole('button', { name: 'Take away “Leave no trace”' }).click()
  await expect(page.locator('.rules')).toHaveCount(0)
})

test('defining a role', async () => {
  await page.goto(url('/g/hikers.groups.test/settings'))
  await page.getByRole('textbox', { name: 'Its key' }).fill('leader')
  await page.getByRole('textbox', { name: 'Its name' }).fill('Ride leader')
  await page.getByRole('checkbox', { name: 'invite' }).check()
  await page.getByRole('button', { name: 'Define the role' }).click()
  await expect(page.locator('.roles li', { hasText: 'Ride leader' })).toContainText('invite')
  await page.getByRole('button', { name: 'Take away Ride leader' }).click()
  await expect(page.locator('.roles li', { hasText: 'Ride leader' })).toHaveCount(0)
})

test('making an app password, and ending what it signed in', async () => {
  await page.goto(url('/g/hikers.groups.test/settings'))
  await page.getByRole('textbox', { name: 'Where it is used' }).fill('Phone')
  await page.getByRole('button', { name: 'Make an app password' }).click()
  const password = (await page.locator('code.password').textContent()).trim()
  const signIn = () =>
    fetch(`${world.host}/xrpc/com.atproto.server.createSession`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ identifier: 'hikers.groups.test', password }),
    })
  expect((await signIn()).status).toBe(200)
  await page.goto(url('/g/hikers.groups.test/settings'))
  await expect(page.locator('.passwords', { hasText: 'Phone' })).toBeVisible()
  await page.getByRole('button', { name: "End app-password:Phone's access" }).click()
  await expect(page.locator('.sessions')).toHaveCount(0)
  await page.getByRole('button', { name: 'Revoke “Phone”' }).click()
  await expect(page.locator('.passwords')).toHaveCount(0)
  expect((await signIn()).status).toBe(401)
})

test('adding a recovery key, and taking it off', async () => {
  const key = didKey()
  await page.goto(url('/g/hikers.groups.test/settings'))
  await expect(page.locator('.recovery-keys li')).toHaveCount(1)
  await page.getByRole('textbox', { name: 'A key of yours, as a did:key' }).fill(key)
  await page.getByRole('button', { name: 'Add the recovery key' }).click()
  const keys = page.locator('.recovery-keys li')
  await expect(keys).toHaveCount(2)
  await expect(keys.first()).toContainText(key)
  await expect(keys.last()).toContainText('this host')
  await keys.first().getByRole('button', { name: 'Take off' }).click()
  await expect(page.locator('.recovery-keys li')).toHaveCount(1)
})

test('moderating: a label, and the queue', async () => {
  await page.goto(url('/g/hikers.groups.test'))
  await page.getByRole('link', { name: 'Moderation' }).click()
  await expect(page.getByText('Nothing to look at')).toBeVisible()
  await page.getByRole('textbox', { name: 'Their handle' }).fill('bob.test')
  await page.getByRole('textbox', { name: 'Label' }).fill('troll')
  await page.getByRole('button', { name: 'Label them' }).click()
  const subject = page.locator('.subject', { hasText: who.bob.did })
  await expect(subject.locator('.status')).toHaveText('open')
  await expect(subject.locator('.events')).toContainText('label: troll')
  await subject.getByRole('button', { name: 'Escalate' }).click()
  await expect(page.locator('.subject', { hasText: who.bob.did }).locator('.status')).toHaveText('escalated')
  await page.locator('.subject', { hasText: who.bob.did }).getByRole('textbox', { name: 'Note' }).fill('Asked him to stop')
  await page.locator('.subject', { hasText: who.bob.did }).getByRole('button', { name: 'Add the note' }).click()
  await expect(page.locator('.subject', { hasText: who.bob.did }).locator('.events')).toContainText('note: Asked him to stop')
  await page.locator('.subject', { hasText: who.bob.did }).getByRole('button', { name: 'Resolve' }).click()
  await page.goto(url('/g/hikers.groups.test/moderation?status=resolved'))
  await expect(page.locator('.subject', { hasText: who.bob.did }).locator('.status')).toHaveText('resolved')
})

test('signing an app in as her group', async () => {
  const app = await new App('atproto repo:app.bsky.feed.post').listen()
  await page.goto(await app.push())
  await expect(page.getByRole('heading', { name: 'Sign in an app' })).toBeVisible()
  await expect(page.getByText('repo:app.bsky.feed.post').first()).toBeVisible()
  await page.getByRole('button', { name: 'Sign it in as Riders' }).click()
  const back = await app.back
  app.close()
  expect(new URL(back).searchParams.get('state')).toBe('st4te')
  const tokens = await app.exchange(back)
  expect(tokens.sub).toBe(world.riders)
  expect(tokens.scope).toBe('atproto repo:app.bsky.feed.post')

  // The app writes as the group, and it is there.
  const record = { $type: 'app.bsky.feed.post', text: 'Ride on Saturday', createdAt: new Date().toISOString() }
  const made = await app.call('com.atproto.repo.createRecord', tokens.access_token, {
    repo: world.riders,
    collection: 'app.bsky.feed.post',
    record,
  })
  expect(made.status, JSON.stringify(made.body)).toBe(200)
  const rkey = made.body.uri.split('/').pop()
  const read = await fetch(`${world.host}/xrpc/com.atproto.repo.getRecord?repo=${world.riders}&collection=app.bsky.feed.post&rkey=${rkey}`)
  expect((await read.json()).value.text).toBe('Ride on Saturday')
})

test('connecting an assistant, and disconnecting it', async () => {
  const assistant = await new Assistant().listen()
  await page.goto(await assistant.authorize('Pocket assistant'))
  await expect(page.getByText('wants to act as you in one of your groups')).toBeVisible()
  await expect(page.getByText('That name is its own word for itself')).toBeVisible()
  await page.getByRole('button', { name: 'Connect it to Riders' }).click()
  const back = await assistant.back
  assistant.close()
  expect(new URL(back).searchParams.get('state')).toBe('st4te')
  const tokens = await assistant.exchange(back)

  // It acts as her, in Riders, without saying which.
  const got = await assistant.tool(tokens.access_token, 'getGroup', {})
  expect(got.status).toBe(200)
  expect(got.result.structuredContent.group.did).toBe(world.riders)

  await page.goto(url('/g/riders.groups.test'))
  await expect(page.locator('.assistants', { hasText: 'Pocket assistant' })).toBeVisible()
  await page.getByRole('button', { name: 'Disconnect Pocket assistant' }).click()
  await expect(page.getByText('No assistants connected')).toBeVisible()
  expect((await assistant.tool(tokens.access_token, 'getGroup', {})).status).toBe(401)
})

test('declining an app', async () => {
  const app = await new App('atproto').listen()
  await page.goto(await app.push())
  await page.getByRole('button', { name: 'Decline' }).click()
  const back = await app.back
  app.close()
  expect(new URL(back).searchParams.get('error')).toBe('access_denied')
})

test('coming to a request that has lapsed', async () => {
  await page.goto(url('/authorize?request_uri=urn%3Aietf%3Aparams%3Aoauth%3Arequest_uri%3Areq-gone'))
  await expect(page.getByText('This sign-in request has expired')).toBeVisible()
})

test('moving an account here to become a group', async () => {
  await page.goto(url('/new'))
  await page.getByRole('link', { name: 'move an account you already have' }).click()
  await page.getByRole('textbox', { name: 'The account to move' }).fill(who.harbor.handle)
  await page.getByRole('button', { name: 'Sign in to it' }).click()
  // At its own server, as itself; then back, still alice.
  await approve(page, 'harbor')
  await page.waitForURL(url('/move'))
  await expect.poll(() => signedInAs(page)).toBe('@alice.test')
  await expect(page.getByText('Here it will be')).toContainText('@harbor.groups.test')
  await page.getByRole('button', { name: 'Copy it here' }).click()
  const code = page.getByRole('textbox', { name: 'The code' })
  await expect(code).toBeVisible()
  // The code its server emailed, from its inbox.
  const mail = await fetch(`${world.mail}?to=harbor@test.com`).then((r) => r.json())
  await code.fill(mail.code)
  await page.getByRole('button', { name: 'Finish moving it' }).click()
  await page.waitForURL(url('/g/harbor.groups.test'))
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible()
})

test('signing out', async () => {
  await page.goto(url('/'))
  await page.getByRole('button', { name: 'Sign out' }).click()
  await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible()
  await expect.poll(() => signedInAs(page)).toBe('')
})
