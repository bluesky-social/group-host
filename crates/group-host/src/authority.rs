//! Being the authority for groups' spaces: which spaces a group here has,
//! who may read and write each, the repos written in each (the writer
//! set `listWriters` answers, with the repos taken out of it), and telling
//! the services that sync a space of each change to it.
//!
//! Who may do what is the host's own to decide, from its grants: the
//! group itself always; others as granted, or anyone reading a space open
//! to all. Groups and their roles fill the grants in.
//!
//! A syncer registers an endpoint as it authorizes, for as long as the
//! credential it gets lasts. Each change to the writer set is sent there
//! once, best effort; word that its registration lapsed, or that the space
//! was deleted, is kept and tried again after a minute, ten minutes, an
//! hour, six hours, then every twelve, for three days.
//!
//! As a repo host, it owes the authority of each space a group writes to
//! word of the write: kept in an outbox and tried again until it lands,
//! or a day has passed, as pds.js's `outbox.js` does.

use crate::Host;
use crate::guard::pinned_client;
use crate::index_api::capped;
use crate::rbac::Policy;
use crate::repo::invalid;
use crate::space::NOTIFY_REPO_UPDATE;
use crate::space_api::NOTIFY_REVOKED;
use crate::store::{Notice, Registration, Space, Store, SyncerNotice, SyncerUpdate};
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::{Context, Result};
use atproto::service_auth::{self, Claims};
use atproto::space::SpaceRef;
use atproto::space_auth::{self, space_host_aud};
use atproto::{Value, jose, syntax};
use axum::http::StatusCode;
use futures::future::join_all;
use futures::{StreamExt, stream};
use serde_json::{Value as Json, json};

/// What a user would do in a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// The method an authority tells a syncer of a change to a space with.
pub const NOTIFY_SPACE_UPDATE: &str = "com.atproto.space.notifySpaceUpdate";
/// The method an authority asks a syncer's endpoint to confirm it asked
/// to be registered with.
pub const CONFIRM_REGISTRATION: &str = "com.atproto.space.confirmRegistration";

/// How long a write's word is tried before it is given up, in seconds.
pub const DELIVER_FOR: i64 = 24 * 60 * 60;
/// The wait before the first retry, doubling each after, up to an hour.
const FIRST_RETRY: i64 = 10;
const LAST_RETRY: i64 = 60 * 60;
/// How many notices one round of delivery tries.
const ROUND: u32 = 50;

/// How long word owed to a syncer is tried before it is given up, in
/// seconds.
pub const SYNCER_DELIVER_FOR: i64 = 72 * 60 * 60;
/// The waits before each retry of word owed to a syncer, the last
/// repeated.
const SYNCER_RETRIES: [i64; 5] = [60, 10 * 60, 60 * 60, 6 * 60 * 60, 12 * 60 * 60];
/// How many syncers are told at once.
const AT_ONCE: usize = 50;
/// The most bytes a syncer's confirmation is read to.
const CONFIRMATION_AT_MOST: usize = 4096;

/// The wait before retry number `attempts`, counted from 1, in seconds.
pub fn retry_delay(attempts: i64) -> i64 {
    let doubled = FIRST_RETRY.saturating_mul(1 << (attempts - 1).clamp(0, 20));
    doubled.min(LAST_RETRY)
}

/// The wait before retry number `attempts` of word owed to a syncer,
/// counted from 1, in seconds.
pub fn syncer_retry_delay(attempts: i64) -> i64 {
    SYNCER_RETRIES[(attempts.clamp(1, 5) - 1) as usize]
}

fn now() -> i64 {
    now_millis() / 1000
}

/// What delivering a notice came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Delivered,
    /// It may land later: the recipient was not reached, or said so.
    Retry,
    /// It never will: the recipient refused it.
    Failed,
}

/// A repo written in a space, at the space's revision `space_rev`, after
/// `prev`: as a syncer is told it, and `listWriters` lists it.
pub fn repo_write(
    repo: &str,
    repo_rev: &str,
    data: &[u8],
    space_rev: &str,
    prev: Option<&str>,
) -> Json {
    let update = json!({
        "$type": "com.atproto.space.defs#repoWrite",
        "repo": repo,
        "repoRev": repo_rev,
        "data": Value::Bytes(data.to_vec()).to_json(),
        "spaceRev": space_rev,
    });
    after(update, prev)
}

