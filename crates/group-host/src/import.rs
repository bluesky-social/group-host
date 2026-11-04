//! An account that already exists on the network, on any server and made
//! with any app, moved here to become a group. It keeps its DID, so
//! whoever follows it, and whatever points at it, still does.
//!
//! This is atproto's account migration, with the group made at the end.
//! The person moving it signs in to it where it lives and asks its server
//! for two things: a `createAccount` service token for this host, the
//! account's own word that it hands its DID over; and, once the copy is
//! made, a PLC operation naming this host, which its server signs when the
//! account's owner confirms it.
//!
//! - `checkImport`: what a move would make of it, before anything moves.
//! - `importAccount`: the token → the account here, switched off, with
//!   keys of its own, its repo and files copied and checked against its
//!   key; and what the PLC operation should name.
//! - `completeImport`: the signed operation → sent to the directory, the
//!   account switched on, and its group made, run by whoever moved it.
//!
//! Only a did:plc account: a did:web one is repointed on its own domain.
//! The old server keeps its copy until its owner deletes it there; once
//! the DID names this host, the network reads the account here.
//!
//! Its spaces come with it, by `importAccountSpace` (`import_space.rs`),
//! while the move is pending.

use crate::Host;
use crate::api::{ANSWER_AT_MOST, Params, REPO_AT_MOST, invalid, param, too_much};
use crate::group::NewGroup;
use crate::group_api::{for_host, signed_in};
use crate::import_space::keeps;
use crate::index_api::capped;
use crate::migration::{creds, repo_writes, take_in};
use crate::rbac::NS;
use crate::repo::{Asked, Write};
use crate::store::Store;
use crate::time::now_millis;
use crate::xrpc::XrpcError;
use anyhow::Result;
use atproto::plc::Signed;
use atproto::service_auth;
use atproto::{Cid, PublicKey, did};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use futures::StreamExt;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::fmt::Display;
use std::sync::Arc;

type Answer = Result<Json<Value>, XrpcError>;

/// The method an account's token for moving here is for.
const CREATE_ACCOUNT: &str = "com.atproto.server.createAccount";
/// The longest name and description a group's profile holds, in
/// characters: no more graphemes than its lexicon lets it have, and so no
/// more bytes.
const NAME_MAX: usize = 64;
const DESCRIPTION_MAX: usize = 300;

fn refused(error: &'static str, message: impl Into<String>) -> XrpcError {
    XrpcError::new(StatusCode::BAD_REQUEST, error, message)
}

/// The account's server, unreachable or refusing.
fn upstream(why: impl Display) -> XrpcError {
    XrpcError::new(
        StatusCode::BAD_GATEWAY,
        "UpstreamFailure",
        format!("the account's server: {why}"),
    )
}

fn not_plc() -> XrpcError {
    refused(
        "NotPlc",
        "only a did:plc account can move here: a did:web one is repointed on its own domain",
    )
}

/// An account's move here, started and not yet finished.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub founder: String,
    pub display_name: String,
    pub description: Option<String>,
}

impl Store {
    /// Keeps word that `founder` is moving `did` here.
    pub fn start_import(&self, did: &str, pending: &Pending, now: i64) -> Result<()> {
        let sql = "INSERT INTO account_import (did, founder, display_name, description, created_at)
                   VALUES (?1, ?2, ?3, ?4, ?5)";
        let p = pending;
        let row = params![did, p.founder, p.display_name, p.description, now];
        self.conn().execute(sql, row)?;
        Ok(())
    }

    /// `did`'s move here, if one is started and not finished.
    pub fn pending_import(&self, did: &str) -> Result<Option<Pending>> {
        let sql = "SELECT founder, display_name, description FROM account_import WHERE did = ?1";
        let found = self.conn().query_row(sql, [did], |r| {
            Ok(Pending {
                founder: r.get(0)?,
                display_name: r.get(1)?,
                description: r.get(2)?,
            })
        });
        Ok(found.optional()?)
    }

    /// `did`'s move here, finished.
    pub fn end_import(&self, did: &str) -> Result<()> {
        let sql = "DELETE FROM account_import WHERE did = ?1";
        self.conn().execute(sql, [did])?;
        Ok(())
    }

