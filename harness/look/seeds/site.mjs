// Riders with a website published: a front page, a page under it, and a
// style sheet.

import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export default async ({ opensocial }) => {
  const dir = mkdtempSync(join(tmpdir(), 'look-site-'));
  mkdirSync(join(dir, 'rides'));
  writeFileSync(join(dir, 'index.html'), '<h1>Riders</h1><p>Saturday rides.</p>');
  writeFileSync(join(dir, 'rides/index.html'), '<h1>Rides</h1>');
  writeFileSync(join(dir, 'style.css'), 'h1 { color: teal }');
  opensocial('alice', 'site', 'publish', 'riders.groups.test', dir);
};