/// A repo taken out of a space's writer set, at the space's revision
/// `space_rev`, after `prev`.
pub fn repo_removed(repo: &str, space_rev: &str, prev: Option<&str>) -> Json {
    let update = json!({
        "$type": "com.atproto.space.defs#repoRemoved",
        "repo": repo,
        "spaceRev": space_rev,
    });
    after(update, prev)
}

fn after(mut update: Json, prev: Option<&str>) -> Json {
    if let Some(prev) = prev {
        update["prevSpaceRev"] = json!(prev);
    }
    update
}

/// Word owed to a syncer, as it is told it.
fn syncer_update(update: &SyncerUpdate) -> Json {
    match update {
        SyncerUpdate::SpaceDeleted => {
            json!({"$type": format!("{NOTIFY_SPACE_UPDATE}#spaceDeleted")})
        }
        SyncerUpdate::RegistrationExpired { space_rev } => json!({
            "$type": format!("{NOTIFY_SPACE_UPDATE}#registrationExpired"),
            "spaceRev": space_rev,
        }),
    }
}

/// What a request's answer says of delivering it.
fn delivery(sent: reqwest::Result<reqwest::Response>) -> Delivery {
    match sent.map(|r| r.status()) {
        Ok(s) if s.is_success() => Delivery::Delivered,
        Ok(s) if retryable(s) => Delivery::Retry,
        Ok(_) => Delivery::Failed,
        Err(_) => Delivery::Retry,
    }
}

/// What telling syncers needs, owned, so that it goes on apart from the
/// request or write that set it off.
#[derive(Clone)]
struct Syncers {
    store: Store,
    /// Whether the host is on http, and so may call endpoints on http.
    http_here: bool,
}

impl Syncers {
    /// Tells the syncer at `to` of `update` to `space`, once.
    async fn send(&self, space: &str, to: &Registration, update: &Json) -> Delivery {
        let url = format!(
            "{}/xrpc/{NOTIFY_SPACE_UPDATE}",
            to.endpoint.trim_end_matches('/')
        );
        // The syncer chose the endpoint: it is fetched as one not trusted.
        let Ok((client, at)) = pinned_client(&url, self.http_here).await else {
            return Delivery::Retry;
        };
        let body = json!({"space": space, "token": to.token, "update": update});
        delivery(client.post(at).json(&body).send().await)
    }

    /// Tells each syncer registered for `space` of `update`, a change to
    /// its writer set at `space_rev`, best effort; then owes those whose
    /// registration lapsed since the last change word of it, in place of
    /// the change.
    async fn forward(&self, space: &str, update: &Json, space_rev: &str) -> Result<()> {
        let now = now();
        let active = self.store.registrations(space, now)?;
        // All at once, so that one slow endpoint holds up none of the others.
        stream::iter(&active)
            .for_each_concurrent(AT_ONCE, |to| async move {
                let told = self.send(space, to, update).await;
                if told != Delivery::Delivered {
                    tracing::warn!(
                        space,
                        endpoint = to.endpoint,
                        ?told,
                        "notifySpaceUpdate failed"
                    );
                }
            })
            .await;
        let lapsed = self.store.claim_lapsed(space, now)?;
        let expired = SyncerUpdate::RegistrationExpired {
            space_rev: space_rev.into(),
        };
        self.notify(space, lapsed, &expired, now).await
    }