    /// The account whose address here is `address`.
    pub fn addressed(&self, address: &str) -> Result<Option<String>> {
        let sql = "SELECT did FROM address WHERE address = lower(?1)";
        let found = self.conn().query_row(sql, [address], |r| r.get(0));
        Ok(found.optional()?)
    }
}

/// A handle's first label as an address's label here: its letters,
/// digits and dashes, lowercased, each run of anything else a dash.
pub fn label_of(handle: &str) -> String {
    let first = handle.split('.').next().unwrap_or_default();
    let mut label = String::new();
    for c in first.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '-' {
            label.push(c);
        } else if !label.ends_with('-') {
            label.push('-');
        }
    }
    let label: String = label.trim_start_matches('-').chars().take(63).collect();
    label.trim_end_matches('-').to_string()
}

/// Whether `handle` is `domain` or under it, a leading dot or not.
fn under(handle: &str, domain: &str) -> bool {
    let domain = domain.trim_start_matches('.');
    handle == domain || handle.ends_with(&format!(".{domain}"))
}

/// What a move here would make of an account.
struct Plan {
    /// Its server, and the key its repo is signed with there.
    pds: String,
    key: PublicKey,
    /// The handle it has now.
    from: Option<String>,
    label: String,
    address: String,
    handle: String,
    keeps: bool,
    /// Why it cannot have that address, or that handle.
    refused: Option<XrpcError>,
}

impl Host {
    /// What moving `did` here would make of it: its handle kept if it is
    /// a domain of its own that still names it, otherwise its address,
    /// which takes its handle's first label, or `asked`'s.
    async fn plan_import(&self, did: &str, asked: Option<&str>) -> Result<Plan, XrpcError> {
        if !did.starts_with("did:plc:") {
            return Err(not_plc());
        }
        let doc = self.identity.document(did).await;
        let doc = doc.map_err(|e| invalid(format!("could not resolve {did}: {e:#}")))?;
        let (Ok(key), Some(pds)) = (did::signing_key(&doc), did::pds(&doc)) else {
            return Err(invalid(format!("{did} names no key, or no server")));
        };
        let from = did::handle(&doc);
        let label = label_of(asked.or(from.as_deref()).unwrap_or_default());
        let address = format!("{label}{}", self.config.handle_domain);
        let keeps = match &from {
            Some(h) => self.keeps_own(did, h, &pds).await,
            None => false,
        };
        let handle = match (&from, keeps) {
            (Some(h), true) => h.clone(),
            _ => address.clone(),
        };
        let refused = if label.is_empty() {
            Some(refused(
                "InvalidHandle",
                "an address of letters, digits and dashes",
            ))
        } else if self.held(&address, did)? || (keeps && self.held(&handle, did)?) {
            Some(refused(
                "HandleNotAvailable",
                format!("{address} or {handle} is taken on this host"),
            ))
        } else {
            None
        };
        Ok(Plan {
            pds,
            key,
            from,
            label,
            address,
            handle,
            keeps,
            refused,
        })
    }

    /// Whether `name`, a handle or an address, is another account's than
    /// `did`'s here: an earlier move of `did` holds nothing that counts.
    fn held(&self, name: &str, did: &str) -> Result<bool, XrpcError> {
        let by_handle = self.store.account(name).map_err(XrpcError::internal)?;
        let by_address = self.store.addressed(name).map_err(XrpcError::internal)?;
        let theirs = by_handle.map(|a| a.did).or(by_address);
        Ok(theirs.is_some_and(|d| d != did))
    }

    /// Whether `did` keeps `handle` here: a domain of its own, which
    /// neither this host nor its server at `pds` gives out (as
    /// bsky.social gives out its own), that still names it.
    async fn keeps_own(&self, did: &str, handle: &str, pds: &str) -> bool {
        if handle == "handle.invalid" || under(handle, &self.config.handle_domain) {
            return false;
        }
        let url = format!("{pds}/xrpc/com.atproto.server.describeServer");
        // A server that cannot say which domains it gives out may give out
        // this one: the handle is not kept on a guess.
        let Ok(described) = self.public_json(&url, "its server").await else {
            return false;
        };
        let domains = described["availableUserDomains"].as_array();
        let theirs = domains.into_iter().flatten().filter_map(Value::as_str);
        if theirs.into_iter().any(|d| under(handle, d)) {
            return false;
        }
        self.names(handle, did).await
    }

