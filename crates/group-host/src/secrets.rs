//! The host's own keys, kept in `host-secrets.json` in its data directory,
//! made on its first start: the rotation key it signs its groups' PLC
//! operations with, and the key its own `did:web` signs with.

use anyhow::{Context, Result};
use atproto::{Curve, Keypair};
use serde_json::{Value, json};
use std::path::Path;

pub struct Secrets {
    /// Signs every PLC operation of the groups here, first of their
    /// rotation keys.
    pub rotation: Keypair,
    /// The host's own signing key, in its `did:web` document.
    pub signing: Keypair,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    bytes
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

impl Secrets {
    /// The secrets in `dir`, made there first if there are none, the file
    /// readable by the host's user alone.
    pub fn load(dir: &Path) -> Result<Secrets> {
        let file = dir.join("host-secrets.json");
        if file.exists() {
            let text = std::fs::read_to_string(&file)?;
            let json: Value =
                serde_json::from_str(&text).context("host-secrets.json is not JSON")?;
            let key = |name: &str| {
                let secret = json[name].as_str().and_then(unhex);
                let secret = secret.with_context(|| format!("host-secrets.json has no {name}"))?;
                Ok::<_, anyhow::Error>(Keypair::from_bytes(Curve::K256, &secret)?)
            };
            return Ok(Secrets {
                rotation: key("rotationKey")?,
                signing: key("signingKey")?,
            });
        }
        let secrets = Secrets {
            rotation: Keypair::generate(Curve::K256),
            signing: Keypair::generate(Curve::K256),
        };
        std::fs::create_dir_all(dir)?;
        let json = json!({
            "rotationKey": hex(&secrets.rotation.to_bytes()),
            "signingKey": hex(&secrets.signing.to_bytes()),
        });
        write_private(&file, &serde_json::to_string_pretty(&json)?)?;
        Ok(secrets)
    }
}

fn write_private(file: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(file)?;
    f.write_all(text.as_bytes())?;
    Ok(())
}