    /// Owes each syncer at `to` word of `update` to `space`: tried now,
    /// and kept to be tried again if it may land later.
    async fn notify(
        &self,
        space: &str,
        to: Vec<Registration>,
        update: &SyncerUpdate,
        now: i64,
    ) -> Result<()> {
        let body = &syncer_update(update);
        let tried: Vec<(Registration, Delivery)> = stream::iter(to)
            .map(|to| async move {
                let told = self.send(space, &to, body).await;
                (to, told)
            })
            .buffer_unordered(AT_ONCE)
            .collect()
            .await;
        for (to, told) in tried {
            match told {
                Delivery::Delivered => {}
                Delivery::Failed => self.give_up(space, &to, update, now)?,
                Delivery::Retry => self.store.owe_syncer(&SyncerNotice {
                    space: space.into(),
                    to,
                    update: update.clone(),
                    attempts: 1,
                    next_at: now + syncer_retry_delay(1),
                    deliver_by: now + SYNCER_DELIVER_FOR,
                })?,
            }
        }
        Ok(())
    }

    /// Gives up telling the syncer at `to` of `update`: one that refuses
    /// word of its lapse, or is never reached, is registered no more,
    /// unless it renewed since. A deleted space's registrations went with
    /// it.
    fn give_up(
        &self,
        space: &str,
        to: &Registration,
        update: &SyncerUpdate,
        now: i64,
    ) -> Result<()> {
        if let SyncerUpdate::RegistrationExpired { .. } = update {
            self.store.unregister_lapsed(space, to, now)?;
        }
        Ok(())
    }

    /// Tries `notice` again, fallen due at `now`.
    async fn retry(&self, notice: &SyncerNotice, now: i64) -> Result<()> {
        let space = SpaceRef::parse(&notice.space)?;
        // Word waits while the space's authority is switched off.
        if self.store.deactivation(&space.authority)?.is_some() {
            return self.reschedule(notice, now);
        }
        if !self.owed(notice, now)? {
            return self.store.syncer_told(notice);
        }
        let body = syncer_update(&notice.update);
        match self.send(&notice.space, &notice.to, &body).await {
            Delivery::Delivered => self.store.syncer_told(notice),
            Delivery::Failed => {
                self.store.syncer_told(notice)?;
                self.give_up(&notice.space, &notice.to, &notice.update, now)
            }
            Delivery::Retry => self.reschedule(notice, now),
        }
    }

    /// Whether `notice` still says what is so: a lapse the syncer has not
    /// renewed since, or a deletion the space has not been made again
    /// since.
    fn owed(&self, notice: &SyncerNotice, now: i64) -> Result<bool> {
        Ok(match &notice.update {
            SyncerUpdate::RegistrationExpired { .. } => self
                .store
                .registration(&notice.space, &notice.to)?
                .is_some_and(|until| until <= now),
            SyncerUpdate::SpaceDeleted => self.store.space_deleted(&notice.space)?,
        })
    }

    /// Tries `notice` again after its next wait; or, its three days past,
    /// gives it up.
    fn reschedule(&self, notice: &SyncerNotice, now: i64) -> Result<()> {
        if notice.deliver_by <= now {
            let (space, endpoint) = (&notice.space, &notice.to.endpoint);
            tracing::warn!(
                space,
                endpoint,
                "word to a syncer given up after three days"
            );
            self.store.syncer_told(notice)?;
            return self.give_up(&notice.space, &notice.to, &notice.update, now);
        }
        let next = now + syncer_retry_delay(notice.attempts + 1);
        self.store.retry_syncer(notice, next)
    }
}

impl Host {
    /// Service auth from `iss` to `aud` for `lxm`, for a minute.
    pub(crate) fn claims(&self, iss: &str, aud: &str, lxm: &str) -> Claims {
        let now = now() as u64;
        Claims {
            iss: iss.into(),
            aud: aud.into(),
            exp: now + 60,
            iat: Some(now),
            lxm: Some(lxm.into()),
            jti: Some(self.clock.next()),
        }
    }

    /// Makes a space of `authority`'s, a group here, of `kind` at `skey`,
    /// under `policy`. Its URI.
    pub fn create_space(
        &self,
        authority: &str,
        kind: &str,
        skey: &str,
        policy: &Json,
    ) -> Result<String, XrpcError> {
        let uri = crate::group::space_uri(authority, kind, skey);
        SpaceRef::parse(&uri).map_err(|e| invalid(e.to_string()))?;
        let account = self.store.account(authority).map_err(XrpcError::internal)?;
        if !account.is_some_and(|a| a.did == authority) {
            return Err(invalid(format!("{authority} is no group here")));
        }
        let made = self
            .store
            .create_space(&uri, authority, policy, now())
            .map_err(XrpcError::internal)?;
        if !made {
            return Err(XrpcError::new(
                StatusCode::BAD_REQUEST,
                "SpaceAlreadyExists",
                format!("{uri} exists"),
            ));
        }
        Ok(uri)
    }

