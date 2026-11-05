// What people do on the console, in order, as alice: sign in at her own
// PDS, see her groups, sign an app in as one of them, and decline one;
// and what the app then does as the group, read back from the host.

import { expect, test } from '@playwright/test'
import { App, Assistant, approve, didKey, joins, signIn, signInDialog, signedInAs, url, who, world } from './pages.mjs'

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

test('coming to the host signed out', async () => {
  await page.goto(url('/'))
  // The host by its address, what signing in is for, and no groups.
  const host = new URL(world.base).hostname
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(`Groups on ${host}`)
  await expect(page.getByText('Sign in with your Atmosphere account to see your groups, or to start one.')).toBeVisible()
  await expect(page.locator('.group-row')).toHaveCount(0)
  await expect(page.getByRole('contentinfo')).toHaveText(`${host} runs opensocial`)
  // Its one button opens the dialog: the apps of the Atmosphere, what an
  // account is, the handle, and where to make one.
  await page.getByRole('main').getByRole('button', { name: 'Sign in' }).click()
  const dialog = signInDialog(page)
  await expect(dialog).toBeVisible()
  await expect(dialog.locator('.apps img')).toHaveCount(40)
  await expect(dialog.getByRole('link', { name: 'What is an Atmosphere account?' })).toHaveAttribute('href', 'https://atmosphereaccount.com/')
  await expect(dialog.getByRole('textbox', { name: 'Handle' })).toBeFocused()
  await expect(dialog.getByRole('link', { name: 'Create one' })).toHaveAttribute('href', 'https://bsky.app/')
  await dialog.getByRole('button', { name: 'Cancel' }).click()
  await expect(dialog).toBeHidden()
})

test("a group's page signed out", async () => {
  // Its handle alone, and the way in, wherever its access record lets
  // anyone signed in read it.
  await page.goto(url('/g/riders.groups.test'))
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('riders.groups.test')
  await expect(page.getByText('Sign in to see this group.')).toBeVisible()
  await expect(page.locator('.identity, .rules, .standing')).toHaveCount(0)
  // Nor does the host tell anyone not signed in more of it.
  const { group } = await fetch(`${world.host}/xrpc/fyi.opensocial.getGroup?group=${world.riders}`).then((r) => r.json())
  expect(group).toEqual({ did: world.riders, handle: 'riders.groups.test' })
})

test("signing in at her own PDS, from a group's page", async () => {
  // Signed in from the page, back on it, as she may see it.
  await signIn(page, 'alice', page.getByRole('main').getByRole('button', { name: 'Sign in' }))
  await expect(page).toHaveURL(url('/g/riders.groups.test'))
  await expect.poll(() => signedInAs(page)).toBe('@alice.test')
  await expect(page.getByText('Sign in to see this group.')).toHaveCount(0)
  await expect(page.getByText("You're a member, as")).toBeVisible()
  await page.goto(url('/'))
})

test('seeing her groups', async () => {
  await expect(page.getByRole('heading', { name: 'Your groups' })).toBeVisible()
  await expect(page.locator('.group-row', { hasText: 'Riders' }).first()).toBeVisible()
})

test('coming back to her groups by a link, the page drawn in the browser', async () => {
  await page.goto(url('/new'))
  await page.getByRole('link', { name: 'opensocial' }).click()
  await page.waitForURL(url('/'))
  await expect(page.getByRole('heading', { name: 'Your groups' })).toBeVisible()
  await expect(page.locator('.group-row', { hasText: 'Riders' }).first()).toBeVisible()
})

