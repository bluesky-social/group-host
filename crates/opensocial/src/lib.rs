//! A client for opensocial group hosts, and what the `opensocial` command
//! does with it. Each command arrives with the methods it calls; see
//! `docs/design/README.md` for the order.

use anyhow::{Result, bail};

/// What `opensocial` prints for `--help`, or with no command.
pub const USAGE: &str = "usage: opensocial <command>

  Commands arrive as the host's methods do: see docs/reference/cli.md.
";

/// Runs the command `args` name (without the program's own name),
/// returning what it prints.
pub fn run(args: &[String]) -> Result<String> {
    match args.first().map(String::as_str) {
        None | Some("help" | "--help" | "-h") => Ok(USAGE.to_string()),
        Some(other) => bail!("no command {other}\n\n{USAGE}"),
    }
}
