//! The host moved to a new address: the URL it answers at, and its
//! did:web. The groups keep their DIDs; what names the host changes: each
//! account's DID document, every service at the old address pointed at
//! the new one, signed with the host's rotation key, and the relays told
//! who each account is now.
//!
//! `group-host move-host --from <old address> [--dry-run]`, once, with the
//! host stopped and its environment naming the new address (`PUBLIC_URL`,
//! `SERVICE_DID`). Run again, it finds nothing left to repoint, and tells
//! the relays again of each account at the new address.
//!
//! The host keeps no app managing its spaces and no PDS per group, so
//! only the documents change.

use crate::Host;
use crate::repo::identity;
use crate::store::Store;
use anyhow::{Result, bail};
use atproto::plc::Signed;

/// What became of an account's DID document.
#[derive(Debug, Clone, PartialEq)]
pub enum Moved {
    /// Its services named here: those that were at the old address.
    Repointed(Vec<String>),
    /// Already at the new address, or never at the old.
    Current,
    /// Its DID is ended.
    Ended,
    /// This host holds no rotation key to it any more.
    NotOurs,
}

impl Store {
    /// Keeps `events` of `did`'s, for the relays, with nothing else.
    pub fn tell(&self, did: &str, events: &[crate::store::Event]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        crate::store::sequence(&tx, did, events)?;
        tx.commit()?;
        Ok(())
    }
}

impl Host {
    /// `did`'s document, every service at `from` pointed at this host's
    /// address, unless `dry_run`.
    async fn repoint(&self, did: &str, handle: &str, from: &str, dry_run: bool) -> Result<Moved> {
        let to = self.config.public_url.trim_end_matches('/');
        let last = self.plc.last_op(did).await?;
        let Signed::Update { op, .. } = &last else {
            return Ok(Moved::Ended);
        };
        let at = |endpoint: &str, here: &str| endpoint.trim_end_matches('/') == here;
        let stale: Vec<String> = op
            .services
            .iter()
            .filter(|(_, s)| at(&s.endpoint, from))
            .map(|(id, _)| id.clone())
            .collect();
        if stale.is_empty() {
            let here = op.services.values().any(|s| at(&s.endpoint, to));
            if here && !dry_run {
                self.store.tell(did, &[identity(did, handle)])?;
            }
            return Ok(Moved::Current);
        }
        let ours = self.secrets.rotation.public().did_key();
        if !op.rotation_keys.contains(&ours) {
            return Ok(Moved::NotOurs);
        }
        if !dry_run {
            let mut next = op.clone();
            for id in &stale {
                let service = next.services.get_mut(id).expect("a service it has");
                service.endpoint = to.to_string();
            }
            let signed = next.sign(Some(last.cid()), &self.secrets.rotation);
            self.plc.publish(did, &signed).await?;
            self.store.tell(did, &[identity(did, handle)])?;
        }
        Ok(Moved::Repointed(stale))
    }
}

/// Moves `host`, whose environment names its new address, from `from`:
/// what it did, a line each.
pub async fn move_host(host: &Host, from: &str, dry_run: bool) -> Result<Vec<String>> {
    let from = from.trim_end_matches('/');
    let to = host.config.public_url.trim_end_matches('/');
    if from == to {
        bail!("the environment still names {from}: set PUBLIC_URL to the new address first");
    }
    let mut said = vec![format!(
        "{from} → {to} ({}){}",
        host.config.service_did,
        if dry_run {
            ": a dry run, nothing is written"
        } else {
            ""
        }
    )];
    let (mut cursor, mut seen, mut moved) = (None::<String>, 0, 0);
    loop {
        let page = host.store.accounts(cursor.as_deref(), 100)?;
        let Some((last, _)) = page.last() else {
            break;
        };
        cursor = Some(last.clone());
        for (did, _) in &page {
            seen += 1;
            let account = host.store.account(did)?.expect("an account listed");
            let name = &account.handle;
            match host.repoint(did, name, from, dry_run).await? {
                Moved::Repointed(ids) => {
                    moved += 1;
                    said.push(format!("{name}: {} → {to}", ids.join(", ")));
                }
                Moved::Current => {}
                Moved::Ended => said.push(format!("{name}: its DID is ended, skipped")),
                Moved::NotOurs => said.push(format!(
                    "{name}: this host holds no rotation key to it, skipped"
                )),
            }
        }
    }
    let done = if dry_run { "to repoint" } else { "repointed" };
    said.push(format!("{moved} of {seen} DID documents {done}"));
    Ok(said)
}
