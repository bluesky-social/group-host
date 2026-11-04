//! The reference network (`harness/pds.mjs`): a PLC directory and the
//! reference PDS, on the space alpha where `GROUP_HOST_DEV_ENV` names its
//! build, started once for a test binary and stopped as it exits.

use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

#[derive(Deserialize, Clone, Debug)]
pub struct Net {
    pub pds: String,
    pub plc: String,
    /// Whether the PDS is on the space alpha.
    pub spaces: bool,
    /// Where the codes the PDS mails are read: `GET <mail>?to=<address>`.
    pub mail: String,
    pub accounts: Vec<Member>,
}

/// An account on the reference PDS.
#[derive(Deserialize, Clone, Debug)]
pub struct Member {
    pub handle: String,
    pub did: String,
    pub password: String,
}

static NET: OnceLock<Net> = OnceLock::new();
/// Kept so the harness lives as long as the tests; its stdin closes, and
/// it stops, when this process exits.
static CHILD: Mutex<Option<Child>> = Mutex::new(None);

pub fn net() -> &'static Net {
    NET.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut child = Command::new("node")
            .arg(root.join("harness/pds.mjs"))
            .args(["--accounts", "alice,bob"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("node runs harness/pds.mjs (npm ci in harness/)");
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let net: Net = loop {
            let line = lines
                .next()
                .expect("the harness reports before it stops")
                .unwrap();
            if line.starts_with("{\"pds\"") {
                break serde_json::from_str(&line).unwrap();
            }
        };
        // Named, the alpha is what the tests are to run on: none skips.
        let named = std::env::var_os("GROUP_HOST_DEV_ENV").is_some();
        assert!(
            net.spaces || !named,
            "GROUP_HOST_DEV_ENV names a PDS without spaces"
        );
        std::thread::spawn(move || for _ in lines {});
        *CHILD.lock().unwrap() = Some(child);
        net
    })
}
