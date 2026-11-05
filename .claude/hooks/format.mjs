// Claude Code's PostToolUse hook: formats each file an agent writes as
// `just fmt` would, so what is committed passes the format checks. Rust
// by rustfmt; the console's, the harness's and the docs site's code by
// oxfmt. Anything else, or a file a formatter cannot parse yet, is left as
// written: the checks still catch what is pushed.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { relative, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../..');
const input = JSON.parse(readFileSync(0, 'utf8'));
const file = input.tool_input?.file_path;
if (!file || !existsSync(file)) process.exit(0);
const path = relative(root, resolve(file));

const run = (cmd, args) => {
  try {
    execFileSync(cmd, args, { cwd: root, stdio: 'ignore' });
  } catch {
    // Left as written.
  }
};

if (path.endsWith('.rs')) {
  run('rustup', ['run', 'stable', 'rustfmt', '--edition', '2024', path]);
} else if (
  /^(web|harness|docs-site)\//.test(path) &&
  !path.includes('node_modules/') &&
  /\.(svelte|ts|js|mjs|cjs|css|json)$/.test(path)
) {
  const oxfmt = resolve(root, 'web/node_modules/.bin/oxfmt');
  if (existsSync(oxfmt)) run(oxfmt, ['--no-error-on-unmatched-pattern', path]);
}