    /// GETs `url` of the account's server, with `query`.
    async fn ask_server(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<reqwest::Response, XrpcError> {
        let (pinned, at) = self.pinned(url).await.map_err(upstream)?;
        let res = pinned.get(at).query(query).send().await.map_err(upstream)?;
        if !res.status().is_success() {
            return Err(upstream(format!("{url} answered {}", res.status())));
        }
        Ok(res)
    }

    /// Every file `did` holds at `pds`, copied four at a time: how many
    /// came. One that does not, or is not what it says, is left out, as a
    /// file deleted there is.
    async fn copy_blobs(&self, did: &str, pds: &str) -> Result<usize, XrpcError> {
        let list = format!("{pds}/xrpc/com.atproto.sync.listBlobs");
        let mut cids: Vec<String> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut query = vec![("did", did), ("limit", "1000")];
            query.extend(cursor.as_deref().map(|c| ("cursor", c)));
            let res = self.ask_server(&list, &query).await?;
            let read = capped(res, ANSWER_AT_MOST).await;
            let read = read.ok_or(upstream(too_much(ANSWER_AT_MOST)))?;
            let page: Value = serde_json::from_slice(&read).map_err(upstream)?;
            let listed = page["cids"].as_array().cloned().unwrap_or_default();
            let next = page["cursor"].as_str().filter(|_| !listed.is_empty());
            cursor = next.map(String::from);
            cids.extend(listed.iter().filter_map(|c| c.as_str().map(String::from)));
            if cursor.is_none() {
                break;
            }
        }
        let get = format!("{pds}/xrpc/com.atproto.sync.getBlob");
        let copies: Vec<_> = cids
            .iter()
            .map(|cid| self.copy_blob(did, &get, cid))
            .collect();
        let copies = futures::stream::iter(copies).buffer_unordered(4);
        let came: Vec<bool> = copies.collect().await;
        Ok(came.into_iter().filter(|c| *c).count())
    }

    /// The file `cid` of `did`'s, from `url`, kept: whether it came, as
    /// what it says it is.
    async fn copy_blob(&self, did: &str, url: &str, cid: &str) -> bool {
        let came = async {
            let res = self.ask_server(url, &[("did", did), ("cid", cid)]).await?;
            let mime = res.headers().get(header::CONTENT_TYPE).cloned();
            let mime = mime.and_then(|m| m.to_str().ok().map(String::from));
            let most = self.config.blob_upload_limit as usize;
            let read = capped(res, most).await;
            let bytes = read.ok_or(upstream(too_much(most)))?;
            if Cid::of_raw(&bytes).to_string() != cid {
                return Err(upstream(format!("the file {cid} is not what it says")));
            }
            self.upload_blob(did, &bytes, &mime.unwrap_or_default())
        };
        let came = came.await;
        if let Err(e) = &came {
            tracing::warn!(did, cid, error = %e.message, "a file did not come");
        }
        came.is_ok()
    }

    /// `did`'s files and repo, copied from its server and checked against
    /// its key, into its account here, switched off: how many records and
    /// files, and its profile. A record this host would not keep (one not
    /// of its collection's type) is left out.
    async fn copy(&self, did: &str, plan: &Plan) -> Result<(usize, usize, Value), XrpcError> {
        // Its files first: its records hold them.
        let blobs = self.copy_blobs(did, &plan.pds).await?;
        let url = format!("{}/xrpc/com.atproto.sync.getRepo", plan.pds);
        let res = self.ask_server(&url, &[("did", did)]).await?;
        let read = capped(res, REPO_AT_MOST).await;
        let bytes = read.ok_or(upstream(too_much(REPO_AT_MOST)))?;
        let Some(writes) = repo_writes(&bytes, did, &plan.key) else {
            return Err(refused(
                "InvalidRepo",
                "its repo is not the account's, signed and whole",
            ));
        };
        let total = writes.len();
        let writes: Vec<Write> = writes.into_iter().filter(keeps).collect();
        let left = total - writes.len();
        tracing::info!(did, left, "records this host would not keep, left out");
        let profile = writes.iter().find_map(|w| match w {
            Write::Create {
                collection, value, ..
            } if collection == "app.bsky.actor.profile" && w.path().1 == "self" => {
                Some(value.clone())
            }
            _ => None,
        });
        let copied = Asked {
            off: true,
            copied: true,
            ..Asked::default()
        };
        if !writes.is_empty() {
            self.write_asked(did, &writes, &copied)?;
        }
        Ok((writes.len(), blobs, profile.unwrap_or_default()))
    }
}