    /// The policy of the space at `uri`, if it is one here.
    pub fn policy(&self, uri: &str) -> Result<Option<Policy>> {
        let Some(space) = self.store.space(uri)? else {
            return Ok(None);
        };
        let at = SpaceRef::parse(uri)?;
        Ok(Some(Policy::of(&space.policy, &at)))
    }

    /// The space at `uri`, if its authority is a group here.
    pub fn hosted_space(&self, uri: &str) -> Result<Space, XrpcError> {
        let space = self.store.space(uri).map_err(XrpcError::internal)?;
        space.ok_or_else(|| {
            XrpcError::new(StatusCode::BAD_REQUEST, "SpaceNotFound", "space not found")
        })
    }

    /// Lets `did` read `space`, write it, or both: what it was let do
    /// before is replaced.
    pub fn grant(&self, space: &str, did: &str, read: bool, write: bool) -> Result<(), XrpcError> {
        self.hosted_space(space)?;
        if !syntax::is_did(did) {
            return Err(invalid(format!("{did} is no DID")));
        }
        let granted = self.store.grant(space, did, read, write);
        granted.map_err(XrpcError::internal)
    }

    /// Takes back whatever `did` was let do in `space`.
    pub fn ungrant(&self, space: &str, did: &str) -> Result<(), XrpcError> {
        self.hosted_space(space)?;
        self.store.ungrant(space, did).map_err(XrpcError::internal)
    }

    /// Whether `user` may do `access` in `space` (at `uri`): with no user,
    /// whether anyone may read it.
    pub fn may(
        &self,
        uri: &str,
        space: &Space,
        user: Option<&str>,
        access_kind: Access,
    ) -> Result<bool> {
        // Nothing reads or writes in the spaces of an authority switched off.
        if self.store.deactivation(&space.authority)?.is_some() {
            return Ok(false);
        }
        let Some(user) = user else {
            return Ok(access_kind == Access::Read && space.public_read);
        };
        // The authority never locks itself out of its own space.
        if user == space.authority {
            return Ok(true);
        }
        // Its policy decides, against the user's roles in the group; then
        // what the host keeps of each account beyond it, from a space
        // brought here with its members.
        let policy = Policy::of(&space.policy, &SpaceRef::parse(uri)?);
        let roles = self.roles(&space.authority, user)?;
        let granted = self.store.granted(uri, user)?;
        Ok(match access_kind {
            Access::Read => policy.can_read(&roles) || granted.is_some_and(|(read, _)| read),
            Access::Write => policy.writes_own(&roles) || granted.is_some_and(|(_, write)| write),
        })
    }

    fn syncers(&self) -> Syncers {
        Syncers {
            store: self.store.clone(),
            http_here: self.config.public_url.starts_with("http://"),
        }
    }

    /// Tells the syncers of `space` of `update`, a change to its writer
    /// set at `space_rev`, apart from what set it off: neither a write here
    /// nor a repo host telling of one waits on them.
    fn forward(&self, space: &str, update: Json, space_rev: String) {
        let (syncers, space) = (self.syncers(), space.to_string());
        tokio::spawn(async move {
            let forwarded = syncers.forward(&space, &update, &space_rev).await;
            tracing::debug!(space, ?forwarded, "forwarded a change to the writer set");
        });
    }

    /// A write to `space`, of a group here, by `repo`: kept in the writer
    /// set unless it knows a later one, and told to the space's syncers.
    pub fn record_writer(
        &self,
        space: &str,
        repo: &str,
        repo_rev: &str,
        data: &[u8],
    ) -> Result<()> {
        let space_rev = self.clock.next();
        let kept = self
            .store
            .record_writer(space, repo, repo_rev, data, &space_rev)?;
        let Some(prev) = kept else {
            return Ok(());
        };
        self.reindex(space);
        let update = repo_write(repo, repo_rev, data, &space_rev, prev.as_deref());
        self.forward(space, update, space_rev);
        Ok(())
    }

