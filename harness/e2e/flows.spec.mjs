// What people do on the console, in order, as alice: sign in at her own
// PDS, see her groups, sign an app in as one of them, and decline one;
// and what the app then does as the group, read back from the host.

import { expect, test } from '@playwright/test'
import { App, signIn, signedInAs, url, world } from './pages.mjs'

test.describe.configure({ mode: 'serial' })

let page

test.beforeAll(async ({ browser }) => {
  page = await browser.newPage()
})

test.afterAll(async () => {
  await page.close()
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

test('signing an app in as her group', async () => {
  const app = await new App('atproto repo:app.bsky.feed.post').listen()
  await page.goto(await app.push())
  await expect(page.getByRole('heading', { name: 'Sign in an app' })).toBeVisible()
  await expect(page.getByText('repo:app.bsky.feed.post')).toBeVisible()
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

test('signing out', async () => {
  await page.goto(url('/'))
  await page.getByRole('button', { name: 'Sign out' }).click()
  await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible()
  await expect.poll(() => signedInAs(page)).toBe('')
})