/// The account a token is the word of, for moving it here: its own
/// token, signed with its key, for this host and `createAccount`.
async fn proof(host: &Host, jwt: &str) -> Result<String, XrpcError> {
    let bad = |why: &str| refused("InvalidToken", format!("not the account's token: {why}"));
    let (_, claims) = service_auth::parts(jwt).map_err(|e| bad(&e.0))?;
    let did = claims.account().to_string();
    if !did.starts_with("did:plc:") {
        return Err(not_plc());
    }
    let doc = host.identity.document(&did).await;
    let doc = doc.map_err(|e| invalid(format!("could not resolve {did}: {e:#}")))?;
    let key = did::signing_key(&doc).map_err(|_| bad("its DID names no key"))?;
    let now = (now_millis() / 1000) as u64;
    let claims = service_auth::verify(jwt, &key, now).map_err(|e| bad(&e.0))?;
    if !for_host(host, &claims.aud) || claims.lxm.as_deref() != Some(CREATE_ACCOUNT) {
        return Err(bad("not for this host's createAccount"));
    }
    Ok(did)
}

/// What moving an account here would make of it, before anything moves.
pub async fn check_import(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
    headers: HeaderMap,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.checkImport")).await?;
    let did = param(&p, "did")?;
    let plan = host
        .plan_import(did, p.get("address").map(String::as_str))
        .await?;
    let mut out = json!({
        "did": did,
        "handle": plan.handle,
        "address": plan.address,
        "label": plan.label,
        "keeps": plan.keeps,
        "pds": plan.pds,
    });
    // What its operation should name, again, for whoever is moving it.
    let pending = host
        .store
        .pending_import(did)
        .map_err(XrpcError::internal)?;
    if pending.is_some_and(|p| p.founder == me) {
        let account = host.store.account(did).map_err(XrpcError::internal)?;
        out["recommendedCreds"] = creds(&host, &account.expect("a move's account"));
    }
    if let Some(from) = &plan.from {
        out["from"] = json!(from);
    }
    if let Some(e) = &plan.refused {
        out["refused"] = json!({"error": e.error, "message": e.message});
    }
    Ok(Json(out))
}

/// Starts moving an account here: its token checked, the account made
/// here switched off with keys of its own, its repo and files copied.
/// Started again by whoever started it, what the first start copied goes.
pub async fn import_account(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.importAccount")).await?;
    let jwt = body["serviceJwt"].as_str();
    let jwt = jwt.expect("its lexicon requires it");
    let started = host.import_as(&me, jwt, body["address"].as_str()).await?;
    Ok(Json(started))
}

impl Host {
    /// Starts moving the account `jwt` is the word of here, for `me`, at
    /// `address` if asked: what importAccount answers.
    pub(crate) async fn import_as(
        &self,
        me: &str,
        jwt: &str,
        address: Option<&str>,
    ) -> Result<Value, XrpcError> {
        self.founding(me)?;
        let did = proof(self, jwt).await?;
        if did == me {
            return Err(refused(
                "SameAccount",
                "sign in with your own account, not the one becoming the group",
            ));
        }
        let started = self.store.pending_import(&did);
        let here = self.store.account(&did).map_err(XrpcError::internal)?;
        match started.map_err(XrpcError::internal)? {
            Some(p) if p.founder != me => {
                return Err(refused(
                    "AlreadyHere",
                    "someone else is moving this account here",
                ));
            }
            Some(_) => self.forget(&did),
            None if here.is_some() => {
                return Err(refused("AlreadyHere", "this host has that account already"));
            }
            None => {}
        }
        let plan = self.plan_import(&did, address).await?;
        if let Some(e) = plan.refused {
            return Err(e);
        }
        let account = take_in(self, &did, &plan.handle, &plan.address).await?;
        let off = self.store.deactivate(&did, now_millis(), me, &[], &[]);
        off.map_err(XrpcError::internal)?;
        let (records, blobs, profile) = match self.copy(&did, &plan).await {
            Ok(copied) => copied,
            Err(e) => {
                // Nothing half copied stays.
                self.forget(&did);
                return Err(e);
            }
        };
        let named = profile["displayName"].as_str().map(str::trim);
        let display_name = named.filter(|n| !n.is_empty()).unwrap_or(&plan.label);
        // As long as a group's name may be, as its description is cut.
        let display_name: String = display_name.chars().take(NAME_MAX).collect();
        let description = profile["description"].as_str();
        let description = description.map(|d| d.chars().take(DESCRIPTION_MAX).collect());
        let pending = Pending {
            founder: me.to_string(),
            display_name,
            description,
        };
        let kept = self.store.start_import(&did, &pending, now_millis());
        kept.map_err(XrpcError::internal)?;
        tracing::info!(did, records, blobs, "an account copied here, switched off");
        Ok(json!({
            "did": did,
            "handle": plan.handle,
            "address": plan.address,
            "displayName": pending.display_name,
            "records": records,
            "blobs": blobs,
            "recommendedCreds": creds(self, &account),
        }))
    }
}