    /// Takes `repo` out of `space`'s writer set, the index forgetting what
    /// it read of it there, and tells the space's syncers.
    pub fn remove_writer(&self, space: &str, repo: &str) -> Result<()> {
        self.store.unindex(space, repo)?;
        let space_rev = self.clock.next();
        let removed = self.store.remove_writer(space, repo, &space_rev, now())?;
        if let Some(prev) = removed {
            let update = repo_removed(repo, &space_rev, prev.as_deref());
            self.forward(space, update, space_rev);
        }
        Ok(())
    }

    /// Takes out of the writer set of `space` (at `uri`) each repo that
    /// may write there no more, as its policy and the roles it reads have
    /// it now; with `only`, that repo alone.
    pub fn prune_writers(&self, uri: &str, space: &Space, only: Option<&str>) -> Result<()> {
        for writer in self.store.writers(uri, None, u32::MAX)? {
            let other = only.is_some_and(|o| o != writer.did);
            if other || self.may(uri, space, Some(&writer.did), Access::Write)? {
                continue;
            }
            self.remove_writer(uri, &writer.did)?;
        }
        Ok(())
    }

    /// Takes `member` out of the writer set of each of `group`'s spaces
    /// they may write in no more.
    pub fn prune_writer_of(&self, group: &str, member: &str) -> Result<()> {
        for uri in self.store.spaces_of(group)? {
            let space = self.store.space(&uri)?.context("a space of the group's")?;
            self.prune_writers(&uri, &space, Some(member))?;
        }
        Ok(())
    }

    /// Asks the syncer endpoint `to` names to confirm it asked to be told
    /// of `space`: it echoes a challenge, within ten seconds, unredirected.
    pub(crate) async fn confirm_registration(
        &self,
        space: &str,
        to: &Registration,
    ) -> Result<(), XrpcError> {
        let challenge = jose::random(16);
        let url = format!(
            "{}/xrpc/{CONFIRM_REGISTRATION}",
            to.endpoint.trim_end_matches('/')
        );
        let body = json!({"space": space, "token": to.token, "challenge": challenge});
        let echoed = async {
            // The syncer chose the endpoint: it is fetched as one not
            // trusted.
            let (client, at) = self.pinned(&url).await.ok()?;
            let res = client.post(at).json(&body).send().await.ok()?;
            let bytes = capped(res, CONFIRMATION_AT_MOST).await?;
            let said: Json = serde_json::from_slice(&bytes).ok()?;
            said["challenge"].as_str().map(String::from)
        };
        if echoed.await.as_deref() == Some(challenge.as_str()) {
            return Ok(());
        }
        tracing::warn!(space, endpoint = to.endpoint, "registration not confirmed");
        Err(XrpcError::new(
            StatusCode::BAD_REQUEST,
            "RegistrationNotConfirmed",
            "the notify endpoint did not confirm the registration",
        ))
    }

    /// Tells the syncers of each of `authority`'s spaces, lapsed or not,
    /// that it is gone: best effort, and nothing kept to try again, as the
    /// authority goes with its spaces. A syncer that misses it learns so
    /// when it next authorizes.
    pub fn tell_spaces_deleted(&self, authority: &str) -> Result<()> {
        let gone = syncer_update(&SyncerUpdate::SpaceDeleted);
        for (space, to) in self.store.registrations_of(authority)? {
            let (syncers, gone) = (self.syncers(), gone.clone());
            tokio::spawn(async move {
                let told = syncers.send(&space, &to, &gone).await;
                tracing::debug!(
                    space,
                    endpoint = to.endpoint,
                    ?told,
                    "told a syncer the space is gone"
                );
            });
        }
        Ok(())
    }

