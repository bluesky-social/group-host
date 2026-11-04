# The commands and the recipes

## `group-host`

| Command | What it does |
| --- | --- |
| none | serves, as the environment says ([config](config.md)) |
| `move-host --from <old address> [--dry-run]` | points each account's DID document at the address the environment names now ([moving the host](../design/moving.md#moving-the-host)) |

Anything else fails with its usage.

## `opensocial`

| Command | What it does |
| --- | --- |
| `help`, `--help`, `-h`, or none | prints its usage |

Any other command fails with `no command <name>` and the usage. Commands
arrive with the methods they call (see [the order](../design/README.md#the-order)).

## `just`

| Recipe | What it does |
| --- | --- |
| `setup` | installs the console's packages and cargo-llvm-cov |
| `test`, `t <args>` | every test, or those `args` pick |
| `cov`, `cov-html` | the tests under the 100% gate, or their coverage as a page |
| `lint` | formatting, clippy, the console's layers and svelte-check |
| `lex` | the lexicons' checks |
| `serve` | the host |
| `web`, `web-dev` | the console built, or served as its source changes |
| `e2e <args>` | the console's flows in Chromium over the reference PDS, as the `e2e` check runs them |
| `opensocial <args>` | the command |