/// Whether `signed` gives `account` to this host, as importAccount said
/// it should: this host its server and a key to its DID, the keys made
/// for it here, and its handle.
fn gives_here(host: &Host, account: &crate::store::Account, signed: &Signed) -> bool {
    let Signed::Update { op, .. } = signed else {
        return false;
    };
    let pds = op.services.get("atproto_pds").map(|s| s.endpoint.as_str());
    let methods = &op.verification_methods;
    let key = |name: &str| methods.get(name).map(String::as_str);
    pds == Some(host.config.public_url.as_str())
        && key("atproto") == Some(&account.signing.public().did_key())
        && key("atproto_kem") == Some(&account.kem.public().did_key())
        && op
            .rotation_keys
            .contains(&host.secrets.rotation.public().did_key())
        && op
            .also_known_as
            .contains(&format!("at://{}", account.handle))
}

/// Finishes moving an account here: the operation its old server signed
/// sent to the directory, the account switched on, and its group made,
/// run by whoever moved it.
pub async fn complete_import(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Answer {
    let me = signed_in(&host, &headers, &format!("{NS}.completeImport")).await?;
    let did = body["did"].as_str();
    let did = did.expect("its lexicon requires it");
    let done = host
        .complete_import_as(&me, did, &body["operation"])
        .await?;
    Ok(Json(done))
}

impl Host {
    /// Finishes `me`'s move of `did` with `operation`, its old server's
    /// signature: what completeImport answers.
    pub(crate) async fn complete_import_as(
        &self,
        me: &str,
        did: &str,
        operation: &Value,
    ) -> Result<Value, XrpcError> {
        let pending = self
            .store
            .pending_import(did)
            .map_err(XrpcError::internal)?;
        let Some(pending) = pending.filter(|p| p.founder == me) else {
            return Err(refused("NotFound", "no move of that account here of yours"));
        };
        let account = self.store.account(did).map_err(XrpcError::internal)?;
        let account = account.expect("a move's account");
        let signed = Signed::from_json(operation).ok();
        let Some(signed) = signed.filter(|s| gives_here(self, &account, s)) else {
            return Err(refused(
                "InvalidOperation",
                "the signed operation does not give the account to this host",
            ));
        };
        // The directory may have it already: sent by the account's server, or
        // by a try of this that failed after sending it. It is not sent again.
        let last = self.plc.last_op(did).await.ok().map(|op| op.cid());
        let sent = match last == Some(signed.cid()) {
            true => Ok(()),
            false => self.plc.publish(did, &signed).await,
        };
        sent.map_err(|e| {
            refused(
                "InvalidOperation",
                format!("the directory refused it: {e:#}"),
            )
        })?;
        let on = self.activate_arrived(did, &account.handle);
        on.map_err(XrpcError::internal)?;
        self.tell_spaces(did).await.map_err(XrpcError::internal)?;
        let group = NewGroup {
            handle: account.handle.clone(),
            founder: me.to_string(),
            display_name: pending.display_name,
            description: pending.description,
            ..NewGroup::default()
        };
        let made = self.make_group(did, &group).await?;
        self.store.end_import(did).map_err(XrpcError::internal)?;
        let logged = self.record(did, me, None, "moved the account here as a group");
        logged.map_err(XrpcError::internal)?;
        let address = self.store.address(did).map_err(XrpcError::internal)?;
        Ok(json!({
            "did": did,
            "handle": account.handle,
            "address": address,
            "meta": made.meta,
            "members": made.members,
        }))
    }
}