    /// Revokes the credentials `did` holds to `authority`'s spaces: at
    /// once for the repos here, and told to the host of each writer
    /// elsewhere in those spaces, best effort, as the authority.
    pub async fn revoke_credentials(&self, authority: &str, did: &str) -> Result<()> {
        let issued = self.store.issued_to(authority, did, now())?;
        self.revoke_issued(authority, issued).await
    }

    /// Whether the grant `id` still lasts, and its member reads `space`,
    /// at `uri`, now: what a delegation it was given buys with.
    pub fn grant_reads(&self, id: &str, uri: &str, space: &Space) -> Result<bool> {
        let Some(grant) = self.store.grant_by_id(id)? else {
            return Ok(false);
        };
        self.may(uri, space, Some(&grant.member), Access::Read)
    }

    /// Ends what the grants of apps signed in as `authority` bought of a
    /// space of its that they read no more, the grant gone or its member
    /// no longer reading there: their credentials revoked, as
    /// `revoke_credentials` does, and their syncers' registrations
    /// withdrawn, told nothing more.
    pub async fn end_grant_reads(&self, authority: &str) -> Result<()> {
        let mut asked = std::collections::BTreeMap::new();
        let mut gone = Vec::new();
        for (space, jti, expires_at, grant) in self.store.bought_of(authority, now())? {
            if !self.still_reads(&mut asked, &grant, &space)? {
                self.store.forget_bought(&space, &jti)?;
                gone.push((space, jti, expires_at));
            }
        }
        for (space, to, grant) in self.store.bound_registrations(authority)? {
            if !self.still_reads(&mut asked, &grant, &space)? {
                self.store.unregister(&space, &to)?;
            }
        }
        self.revoke_issued(authority, gone).await
    }

    /// `grant_reads` for the grant `grant` and the space at `uri`, asked
    /// once for each pair kept in `asked`.
    fn still_reads(
        &self,
        asked: &mut std::collections::BTreeMap<(String, String), bool>,
        grant: &str,
        uri: &str,
    ) -> Result<bool> {
        let key = (grant.to_string(), uri.to_string());
        if let Some(reads) = asked.get(&key) {
            return Ok(*reads);
        }
        // `bought_of` and `bound_registrations` name only spaces here.
        let space = self.store.space(uri)?.context("the space is here")?;
        let reads = self.grant_reads(grant, uri, &space)?;
        asked.insert(key, reads);
        Ok(reads)
    }

    /// Revokes the credentials `issued` to `authority`'s spaces, each its
    /// space, jti and lapse, as `revoke_credentials` says.
    async fn revoke_issued(
        &self,
        authority: &str,
        issued: Vec<(String, String, i64)>,
    ) -> Result<()> {
        let mut by_space: std::collections::BTreeMap<String, (Vec<String>, i64)> =
            Default::default();
        for (space, jti, expires_at) in issued {
            let (jtis, until) = by_space.entry(space).or_default();
            jtis.push(jti);
            *until = (*until).max(expires_at + 2 * space_auth::CLOCK_SKEW as i64);
        }
        let signing = self
            .store
            .account(authority)?
            .context("the authority is here")?
            .signing;
        for (space, (jtis, until)) in by_space {
            self.store.revoke(&space, &jtis, until, now())?;
            for writer in self.store.writers(&space, None, u32::MAX)? {
                let here = self
                    .store
                    .account(&writer.did)?
                    .is_some_and(|a| a.did == writer.did);
                if here {
                    continue;
                }
                let told = self
                    .tell_revoked(&signing, authority, &writer.did, &space, &jtis)
                    .await;
                if let Err(e) = told {
                    tracing::warn!(space, repo = writer.did, error = %format!("{e:#}"), "notifyCredentialsRevoked failed");
                }
            }
        }
        Ok(())
    }

    async fn tell_revoked(
        &self,
        signing: &atproto::Keypair,
        authority: &str,
        writer: &str,
        space: &str,
        jtis: &[String],
    ) -> Result<()> {
        let doc = self.identity.document(writer).await?;
        let claims = self.claims(authority, writer, NOTIFY_REVOKED);
        let pds = self.at_pds(&doc, NOTIFY_REVOKED).await;
        let (client, at) = pds.map_err(anyhow::Error::msg)?;
        let res = client
            .post(at)
            .bearer_auth(service_auth::sign(signing, &claims))
            .json(&json!({ "space": space, "credentials": jtis }))
            .send()
            .await?;
        res.error_for_status()?;
        Ok(())
    }

