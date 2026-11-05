// The console used as people use it: signed in to through the PDS's own
// approval page, and an app signed in as a group through it, end to end.
//
//   just e2e    a world of its own, and the flows
//
// The flows go in order in one worker, as each goes on from where the one
// before left the browser and the world.

import { defineConfig } from '@playwright/test';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolving } from '../world.mjs';

const root = resolve(fileURLToPath(new URL('.', import.meta.url)), '../..');

export default defineConfig({
  testDir: '.',
  testMatch: 'flows.spec.mjs',
  globalSetup: './setup.mjs',
  outputDir: join(root, 'target/e2e/results'),
  workers: 1,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  reporter: [['list']],
  use: {
    browserName: 'chromium',
    viewport: { width: 1280, height: 800 },
    locale: 'en-US',
    // The console is reached as the PDS reaches it: over https, at the
    // harness's console hostname, with the harness's own certificate.
    ignoreHTTPSErrors: true,
    launchOptions: { args: [resolving] },
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure'
  }
});