test('starting a group', async () => {
  await page.getByRole('link', { name: 'Start a group' }).click()
  // Its address made from its name, under the host's domain.
  await page.getByRole('textbox', { name: 'Name' }).pressSequentially('Hikers')
  await expect(page.getByRole('textbox', { name: 'Its address' })).toHaveValue('hikers')
  await expect(page.getByText('.groups.test', { exact: true })).toBeVisible()
  await page.getByRole('textbox', { name: 'What it is for' }).fill('Walks on Sundays.')
  await page.getByRole('button', { name: 'Start the group' }).click()
  await page.waitForURL(url('/g/hikers.groups.test'))
  await expect(page.getByRole('heading', { name: 'Hikers' })).toBeVisible()
  await expect(page.getByText('Walks on Sundays.')).toBeVisible()
  await expect(page.getByText("You're a member, as")).toBeVisible()
  // An address taken already is said as it is typed, with the next free
  // one to take; and the host's to refuse, sent all the same.
  await page.goto(url('/new'))
  await page.getByRole('textbox', { name: 'Name' }).pressSequentially('Hikers')
  await expect(page.getByText('hikers.groups.test is taken.')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Use hikers-2.groups.test' })).toBeVisible()
  await page.getByRole('button', { name: 'Start the group' }).click()
  await expect(page.locator('p.danger')).toBeVisible()
  // Its address changed by hand stays as it was put, whatever the name.
  await page.goto(url('/new'))
  await page.getByRole('textbox', { name: 'Its address' }).pressSequentially('Sunday Walks!')
  await page.getByRole('textbox', { name: 'Name' }).pressSequentially('Hikers')
  await expect(page.getByRole('textbox', { name: 'Its address' })).toHaveValue('sunday-walks-')
})

test('asking to join with a note, and being let in', async () => {
  await bob.goto(url('/g/hikers.groups.test'))
  await signIn(bob, 'bob')
  // The group, and how joining it works: someone looks at each request.
  await expect(bob.getByRole('heading', { name: 'Hikers', exact: true })).toBeVisible()
  await expect(bob.getByText('Joining is by request.')).toBeVisible()
  await expect(bob.getByRole('heading', { name: 'Ask to join' })).toBeVisible()
  const note = bob.getByRole('textbox', { name: 'A note to them, if you like' })
  await note.fill('I walk most Sundays.')
  await bob.getByRole('button', { name: 'Send request' }).click()
  await expect(bob.getByRole('heading', { name: 'Your request is waiting' })).toBeVisible()
  // Withdrawn, and asked again.
  await bob.getByRole('button', { name: 'Withdraw it' }).click()
  await expect(bob.getByRole('heading', { name: 'Ask to join' })).toBeVisible()
  await note.fill('I walk most Sundays.')
  await bob.getByRole('button', { name: 'Send request' }).click()
  await expect(bob.getByRole('heading', { name: 'Your request is waiting' })).toBeVisible()
  // Alice sees it, with his note, and lets him in.
  await page.goto(url('/g/hikers.groups.test'))
  const asking = page.locator('.requests li', { hasText: 'bob.test' })
  await expect(asking).toContainText('I walk most Sundays.')
  await asking.getByRole('button', { name: 'Let in' }).click()
  await expect(page.locator('.members', { hasText: 'bob.test' })).toBeVisible()
  await expect(page.getByText('Nobody is asking to join')).toBeVisible()
  await bob.goto(url('/g/hikers.groups.test'))
  await expect(bob.getByText("You're a member, as")).toBeVisible()
})

test('leaving a group', async () => {
  await bob.getByRole('button', { name: 'Leave the group' }).click()
  await expect(bob.getByRole('heading', { name: 'Ask to join' })).toBeVisible()
})

test('inviting someone from the dialog', async () => {
  await page.goto(url('/g/hikers.groups.test'))
  const dialog = page.getByRole('dialog', { name: 'Invite someone to Hikers' })
  const invite = page.getByRole('button', { name: 'Invite someone' })
  await invite.click()
  await expect(dialog.getByText("They'll see it the next time they sign in, here or in any app that shows invites.")).toBeVisible()
  await dialog.getByRole('textbox', { name: 'Their handle' }).fill('nobody.test')
  await dialog.getByRole('button', { name: 'Send invite' }).click()
  await expect(page.getByText('We cannot find nobody.test.')).toBeVisible()
  // Cancelled, it sends nothing.
  await invite.click()
  await dialog.getByRole('button', { name: 'Cancel' }).click()
  await expect(dialog).toBeHidden()
  await expect(page.locator('.invites')).toHaveCount(0)
  // Sent, with a note; taken back; and sent again.
  const send = async () => {
    await invite.click()
    await dialog.getByRole('textbox', { name: 'Their handle' }).fill('@bob.test')
    await dialog.getByRole('textbox', { name: 'A note, if you like' }).fill('Come walk with us.')
    await dialog.getByRole('button', { name: 'Send invite' }).click()
    await expect(page.locator('.invites', { hasText: who.bob.did })).toBeVisible()
  }
  await send()
  await page.getByRole('button', { name: 'Take back' }).click()
  await expect(page.locator('.invites', { hasText: who.bob.did })).toHaveCount(0)
  await send()
})

test('taking up an invitation', async () => {
  // On his own page, with its note; and on the group's.
  await bob.goto(url('/'))
  await expect(bob.getByRole('heading', { name: 'You are invited' })).toBeVisible()
  await expect(bob.getByText('Come walk with us.')).toBeVisible()
  await bob.goto(url('/g/hikers.groups.test'))
  await expect(bob.getByRole('heading', { name: "You're invited" })).toBeVisible()
  await expect(bob.getByText('Someone in Hikers invited you to join.')).toBeVisible()
  await bob.getByRole('button', { name: 'Join', exact: true }).click()
  await expect(bob.getByRole('heading', { name: "You're in" })).toBeVisible()
  await expect(bob.getByText('Welcome to Hikers.')).toBeVisible()
  await expect(bob.getByText("You're a member, as")).toBeVisible()
  await bob.goto(url('/'))
  await expect(bob.getByRole('heading', { name: 'You are invited' })).toHaveCount(0)
  await expect(bob.locator('.group-row', { hasText: 'Hikers' }).first()).toBeVisible()
  await page.goto(url('/g/hikers.groups.test'))
  await expect(page.locator('.members', { hasText: 'bob.test' })).toBeVisible()
})

test('giving roles, and putting someone out', async () => {
  const row = () => page.locator('.member', { hasText: 'bob.test' })
  await row().getByText('Change').click()
  await expect(row().getByRole('checkbox', { name: 'member' })).toBeChecked()
  await row().getByRole('button', { name: "Save bob.test's roles" }).click()
  await expect(row()).toContainText('member')
  await row().getByText('Change').click()
  await row().getByRole('button', { name: 'Put bob.test out' }).click()
  await expect(page.locator('.members', { hasText: 'bob.test' })).toHaveCount(0)
})

test('being told why the host refused', async () => {
  // The group's only admin may not leave it: the page says so.
  await page.getByRole('button', { name: 'Leave the group' }).click()
  await expect(page.locator('.danger')).toBeVisible()
  await expect(page.getByText("You're a member, as")).toBeVisible()
})

test("joining a group anyone can join", async () => {
  // Alice opens it to anyone.
  const settings = url('/g/hikers.groups.test/settings')
  await page.goto(settings)
  await page.getByRole('combobox', { name: 'How people join' }).selectOption('open')
  const saved = page.waitForResponse((r) => r.request().method() === 'POST' && r.url().includes('/profile'))
  await page.getByRole('button', { name: 'Save the profile' }).click()
  await saved
  // Bob, put out, joins it at once.
  await bob.goto(url('/g/hikers.groups.test'))
  await expect(bob.getByRole('heading', { name: 'Join Hikers' })).toBeVisible()
  await expect(bob.locator('.join')).toContainText('Anyone can join.')
  await bob.getByRole('button', { name: 'Join', exact: true }).click()
  await expect(bob.getByRole('heading', { name: "You're in" })).toBeVisible()
  await expect(bob.getByText("You're a member, as")).toBeVisible()
})

test('finding a member, and showing more', async () => {
  // Six in all: more than a page of five.
  for (const name of ['carol', 'dan', 'erin', 'frank']) await joins(name, 'hikers.groups.test')
  await page.goto(url('/g/hikers.groups.test'))
  await expect(page.getByRole('heading', { name: 'Members' })).toContainText('6')
  const rows = page.locator('.members > li')
  await expect(rows).toHaveCount(5)
  await page.getByRole('button', { name: 'Show more' }).click()
  await expect(rows).toHaveCount(6)
  await expect(page.getByRole('button', { name: 'Show more' })).toHaveCount(0)
  // Found by part of a handle, as it is typed.
  const find = page.getByRole('searchbox', { name: 'Find a member' })
  await find.fill('erin')
  await expect(rows).toHaveCount(1)
  await expect(rows).toContainText('erin.test')
  await expect(find).toBeFocused()
  await find.fill('nobody')
  await expect(page.getByText("No member's handle has that in it")).toBeVisible()
})

test('connecting an AI app is shown to those who run the group', async () => {
  await page.goto(url('/g/hikers.groups.test'))
  const connect = page.locator('section', { has: page.getByRole('heading', { name: 'Connect an AI app' }) })
  await expect(connect).toContainText('It acts for you in Hikers, with what your roles let you do.')
  await expect(connect.locator('code')).toHaveText(`${world.base}/mcp`)
  await expect(connect).toContainText("It'll ask you to sign in here, and to say yes.")
  await expect(connect.locator('.asks li')).toHaveCount(3)
  // Not to a plain member.
  await bob.goto(url('/g/hikers.groups.test'))
  await expect(bob.getByText("You're a member, as")).toBeVisible()
  await expect(bob.getByRole('heading', { name: 'Connect an AI app' })).toHaveCount(0)
})

test("changing a group's profile", async () => {
  await page.goto(url('/g/hikers.groups.test'))
  await page.getByRole('link', { name: 'Settings' }).click()
  await page.getByRole('textbox', { name: 'Name', exact: true }).fill('Hikers of the Hills')
  await page.getByRole('textbox', { name: 'Link 1', exact: true }).fill('https://chat.hikers.example')
  await page.getByRole('textbox', { name: 'What link 1 is' }).fill('Our chat')
  await page.getByRole('textbox', { name: 'Link 2', exact: true }).fill('https://walks.example/')
  await page.getByRole('button', { name: 'Save the profile' }).click()
  await page.goto(url('/g/hikers.groups.test'))
  await expect(page.getByRole('heading', { name: 'Hikers of the Hills' })).toBeVisible()
  await expect(page.getByText('Walks on Sundays.')).toBeVisible()
  // Its links, in a row: by title, or by address.
  const links = page.getByRole('navigation', { name: "Hikers of the Hills's links" })
  await expect(links.getByRole('link', { name: 'Our chat' })).toHaveAttribute('href', 'https://chat.hikers.example')
  await expect(links.getByRole('link', { name: 'walks.example' })).toHaveAttribute('href', 'https://walks.example/')
})

// A one-pixel PNG.
const PIXEL = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64')

test("giving a group a picture, and taking it away", async () => {
  await page.goto(url('/g/hikers.groups.test/settings'))
  await page.getByLabel('Avatar').setInputFiles({ name: 'face.png', mimeType: 'image/png', buffer: PIXEL })
  await page.getByRole('button', { name: 'Upload the avatar' }).click()
  // Drawn on its page, from the host, as the image it is: one pixel wide.
  await page.goto(url('/g/hikers.groups.test'))
  const avatar = page.locator('.identity img.av')
  await expect(avatar).toBeVisible()
  await expect.poll(() => avatar.evaluate((img) => img.naturalWidth)).toBe(1)
  await page.goto(url('/g/hikers.groups.test/settings'))
  await page.getByRole('button', { name: 'Take the avatar away' }).click()
  await page.goto(url('/g/hikers.groups.test'))
  await expect(page.locator('.identity img.av')).toHaveCount(0)
})

test("hiding a group's profile, and showing it again", async () => {
  // The host lists every group of its own to someone signed in, as bob
  // is, a member of none; a hidden one by its handle alone.
  const listed = async () => {
    const { groups } = await bob.evaluate(() => fetch('/xrpc/fyi.opensocial.listGroups').then((r) => r.json()))
    return groups.find((g) => g.handle === 'hikers.groups.test')?.displayName !== undefined
  }
  expect(await listed()).toBe(true)
  await page.goto(url('/g/hikers.groups.test/settings'))
  // Saved, and the page opened afresh as the host has it: not reloaded,
  // which would send the form's post again.
  const settings = url('/g/hikers.groups.test/settings')
  const save = async (label) => {
    await page.goto(settings)
    await page.getByLabel(label).check()
    const saved = page.waitForResponse((r) => r.request().method() === 'POST' && r.url().includes('/visibility'))
    await page.getByRole('button', { name: 'Save who sees it' }).click()
    await saved
    await page.goto(settings)
    await expect(page.getByLabel(label)).toBeChecked()
  }
  await save('Its members alone: to others, just its handle')
  await expect.poll(listed).toBe(false)
  await save('Anyone, and found on the network')
  await expect.poll(listed).toBe(true)
})

test('adding a rule, and taking it away', async () => {
  await page.goto(url('/g/hikers.groups.test/settings'))
  await page.getByRole('textbox', { name: 'Rule' }).fill('Leave no trace')
  await page.getByRole('button', { name: 'Add the rule' }).click()
  await expect(page.locator('.rules', { hasText: 'Leave no trace' })).toBeVisible()
  // On the group's page, folded under how many there are.
  await page.goto(url('/g/hikers.groups.test'))
  await page.getByText('Rules (1)').click()
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
  // The console's own token, from inviting, stays.
  await expect(page.locator('.sessions', { hasText: 'app-password:Phone' })).toHaveCount(0)
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
  // Putting him out is already in the queue, on him.
  const ejected = page.locator('.subject', { hasText: who.bob.did })
  await expect(ejected.locator('.status')).toHaveText('open')
  await expect(ejected.locator('.events')).toContainText('eject')
  await page.getByRole('textbox', { name: 'Their handle' }).fill('bob.test')
  await page.getByRole('textbox', { name: 'Label' }).fill('troll')
  await page.getByRole('button', { name: 'Label them' }).click()
  const subject = page.locator('.subject', { hasText: who.bob.did })
  await expect(subject.locator('.status')).toHaveText('open')
  await expect(subject.locator('.events')).toContainText('label: troll')
  // Escalated with a note, it leaves the open queue for the escalated.
  await subject.getByRole('textbox', { name: 'Note' }).fill('Asked him to stop')
  await subject.getByRole('button', { name: 'Escalate' }).click()
  await expect(page.locator('.subject', { hasText: who.bob.did })).toHaveCount(0)
  await page.goto(url('/g/hikers.groups.test/moderation?status=escalated'))
  await expect(page.locator('.subject', { hasText: who.bob.did }).locator('.status')).toHaveText('escalated')
  await expect(page.locator('.subject', { hasText: who.bob.did }).locator('.events')).toContainText('escalated: Asked him to stop')
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

test('connecting an AI app, and disconnecting it', async () => {
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
  await expect(page.locator('.ai-apps', { hasText: 'Pocket assistant' })).toBeVisible()
  await page.getByRole('button', { name: 'Disconnect Pocket assistant' }).click()
  await expect(page.getByText('No AI app connected yet')).toBeVisible()
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
  await expect(page.getByRole('banner').getByRole('button', { name: 'Sign in' })).toBeVisible()
  await expect.poll(() => signedInAs(page)).toBe('')
})