    /// Ends `authority`'s space at `space`: its state and the authority's
    /// own repo in it go, it is remembered as deleted, and every syncer
    /// registered for it, lapsed or not, is owed word of it.
    pub fn end_space(&self, authority: &str, space: &str) -> Result<()> {
        let (orphans, registered) = self.store.delete_space(space, authority, now())?;
        self.remove_blobs(orphans.iter().map(|cid| (authority, cid)));
        let (syncers, space) = (self.syncers(), space.to_string());
        tokio::spawn(async move {
            let deleted = SyncerUpdate::SpaceDeleted;
            let told = syncers.notify(&space, registered, &deleted, now()).await;
            tracing::debug!(space, ?told, "told the syncers the space is gone");
        });
        Ok(())
    }

    /// Tries the word owed to syncers due now. How much was tried.
    pub async fn retry_syncers_due(&self) -> Result<usize> {
        let now = now();
        let due = self.store.syncer_notices_due(now, ROUND)?;
        let syncers = self.syncers();
        // At most a round at once.
        let tried = join_all(due.iter().map(|notice| syncers.retry(notice, now)));
        tried.await.into_iter().collect::<Result<()>>()?;
        Ok(due.len())
    }

    /// Tries `notice` now: tells its space's authority, at its PDS, as the
    /// repo.
    pub async fn deliver(&self, notice: &Notice) -> Delivery {
        let Ok(space) = SpaceRef::parse(&notice.space) else {
            return Delivery::Failed;
        };
        let Ok(Some(account)) = self.store.account(&notice.repo) else {
            return Delivery::Failed;
        };
        let doc = match self.identity.kept(&space.authority) {
            Some(doc) => Ok(doc),
            None => self.identity.document(&space.authority).await,
        };
        // An authority not found, or naming no PDS yet, or none to be
        // asked ([`crate::guard`]), may be found later.
        let doc = doc.unwrap_or_default();
        let Ok((client, at)) = self.at_pds(&doc, NOTIFY_REPO_UPDATE).await else {
            return Delivery::Retry;
        };
        let aud = space_host_aud(&space.authority);
        let token = service_auth::sign(
            &account.signing,
            &self.claims(&account.did, &aud, NOTIFY_REPO_UPDATE),
        );
        let body = json!({
            "space": notice.space,
            "repo": notice.repo,
            "repoRev": notice.rev,
            "data": Value::Bytes(notice.data.clone()).to_json(),
        });
        let sent = client.post(at).bearer_auth(token).json(&body).send().await;
        delivery(sent)
    }

    /// Tries `notice`, and keeps what came of it: owed no more, or owed
    /// again after a wait.
    pub(crate) async fn try_notice(&self, notice: &Notice) -> Result<Delivery> {
        let delivery = self.deliver(notice).await;
        match delivery {
            Delivery::Retry => {
                let next = now() + retry_delay(notice.attempts + 1);
                self.store.retry(notice, next)?;
            }
            Delivery::Delivered => self.store.delivered(notice)?,
            Delivery::Failed => {
                tracing::warn!(
                    space = notice.space,
                    repo = notice.repo,
                    "the authority refused notifyRepoUpdate"
                );
                self.store.delivered(notice)?;
            }
        }
        Ok(delivery)
    }

    /// Tries the notices owed now, and gives up those a day old. How many
    /// were tried.
    pub async fn deliver_due(&self) -> Result<usize> {
        for (repo, space, rev) in self.store.lapse(now())? {
            tracing::warn!(repo, space, rev, "notifyRepoUpdate given up after a day");
        }
        let due = self.store.due(now(), ROUND)?;
        for notice in &due {
            self.try_notice(notice).await?;
        }
        Ok(due.len())
    }
}

/// Whether a refusal says the same request may land later.
fn retryable(status: StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 425 | 429) || status.is_server_error()
}
