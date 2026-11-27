//! A client for Atmosphere Groups hosts, and what the `atgrp` command
//! does with it. Each command is `atgrp <group> <action>`; all but
//! those that make, move or delete a group or an account are the twins of the
//! host's MCP tools, made from the same `group_host_mcp::Tool`s and
//! carried out from the same `Plan`s.
//!
//! The command acts as a person: it signs in to their own PDS with their
//! handle and password (an app password is enough), and asks that PDS for
//! a service token for each method it calls on a host, as an app does. So
//! an account on any PDS may run the command against any host. To write as
//! a group, as an invite, a role or a membership is written, it asks the
//! host for a group token (`getGroupAuth`), bound to a DPoP key of its own.

use anyhow::{Context, Result, anyhow, bail};
use atproto::keys::{Curve, Keypair};
use atproto::space::{SPACE_URI_PREFIX, SpaceRecordUri, SpaceRef};
use atproto::{did, dpop};
use group_host_mcp::{Plan, Tool};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

/// What `atgrp` prints for `--help`, or with no command.
pub const USAGE: &str = "usage: atgrp <group> <action> <arguments>

group
  group create <handle> [--display-name <name>] [--description <words>] [--join-policy open|approval|invite]
                         start a group, run by you
  group show <group>     a group as you may see it: its roles, members and spaces
  group seed <handle>    a group to try things on: two rules, a moderator role, and an
                         invite for each handle in ATGRP_SEED_INVITE
  group move <group> --to <address> [--to-did <did>]
                         move a group, whole, from this host to the one at <address>
  group deactivate <group> [--undo]
                         switch a group off, so nothing reads or writes it, or with
                         --undo switch it back on
  group handle <group> <handle>
                         give a group a handle on a domain of its own, once the domain
                         names it, or its own address back
  group domain <group> [--undo]
                         serve the group at its own domain, its handle, rather than its
                         address, once that domain reaches this host; or with --undo, at
                         its address again
  group delete <group>   delete a group, its spaces and its identity, for good
profile
  profile set <group> [--display-name <name>] [--description <words>] [--website <url>]
          [--avatar <blob json>] [--join-policy open|approval|invite]
                         change the group's about record in what is given, \"\" taking a
                         field away; --join-policy changes its config
rule
  rule set <group> --title <title> --text <words> [--order <n>] [--rule <key>]
                         write a rule: the one --rule names, or a new one
  rule delete <group> <rule>
                         delete a rule
label
  label define <group> <label> --name <name> --description <words> [--severity <s>] …
                         define one of the group's own labels, or change one it defines
  label delete <group> <label>
                         stop defining one of the group's own labels
role
  role set <group> <role> [--display-name <name>] [--description <words>] [--default] [--bot]
          [--permissions <permission,…>] [--can-assign <role,…>]
                         write a role, with what it may do across the group and the
                         roles its holders may give (* for every role)
  role delete <group> <role>
                         delete a role nobody holds
  role grant <group> <role> [--permissions <permission,…>] [--can-assign <role,…>] [--undo]
                         add to what a role may do, or with --undo take it away
member
  member list <group> [--q <text>] [--limit <n>] [--cursor <cursor>]
                         who is in the group, and their roles, a page at a time
  member assign <group> <member> <role,role…>
                         give a member these roles, in place of theirs, or let them in
  member eject <group> <member>
                         put a member out
join
  join list <group> [--limit <n>] [--cursor <cursor>]
                         the requests to join waiting
  join deny <group> <member>
                         turn down a request to join
invite
  invite send <group> <person> [--message <words>] [--expires-at <time>] [--undo]
                         invite someone in, or with --undo take the invite back
  invite list <group> [--limit <n>] [--cursor <cursor>]
                         the invites waiting
space
  space create <group> <type> --policy <json> [--skey <key>]
                         add a space to the group, under the policy given
  space records <group> <space> [--collection <nsid>] [--field <path> --equals <value>] …
                         the records in one of the group's spaces
mod
  mod list <group> [--status open|resolved] [--tags <tag,…>] [--sort oldest|newest|updated]
          [--limit <n>] [--cursor <cursor>]
                         the moderation queue: every subject, or those of a status or
                         with any of the tags, oldest first by default
  mod history <group> <subject> [--cid <cid>] [--sort newest|oldest] [--limit <n>] [--cursor <cursor>]
                         what was done about a subject, newest first by default
  mod update <group> <subject> [--status open|resolved] [--note <words>] [--add-tags <tag,…>]
          [--remove-tags <tag,…>] [--cid <cid>]
                         resolve or reopen a subject, tag it, or add a note
  mod label <group> <subject> <val> [--cid <cid>] [--undo]
                         label a subject, or with --undo take the label back
admin
  admin logs [--trace <id>] [--message <message>] [--level <level>] [--min-ms <n>] [--limit <n>]
                         the lines the host logged, oldest first: one request's by the
                         x-trace-id it was answered with, or the last of a message, a
                         level or a duration; for the host's operators
  admin stats [--days <n>]
                         how the host is used, day by day, and what it holds; for the
                         host's operators
account
  account import <account> [--address <label>]
                         move an account from its server here, its spaces with it, to
                         become a group run by you; its server emails a code
  account import <account> --code <code>
                         with that code, finish moving it

  A group or person is a handle or a DID; a subject an account's DID or a
  record's URI. A flag is a field of what the command writes or asks,
  in kebab case; a list is comma-separated, and anything more than text or
  a number is JSON. Every command but group create, seed, move and
  delete, account import and admin is also the MCP tool <group>_<action>
  (member_assign), which an MCP client connected to a group calls there.

  You sign in as ATGRP_AS (a handle) with ATGRP_PASSWORD, at
  PDS_URL; the host is ATGRP_HOST, its DID ATGRP_HOST_DID (by
  default the did:web of its address). An account you import signs in at
  its own server with ATGRP_IMPORT_PASSWORD, its own password: its
  server hands its identity over for no app password.
";

/// Where the command acts, and as whom: from the environment.
#[derive(Debug, Clone)]
pub struct Env {
    /// The host, `ATGRP_HOST`.
    pub host: String,
    /// Its DID, `ATGRP_HOST_DID`: by default the did:web of its address.
    pub host_did: Option<String>,
    /// The PDS the person signs in at, `PDS_URL`.
    pub pds: String,
    /// The person, `ATGRP_AS`, and their password,
    /// `ATGRP_PASSWORD`.
    pub handle: Option<String>,
    pub password: Option<String>,
    /// Who `seed` invites, `ATGRP_SEED_INVITE`, comma-separated.
    pub seed_invite: Vec<String>,
    /// The password of an account `import` moves, at its own server,
    /// `ATGRP_IMPORT_PASSWORD`.
    pub import_password: Option<String>,
}

/// The did:web a host at `url` is by default, as the host names itself.
pub fn did_web(url: &str) -> Result<String> {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    Ok(did::web(host)?)
}

impl Env {
    /// The environment `var` reads.
    pub fn from(var: impl Fn(&str) -> Option<String>) -> Env {
        let host = var("ATGRP_HOST").unwrap_or_else(|| "http://localhost:2600".into());
        let host = host.trim_end_matches('/').to_string();
        let host_did = var("ATGRP_HOST_DID");
        let pds = var("PDS_URL").unwrap_or_else(|| "http://localhost:2583".into());
        let invite = var("ATGRP_SEED_INVITE").unwrap_or_default();
        Env {
            host,
            host_did,
            pds: pds.trim_end_matches('/').to_string(),
            handle: var("ATGRP_AS"),
            password: var("ATGRP_PASSWORD"),
            import_password: var("ATGRP_IMPORT_PASSWORD"),
            seed_invite: invite
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
        }
    }
}

/// A host, by its address and DID.
#[derive(Debug, Clone)]
pub struct Host {
    pub url: String,
    pub did: String,
}

/// A person signed in at their PDS.
pub struct Client {
    http: reqwest::Client,
    pds: String,
    access: String,
    /// Who they are.
    pub did: String,
}

/// A body's JSON, or why the call failed: its status and what it said.
async fn answer(res: reqwest::Response, what: &str) -> Result<Value> {
    let status = res.status();
    let body: Value = res.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let error = body["error"].as_str().unwrap_or("Error");
        let message = body["message"].as_str().unwrap_or_default();
        bail!("{what}: {status} {error} {message}");
    }
    Ok(body)
}

impl Client {
    /// Signs `handle` in at `pds` with `password`.
    pub async fn sign_in(pds: &str, handle: &str, password: &str) -> Result<Client> {
        let http = reqwest::Client::new();
        let sent = http
            .post(format!("{pds}/xrpc/com.atproto.server.createSession"))
            .json(&json!({"identifier": handle, "password": password}))
            .send()
            .await;
        let res = sent.with_context(|| format!("reaching {pds}"))?;
        let session = answer(res, &format!("signing in as {handle}")).await?;
        let field = |k: &str| text(&session[k]).to_string();
        Ok(Client {
            http,
            pds: pds.into(),
            access: field("accessJwt"),
            did: field("did"),
        })
    }

    /// A service token from the person's PDS, for `aud`'s `lxm`.
    async fn token(&self, aud: &str, lxm: &str) -> Result<String> {
        let url = format!("{}/xrpc/com.atproto.server.getServiceAuth", self.pds);
        let asked = self.http.get(url).query(&[("aud", aud), ("lxm", lxm)]);
        let res = asked.bearer_auth(&self.access).send().await?;
        let said = answer(res, &format!("a token for {lxm}")).await?;
        Ok(text(&said["token"]).to_string())
    }

    /// `host`'s procedure `lxm`, with `body`.
    pub async fn call(&self, host: &Host, lxm: &str, body: &Value) -> Result<Value> {
        let token = self.token(&host.did, lxm).await?;
        let url = format!("{}/xrpc/{lxm}", host.url);
        let sent = self.http.post(url).bearer_auth(token).json(body).send();
        let res = sent
            .await
            .with_context(|| format!("reaching {}", host.url))?;
        answer(res, lxm).await
    }

    /// `host`'s query `lxm`, with `params`.
    pub async fn query(&self, host: &Host, lxm: &str, params: &[(&str, &str)]) -> Result<Value> {
        let token = self.token(&host.did, lxm).await?;
        let url = format!("{}/xrpc/{lxm}", host.url);
        let sent = self.http.get(url).bearer_auth(token).query(params).send();
        let res = sent
            .await
            .with_context(|| format!("reaching {}", host.url))?;
        answer(res, lxm).await
    }

    /// `nsid` of the person's own PDS, as them: a query with `params`, or
    /// with `body`, a procedure (one that takes none with a null body).
    async fn own(
        &self,
        nsid: &str,
        params: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value> {
        let url = format!("{}/xrpc/{nsid}", self.pds);
        let asked = match body {
            Some(Value::Null) => self.http.post(url),
            Some(body) => self.http.post(url).json(body),
            None => self.http.get(url),
        };
        let sent = asked.query(params).bearer_auth(&self.access).send().await;
        let res = sent.context(format!("reaching {}", self.pds))?;
        answer(res, nsid).await
    }

    /// The bytes `nsid` of the person's own PDS answers with `params`, and
    /// their type; none where it holds no repo of theirs.
    async fn own_bytes(
        &self,
        nsid: &str,
        params: &[(&str, &str)],
    ) -> Result<Option<(Vec<u8>, String)>> {
        let url = format!("{}/xrpc/{nsid}", self.pds);
        let asked = self.http.get(url).query(params).bearer_auth(&self.access);
        let res = asked.send().await;
        let res = res.context(format!("reaching {}", self.pds))?;
        let kind = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        let kind = kind.unwrap_or("application/octet-stream").to_string();
        if !res.status().is_success() {
            let said = answer(res, nsid).await.expect_err("a refusal");
            let none = format!("{said}").contains("RepoNotFound");
            return none.then_some(None).ok_or(said);
        }
        Ok(Some((res.bytes().await?.to_vec(), kind)))
    }

    /// `host`'s procedure `lxm` with `params`, taking `bytes` of `kind`.
    async fn send(
        &self,
        host: &Host,
        lxm: &str,
        params: &[(&str, &str)],
        bytes: Vec<u8>,
        kind: &str,
    ) -> Result<Value> {
        let token = self.token(&host.did, lxm).await?;
        let url = format!("{}/xrpc/{lxm}", host.url);
        let asked = self.http.post(url).bearer_auth(token).query(params);
        let sent = asked.header("content-type", kind).body(bytes).send().await;
        let res = sent.context(format!("reaching {}", host.url))?;
        answer(res, lxm).await
    }

    /// Invites `invitee` to `group`, as the group: with a group token for
    /// their inbox, from `getGroupAuth`, `record` written there through
    /// `com.atproto.space.createRecord`.
    pub async fn invite(
        &self,
        host: &Host,
        group: &str,
        invitee: &str,
        record: Value,
    ) -> Result<Value> {
        let inbox = inbox_of(invitee);
        let session = GroupSession::start(self, host, group, &inbox).await?;
        let body = json!({"repo": group, "space": inbox, "collection": "org.atgrp.invite", "record": record});
        session.call("com.atproto.space.createRecord", &body).await
    }

    /// The DID `who` names, a handle or a DID, as `host` resolves it.
    pub async fn resolve(&self, host: &Host, who: &str) -> Result<String> {
        if who.starts_with("did:") {
            return Ok(who.to_string());
        }
        let lxm = "com.atproto.identity.resolveHandle";
        let said = self.query(host, lxm, &[("handle", who)]).await?;
        Ok(text(&said["did"]).to_string())
    }
}

/// The URI of `authority`'s space of `kind` at `skey`.
fn space_uri(authority: &str, kind: &str, skey: &str) -> String {
    let (authority, kind, skey) = (authority.into(), kind.into(), skey.into());
    SpaceRef {
        authority,
        kind,
        skey,
    }
    .to_string()
}

/// `person`'s inbox, where a group writes an invite for them.
fn inbox_of(person: &str) -> String {
    space_uri(person, "org.atgrp.inbox", "self")
}

/// Now, as atproto writes a datetime.
fn now_iso() -> String {
    atproto::time::iso(atproto::time::now_millis())
}

const GET_GROUP_AUTH: &str = "org.atgrp.internal.getGroupAuth";

/// A session as a group, from `getGroupAuth`: its token, bound to a DPoP
/// key of its own, and the host's nonce as it last gave it.
struct GroupSession<'a> {
    client: &'a Client,
    host: &'a Host,
    /// The host's address as it names itself, which a proof names.
    public: String,
    key: Keypair,
    nonce: Mutex<Option<String>>,
    token: String,
}

impl<'a> GroupSession<'a> {
    /// A group token for `space`, as far as the person's roles in `group`
    /// let them write there as it.
    async fn start(
        client: &'a Client,
        host: &'a Host,
        group: &str,
        space: &str,
    ) -> Result<GroupSession<'a>> {
        let url = format!("{}/.well-known/oauth-protected-resource", host.url);
        let res = client.http.get(url).send().await;
        let res = res.context(format!("reaching {}", host.url))?;
        let resource = answer(res, "the host's metadata").await?;
        let mut session = GroupSession {
            client,
            host,
            public: text(&resource["resource"]).to_string(),
            key: Keypair::generate(Curve::P256),
            nonce: Mutex::new(None),
            token: String::new(),
        };
        let service = client.token(&host.did, GET_GROUP_AUTH).await?;
        let body = json!({"group": group, "space": space});
        let auth = format!("Bearer {service}");
        let said = session.send(GET_GROUP_AUTH, &auth, None, &body).await?;
        session.token = text(&said["accessJwt"]).to_string();
        Ok(session)
    }

    /// A DPoP proof of a POST to `nsid`, for `token` if it carries one.
    fn proof(&self, nsid: &str, token: Option<&str>) -> String {
        let url = format!("{}/xrpc/{nsid}", self.public);
        let now = atproto::time::now_millis() as u64 / 1000;
        let mut claims = dpop::Claims::new("POST", &url, now);
        claims.nonce = self.nonce.lock().expect("not poisoned").clone();
        if let Some(token) = token {
            claims = claims.token(token);
        }
        dpop::sign(&self.key, &claims)
    }

    /// POSTs `body` to `nsid` with `authorization` and a fresh proof, once
    /// more with the host's nonce if it asks for one.
    async fn send(
        &self,
        nsid: &str,
        authorization: &str,
        token: Option<&str>,
        body: &Value,
    ) -> Result<Value> {
        let mut res = self.post(nsid, authorization, token, body).await?;
        let stale = res
            .headers()
            .get("www-authenticate")
            .is_some_and(|v| v.to_str().unwrap_or_default().contains("use_dpop_nonce"));
        if stale {
            res = self.post(nsid, authorization, token, body).await?;
        }
        answer(res, nsid).await
    }

    async fn post(
        &self,
        nsid: &str,
        authorization: &str,
        token: Option<&str>,
        body: &Value,
    ) -> Result<reqwest::Response> {
        let url = format!("{}/xrpc/{nsid}", self.host.url);
        let asked = self.client.http.post(url).json(body);
        let asked = asked.header("authorization", authorization);
        let sent = asked.header("dpop", self.proof(nsid, token)).send().await;
        let res = sent.context(format!("reaching {}", self.host.url))?;
        if let Some(nonce) = res.headers().get("dpop-nonce") {
            let nonce = nonce.to_str().unwrap_or_default().to_string();
            *self.nonce.lock().expect("not poisoned") = Some(nonce);
        }
        Ok(res)
    }

    /// The procedure `nsid` with `body`, as the group.
    async fn call(&self, nsid: &str, body: &Value) -> Result<Value> {
        let auth = format!("DPoP {}", self.token);
        self.send(nsid, &auth, Some(&self.token), body).await
    }
}

/// The arguments after a command: its words, and its `--flag value`s.
struct Args<'a> {
    words: Vec<&'a str>,
    flags: HashMap<&'a str, &'a str>,
}

fn parse<'a>(args: &[&'a str], known: &[&str]) -> Result<Args<'a>> {
    let mut words = Vec::new();
    let mut flags = HashMap::new();
    let mut it = args.iter().copied();
    while let Some(a) = it.next() {
        match a.strip_prefix("--") {
            Some(flag) if known.contains(&flag) => {
                let value = it.next().ok_or_else(|| anyhow!("--{flag} wants a value"))?;
                flags.insert(flag, value);
            }
            Some(flag) => bail!("no option --{flag}\n\n{USAGE}"),
            None => words.push(a),
        }
    }
    Ok(Args { words, flags })
}

fn usage() -> anyhow::Error {
    anyhow!("{USAGE}")
}

impl<'a> Args<'a> {
    /// Exactly `N` words, or the usage.
    fn take<const N: usize>(&self) -> Result<[&'a str; N]> {
        <[&str; N]>::try_from(self.words.as_slice()).map_err(|_| usage())
    }
}

/// `value`, given for `name`, as the tool's `schema` for it has it: a
/// number, a list (`a,b`), JSON for anything more than text, or text.
fn typed(name: &str, value: &str, schema: &Value) -> Result<Value> {
    let listed = schema["type"] == "array" && schema["items"]["type"] == "string";
    Ok(match schema["type"].as_str() {
        Some("integer") => {
            let n: i64 = value
                .parse()
                .map_err(|_| anyhow!("{name} wants a number"))?;
            json!(n)
        }
        Some("string") => json!(value),
        _ if listed => {
            let items = value.split(',').map(str::trim).filter(|v| !v.is_empty());
            json!(items.collect::<Vec<_>>())
        }
        _ if value.is_empty() => Value::Null,
        _ => serde_json::from_str(value).map_err(|_| anyhow!("{name} wants JSON"))?,
    })
}

/// A tool's command's words after `<group> <action>`, as the tool's
/// arguments: the group first, then the arguments the tool names in
/// order, then flags, each `--name value` for its `name` in kebab case,
/// a yes or no `--name` or `--no-name`, typed as the tool's schema has
/// it. The group, and the arguments.
pub fn arguments(tool: &Tool, words: &[&str]) -> Result<(String, Value)> {
    let properties = &tool.input_schema["properties"];
    let mut given = json!({});
    let mut positional = Vec::new();
    let mut it = words.iter().copied();
    while let Some(word) = it.next() {
        let Some(flag) = word.strip_prefix("--") else {
            positional.push(word);
            continue;
        };
        let name = flag.replace('-', "_");
        let no = name
            .strip_prefix("no_")
            .filter(|n| properties[*n]["type"] == "boolean");
        let schema = &properties[no.unwrap_or(&name)];
        if schema.is_null() || tool.args.contains(&name) {
            bail!("no option --{flag}\n\n{USAGE}");
        }
        let value = match (no, schema["type"] == "boolean") {
            (Some(n), _) => {
                given[n] = json!(false);
                continue;
            }
            (None, true) => json!(true),
            (None, false) => {
                let value = it.next().ok_or_else(|| anyhow!("--{flag} wants a value"))?;
                typed(&format!("--{flag}"), value, schema)?
            }
        };
        given[name] = value;
    }
    let (group, rest) = positional.split_first().ok_or_else(usage)?;
    if rest.len() != tool.args.len() {
        return Err(usage());
    }
    for (name, value) in tool.args.iter().zip(rest) {
        given[name] = typed(name, value, &properties[name])?;
    }
    Ok((group.to_string(), given))
}

/// Signs in as the environment's person.
async fn signed_in(env: &Env) -> Result<Client> {
    let (Some(handle), Some(password)) = (&env.handle, &env.password) else {
        bail!("set ATGRP_AS and ATGRP_PASSWORD to sign in");
    };
    Client::sign_in(&env.pds, handle, password).await
}

/// Runs the command `args` name (without the program's own name), in
/// `env`, returning what it prints.
pub async fn run(args: &[String], env: &Env) -> Result<String> {
    let did = env
        .host_did
        .clone()
        .map_or_else(|| did_web(&env.host), Ok)?;
    let host = Host {
        url: env.host.clone(),
        did,
    };
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["help" | "--help" | "-h"] => Ok(USAGE.to_string()),
        ["group", "create", rest @ ..] => {
            let a = parse(rest, &["display-name", "description", "join-policy"])?;
            let [handle] = a.take()?;
            create_group(&signed_in(env).await?, &host, handle, &a.flags).await
        }
        ["group", "seed", rest @ ..] => {
            let [handle] = parse(rest, &[])?.take()?;
            seed(&signed_in(env).await?, &host, handle, &env.seed_invite).await
        }
        ["group", "move", rest @ ..] => {
            let a = parse(rest, &["to", "to-did"])?;
            let [group] = a.take()?;
            let to = a.flags.get("to").ok_or_else(usage)?.trim_end_matches('/');
            let did = a.flags.get("to-did").map(|d| d.to_string());
            let did = did.map_or_else(|| did_web(to), Ok)?;
            let dest = Host {
                url: to.to_string(),
                did,
            };
            migrate(&signed_in(env).await?, &host, &dest, group).await
        }
        ["group", "delete", rest @ ..] => {
            let [group] = parse(rest, &[])?.take()?;
            delete(&signed_in(env).await?, &host, group).await
        }
        ["account", "import", rest @ ..] => {
            let a = parse(rest, &["address", "code"])?;
            let [account] = a.take()?;
            import(&signed_in(env).await?, &host, env, account, &a).await
        }
        ["admin", "logs", rest @ ..] => {
            let a = parse(rest, &["trace", "message", "level", "min-ms", "limit"])?;
            let [] = a.take()?;
            let asked: Vec<(&str, &str)> = a
                .flags
                .iter()
                .map(|(flag, value)| (if *flag == "min-ms" { "minMs" } else { flag }, *value))
                .collect();
            let me = signed_in(env).await?;
            let said = me.query(&host, &internal("getLogs"), &asked).await?;
            Ok(log_lines(&said))
        }
        ["admin", "stats", rest @ ..] => {
            let a = parse(rest, &["days"])?;
            let [] = a.take()?;
            let asked: Vec<(&str, &str)> = a.flags.iter().map(|(k, v)| (*k, *v)).collect();
            let me = signed_in(env).await?;
            let said = me.query(&host, &internal("getStats"), &asked).await?;
            Ok(stats_lines(&said))
        }
        [group, action, rest @ ..] => {
            let name = format!("{group}_{action}").replace('-', "_");
            let tools = group_host_mcp::tools();
            let Some(tool) = tools.into_iter().find(|t| t.name == name) else {
                bail!("no command {group} {action}\n\n{USAGE}");
            };
            let (group, given) = arguments(&tool, rest)?;
            command(&signed_in(env).await?, &host, &tool, &group, given).await
        }
        [other] => bail!("no command {other}\n\n{USAGE}"),
    }
}

/// The lines `getLogs` answered, each as the host logged it, one to a
/// line.
fn log_lines(said: &Value) -> String {
    rows(&said["lines"], |l| l["line"].to_string())
}

/// A line for each item of `list`, as `f` writes it.
fn rows(list: &Value, f: impl Fn(&Value) -> String) -> String {
    let items = list.as_array().into_iter().flatten();
    items.map(|i| format!("{}\n", f(i))).collect()
}

/// What `getStats` answered, as lines: what the host holds, a row for
/// each day, then each route and tool.
fn stats_lines(said: &Value) -> String {
    let h = &said["host"];
    let mut out = format!(
        "host\t{} groups, {} switched off, {} spaces, {} bytes of files\n",
        h["groups"], h["deactivated"], h["spaces"], h["blobBytes"]
    );
    out.push_str("day\trequests\trefused\tfailed\tslow\ttool calls\tgroups made\tspaces made\n");
    out.push_str(&rows(&said["days"], |d| {
        let fields = [
            "requests",
            "refused",
            "failed",
            "slow",
            "toolCalls",
            "groupsMade",
            "spacesMade",
        ];
        let counts = fields.map(|f| d[f].to_string()).join("\t");
        format!("{}\t{counts}", text(&d["day"]))
    }));
    out.push_str(&rows(&said["routes"], |r| {
        format!(
            "route\t{} {}\t{} requests, {} refused, {} failed, {} ms on average, {} slow",
            text(&r["method"]),
            text(&r["route"]),
            r["requests"],
            r["refused"],
            r["failed"],
            r["meanMs"],
            r["slow"]
        )
    }));
    out.push_str(&rows(&said["tools"], |t| {
        format!(
            "tool\t{}\t{} calls, {} failed",
            text(&t["tool"]),
            t["calls"],
            t["failed"]
        )
    }));
    out
}

/// A tool's command, as `me`, in `group`: what it answers, as JSON; or
/// for `group show`, as lines.
async fn command(
    me: &Client,
    host: &Host,
    tool: &Tool,
    group: &str,
    mut given: Value,
) -> Result<String> {
    let group = me.resolve(host, group).await?;
    // A person is named by a handle or a DID, and the tool takes a DID.
    let properties = tool.input_schema["properties"].as_object();
    for (name, schema) in properties.into_iter().flatten() {
        let named = given[name.as_str()].as_str().map(String::from);
        if let Some(who) = named.filter(|_| schema["format"] == "did") {
            given[name.as_str()] = json!(me.resolve(host, &who).await?);
        }
    }
    let planned = group_host_mcp::plan(tool, &given, &now_iso()).map_err(anyhow::Error::msg)?;
    let done = carry_out(me, host, &group, planned).await?;
    Ok(match tool.name.as_str() {
        "group_show" => {
            // The first hundred, to one who may read them, as getGroup
            // counts them.
            let members = match done["group"].get("memberCount") {
                Some(_) => {
                    let asked = [("group", group.as_str()), ("limit", "100")];
                    let page = me.query(host, &internal("listMembers"), &asked).await?;
                    page["members"].clone()
                }
                None => Value::Null,
            };
            shown(&done, &members)
        }
        _ => format!("{}\n", serde_json::to_string_pretty(&done)?),
    })
}

/// A query's parameters from JSON, each value as text, a list's each
/// item a parameter of its own.
fn query_of(input: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (k, v) in input.as_object().into_iter().flatten() {
        let items = match v {
            Value::Array(items) => items.iter().collect(),
            one => vec![one],
        };
        for item in items {
            let text = item.as_str().map_or_else(|| item.to_string(), String::from);
            out.push((k.clone(), text));
        }
    }
    out
}

/// Carries out `plan` as `me`, in `group`: a method asked with a service
/// token, a record written with a group token for its space, as an app
/// writes as the group.
async fn carry_out(me: &Client, host: &Host, group: &str, plan: Plan) -> Result<Value> {
    match plan {
        Plan::Call {
            nsid,
            procedure,
            mut input,
        } => {
            input["group"] = json!(group);
            if procedure {
                return me.call(host, &nsid, &input).await;
            }
            let params = query_of(&input);
            let params: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            me.query(host, &nsid, &params).await
        }
        Plan::Write {
            space,
            collection,
            rkey,
            record,
        } => {
            let uri = space_uri(group, &space, "self");
            let session = GroupSession::start(me, host, group, &uri).await?;
            let mut body = json!({"repo": group, "space": uri, "collection": collection});
            if let Some(key) = &rkey {
                body["rkey"] = json!(key);
            }
            let nsid = match (&rkey, record) {
                (Some(_), Some(record)) => {
                    body["record"] = record;
                    "com.atproto.space.putRecord"
                }
                (None, Some(record)) => {
                    body["record"] = record;
                    "com.atproto.space.createRecord"
                }
                (_, None) => "com.atproto.space.deleteRecord",
            };
            session.call(nsid, &body).await
        }
        Plan::Invite {
            person,
            record: Some(record),
        } => me.invite(host, group, &person, record).await,
        Plan::Invite {
            person,
            record: None,
        } => {
            // The one waiting, which the group's list of them names.
            let lxm = internal("listInvites");
            let listed = me
                .query(host, &lxm, &[("group", group), ("limit", "100")])
                .await?;
            let invites = listed["invites"].as_array().into_iter().flatten();
            let mut waiting = invites.filter(|i| i["invitee"] == person.as_str());
            let uri = waiting.next().map(|i| text(&i["uri"]).to_string());
            let record = uri.as_deref().map(SpaceRecordUri::parse).transpose()?;
            let rkey = record.map(|r| r.rkey).unwrap_or_default();
            let inbox = inbox_of(&person);
            let session = GroupSession::start(me, host, group, &inbox).await?;
            let body =
                json!({"repo": group, "space": inbox, "collection": m("invite"), "rkey": rkey});
            session.call("com.atproto.space.deleteRecord", &body).await
        }
        Plan::Edit {
            space,
            rkey,
            records,
        } => {
            let uri = space_uri(group, &space, "self");
            let mut done = Vec::new();
            for (collection, fields) in records {
                // The record as it is, as the space's records are listed.
                let record = format!("{uri}/{group}/{collection}/{rkey}");
                let asked = [("group", group), ("space", &uri), ("uri", &record)];
                let listed = me
                    .query(host, &internal("listSpaceRecords"), &asked)
                    .await?;
                let kept = listed["records"][0]["value"].clone();
                let write =
                    group_host_mcp::edited(&kept, space.clone(), collection, rkey.clone(), fields);
                done.push(Box::pin(carry_out(me, host, group, write)).await?);
            }
            Ok(json!({ "written": done }))
        }
        Plan::Label {
            subject,
            cid,
            val,
            undo,
        } => label(me, host, group, &subject, cid.as_deref(), &val, undo).await,
        Plan::Grant {
            role,
            permissions,
            can_assign,
            undo,
        } => {
            // The role's record as it is, as the space's records are listed.
            let uri = members_of(group);
            let record = format!("{uri}/{group}/{}/{role}", m("role"));
            let asked = [("group", group), ("space", &uri), ("uri", &record)];
            let listed = me
                .query(host, &internal("listSpaceRecords"), &asked)
                .await?;
            let kept = &listed["records"][0]["value"];
            let write = group_host_mcp::regrant(kept, &role, &permissions, &can_assign, undo);
            let write = write.map_err(anyhow::Error::msg)?;
            Box::pin(carry_out(me, host, group, write)).await
        }
        Plan::Space { kind, skey, policy } => {
            // A key of its own, so that the token is for the space it makes.
            let skey = skey.unwrap_or_else(|| atproto::tid::TidClock::new(0).next());
            let uri = space_uri(group, &kind, &skey);
            let session = GroupSession::start(me, host, group, &uri).await?;
            let body = json!({"spaceType": kind, "skey": skey, "policy": policy});
            session.call("com.atproto.space.createSpace", &body).await
        }
    }
}

/// Labels `subject` with `val` as `group`, beside it, with a group token
/// for that space; with `undo`, deletes each label of that value on it
/// there, as the space's records list them.
async fn label(
    me: &Client,
    host: &Host,
    group: &str,
    subject: &str,
    cid: Option<&str>,
    val: &str,
    undo: bool,
) -> Result<Value> {
    let Some(space) = group_host_mcp::label_space(group, subject) else {
        bail!("{subject} is not an account's DID, or a record's URI in one of the group's spaces");
    };
    let session = GroupSession::start(me, host, group, &space).await?;
    let collection = group_host_mcp::LABEL;
    if !undo {
        let record = group_host_mcp::label_record(subject, cid, val, &now_iso());
        let body =
            json!({"repo": group, "space": space, "collection": collection, "record": record});
        return session.call("com.atproto.space.createRecord", &body).await;
    }
    let asked = [
        ("group", group),
        ("space", space.as_str()),
        ("collection", collection),
        ("field", "uri"),
        ("equals", subject),
        ("limit", "100"),
    ];
    let listed = me
        .query(host, &internal("listSpaceRecords"), &asked)
        .await?;
    let records = listed["records"].as_array().into_iter().flatten();
    let ours =
        records.filter(|r| r["repo"] == group && group_host_mcp::labels(&r["value"], subject, val));
    let mut uris = Vec::new();
    for r in ours {
        let uri = text(&r["uri"]);
        let rkey = SpaceRecordUri::parse(uri)?.rkey;
        let body = json!({"repo": group, "space": space, "collection": collection, "rkey": rkey});
        session
            .call("com.atproto.space.deleteRecord", &body)
            .await?;
        uris.push(json!(uri));
    }
    if uris.is_empty() {
        bail!("no {val} label on {subject} to take back");
    }
    Ok(json!({ "uris": uris }))
}

const NS: &str = "org.atgrp";

fn m(name: &str) -> String {
    format!("{NS}.{name}")
}

/// A method of the host's own, not in the proposal: by its name, under
/// `org.atgrp.internal`.
fn internal(name: &str) -> String {
    format!("{NS}.internal.{name}")
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

async fn create_group(
    me: &Client,
    host: &Host,
    handle: &str,
    flags: &HashMap<&str, &str>,
) -> Result<String> {
    let label = handle.split('.').next().unwrap_or_default();
    let name = flags.get("display-name").copied().unwrap_or(label);
    let mut body = json!({"handle": handle, "displayName": name});
    if let Some(d) = flags.get("description") {
        body["description"] = json!(d);
    }
    if let Some(j) = flags.get("join-policy") {
        body["joinPolicy"] = json!(j);
    }
    let made = me.call(host, &internal("createGroup"), &body).await?;
    Ok(format!(
        "group    {handle} {}\nfounder  {}\nmeta     {}\nmembers  {}\n",
        text(&made["did"]),
        me.did,
        text(&made["meta"]),
        text(&made["members"]),
    ))
}

/// Each of `list`'s items, by `f`, a line each, or `none`.
fn lines(list: &Value, f: impl Fn(&Value) -> String) -> String {
    let items = list.as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return "  none\n".into();
    }
    items.iter().map(|i| format!("  {}\n", f(i))).collect()
}

/// A member's line: their handle, or DID, and their roles.
fn member_line(member: &Value) -> String {
    let roles: Vec<&str> = member["roles"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| text(&r["role"]))
        .collect();
    let who = &member["actor"];
    let name = who["handle"].as_str().unwrap_or(text(&who["did"]));
    format!("{name:<30} {}", roles.join(", "))
}

/// What getGroup `said`, and its `members`, as `group show` prints them.
fn shown(said: &Value, members: &Value) -> String {
    let g = &said["group"];
    let mut out = format!(
        "group    {} {}\nname     {}\njoining  {}\n",
        text(&g["handle"]),
        text(&g["did"]),
        text(&g["displayName"]),
        text(&g["joinPolicy"]),
    );
    out += "roles\n";
    out += &lines(&said["roles"], |r| {
        format!("{:<12} {}", text(&r["key"]), text(&r["name"]))
    });
    out += "members\n";
    out += &lines(members, member_line);
    out += "spaces\n";
    out += &lines(&said["spaces"], |s| text(&s["uri"]).to_string());
    out
}

/// A group's members space.
fn members_of(group: &str) -> String {
    space_uri(group, &format!("{NS}.members"), "self")
}

/// Deletes `group`, for good: what went.
async fn delete(me: &Client, host: &Host, group: &str) -> Result<String> {
    let group = me.resolve(host, group).await?;
    let body = json!({"group": group});
    let done = me.call(host, &internal("deleteGroup"), &body).await?;
    let identity = match done["tombstoned"].as_bool() {
        Some(true) => "its DID is ended",
        _ => "its DID is left to the holders of its recovery keys",
    };
    Ok(format!("deleted {group}\n{identity}\n"))
}

async fn migrate(me: &Client, from: &Host, to: &Host, group: &str) -> Result<String> {
    let group = me.resolve(from, group).await?;
    let mut out = format!("moving {group}\n  from {}\n  to   {}\n", from.url, to.url);
    let body = json!({"group": group, "destDid": to.did});
    let prepared = match me.call(from, &internal("migratePrepare"), &body).await {
        Ok(prepared) => prepared,
        // Handed over by a run that stopped before switching it on: that
        // is the step left.
        Err(e) if e.to_string().contains("NotController") => {
            return activate(me, to, &group, out + "handed over already\n").await;
        }
        Err(e) => return Err(e),
    };
    let spaces = prepared["snapshot"]["spaces"]
        .as_array()
        .map_or(0, Vec::len);
    out += &format!("prepared  {spaces} spaces\n");
    let body = json!({
        "did": group,
        "serviceJwt": prepared["serviceJwt"],
        "snapshot": prepared["snapshot"],
        "exportToken": prepared["exportToken"],
    });
    let received = me.call(to, &internal("migrateReceive"), &body).await?;
    out += &format!(
        "received  {} records, {} files, {} spaces\n",
        received["records"], received["blobs"], received["spaces"]
    );
    let body = json!({"group": group, "recommendedCreds": received["recommendedCreds"]});
    let done = me.call(from, &internal("migrateComplete"), &body).await?;
    out += &format!("handed over to {}\n", text(&done["movedTo"]));
    activate(me, to, &group, out).await
}

/// Switches `group` on at `to`, once handed over: what was done, or, if it
/// could not be yet, that the same command run again does it.
async fn activate(me: &Client, to: &Host, group: &str, mut out: String) -> Result<String> {
    let body = json!({"did": group});
    if let Err(e) = me.call(to, &internal("migrateActivate"), &body).await {
        bail!("{out}not switched on yet: {e}\nrun the same command again to switch it on");
    }
    out += "switched on\n";
    Ok(out)
}

/// The cursor a listing's next page starts after, if it has one.
fn next(listed: &Value) -> Option<String> {
    let cursor = text(&listed["cursor"]);
    (!cursor.is_empty()).then_some(cursor.to_string())
}

/// Moves `account` here to become a group run by `me`, in two runs: one
/// copies it and its spaces and has its server email a code, and one, with
/// that code, has its server sign its identity over and finishes.
async fn import(
    me: &Client,
    host: &Host,
    env: &Env,
    account: &str,
    a: &Args<'_>,
) -> Result<String> {
    let did = me.resolve(host, account).await?;
    let mut asked = vec![("did", did.as_str())];
    asked.extend(a.flags.get("address").map(|l| ("address", *l)));
    let plan = me.query(host, &internal("checkImport"), &asked).await?;
    // A move of yours already pending is finished, whatever the address
    // would be now.
    let pending = plan.get("recommendedCreds").is_some();
    if let Some(no) = plan.get("refused").filter(|_| !pending) {
        bail!("{} can't move here: {}", account, text(&no["message"]));
    }
    let Some(password) = &env.import_password else {
        bail!("set ATGRP_IMPORT_PASSWORD to {account}'s own password at its server");
    };
    let theirs = Client::sign_in(text(&plan["pds"]), account, password).await?;
    match a.flags.get("code") {
        None => start_import(me, &theirs, host, &plan, account, a.flags.get("address")).await,
        Some(code) => finish_import(me, &theirs, host, &plan, code).await,
    }
}

async fn start_import(
    me: &Client,
    theirs: &Client,
    host: &Host,
    plan: &Value,
    account: &str,
    address: Option<&&str>,
) -> Result<String> {
    let did = theirs.did.as_str();
    let proof = theirs
        .token(&host.did, "com.atproto.server.createAccount")
        .await?;
    let mut body = json!({ "serviceJwt": proof });
    if let Some(label) = address {
        body["address"] = json!(label);
    }
    let copied = me.call(host, &internal("importAccount"), &body).await?;
    let mut out = format!(
        "moving {did}\n  from {}\n  as   {}\n",
        text(&plan["pds"]),
        text(&copied["handle"])
    );
    out += &format!(
        "copied    {} records, {} files\n",
        copied["records"], copied["blobs"]
    );
    // Each space it governs or wrote in: a space it governs first, then its
    // files there, then its repo, which holds them.
    let mut spaces = Vec::new();
    // Only a server that says it holds spaces has any to bring.
    let described = theirs
        .own("com.atproto.server.describeServer", &[], None)
        .await?;
    let holds = described["spacesAlpha"] == true;
    let mut cursor = holds.then_some(String::new());
    while let Some(after) = cursor.take() {
        let mut page = vec![("limit", "100")];
        page.extend((!after.is_empty()).then_some(("cursor", after.as_str())));
        let listed = theirs
            .own("com.atproto.space.listSpaces", &page, None)
            .await?;
        let uris = listed["spaces"].as_array().into_iter().flatten();
        spaces.extend(uris.map(|s| text(&s["space"]).to_string()));
        cursor = next(&listed);
    }
    let (mut files, mut records) = (0, 0);
    let bring = &internal("importAccountSpace");
    for space in &spaces {
        let at = [("space", space.as_str()), ("repo", did)];
        let governs = space.starts_with(&format!("{SPACE_URI_PREFIX}{did}/"));
        if governs {
            let meta = theirs
                .own_bytes("com.atproto.space.exportSpaceMeta", &at[..1])
                .await?;
            let (meta, kind) = meta.context(format!("{space} exported nothing"))?;
            me.send(host, bring, &[("did", did), ("part", "meta")], meta, &kind)
                .await?;
        }
        let mut cursor = Some(String::new());
        while let Some(after) = cursor.take() {
            let mut page = at.to_vec();
            page.extend((!after.is_empty()).then_some(("cursor", after.as_str())));
            let listed = theirs
                .own("com.atproto.space.listBlobs", &page, None)
                .await?;
            for cid in listed["cids"].as_array().into_iter().flatten() {
                let at = [at[0], at[1], ("cid", text(cid))];
                let blob = theirs.own_bytes("com.atproto.space.getBlob", &at).await?;
                let (bytes, kind) = blob.context(format!("{space} has no file {}", text(cid)))?;
                me.send(host, bring, &[("did", did), ("part", "blob")], bytes, &kind)
                    .await?;
                files += 1;
            }
            cursor = next(&listed);
        }
        if let Some((repo, kind)) = theirs.own_bytes("com.atproto.space.getRepo", &at).await? {
            let kept = me
                .send(host, bring, &[("did", did), ("part", "repo")], repo, &kind)
                .await?;
            records += kept["records"].as_u64().unwrap_or_default();
        }
    }
    out += &format!(
        "brought   {} spaces: {records} records, {files} files\n",
        spaces.len()
    );
    let ask = "com.atproto.identity.requestPlcOperationSignature";
    theirs.own(ask, &[], Some(&Value::Null)).await?;
    out += &format!(
        "its server has emailed a code to move its identity; finish with\n  atgrp account import {account} --code <code>\n"
    );
    Ok(out)
}

async fn finish_import(
    me: &Client,
    theirs: &Client,
    host: &Host,
    plan: &Value,
    code: &str,
) -> Result<String> {
    let did = theirs.did.as_str();
    let Some(Value::Object(creds)) = plan.get("recommendedCreds") else {
        bail!("no move of {did} of yours is under way here: start it without --code");
    };
    let mut asked = creds.clone();
    asked.insert("token".into(), json!(code.trim()));
    let sign = "com.atproto.identity.signPlcOperation";
    let signed = theirs.own(sign, &[], Some(&Value::Object(asked))).await?;
    let body = json!({ "did": did, "operation": signed["operation"] });
    let done = me.call(host, &internal("completeImport"), &body).await?;
    Ok(format!(
        "moved: {} is a group here, run by you\n",
        text(&done["handle"])
    ))
}

/// The rules a seeded group starts with.
const RULES: [(&str, &str); 2] = [
    ("Be kind", "Disagree with ideas, not people."),
    ("Stay on topic", "Post what the group is for."),
];

/// What a seeded group's moderator may do: moderate it, as the group's
/// moderation permission set has it (labels, ejections, the moderation
/// queue), and label in any of its spaces.
const MODERATES: [&str; 2] = [
    "include:org.atgrp.authModerate",
    "space:*?collection=com.atproto.label.record",
];

/// Makes `group` a moderator role, as the group, which its admins, who may
/// give every role, may give.
async fn moderator(me: &Client, host: &Host, group: &str) -> Result<()> {
    let role = json!({
        "$type": m("role"),
        "displayName": "Moderator",
        "description": "Keeps the group to its rules.",
        "permissions": MODERATES,
    });
    let members = members_of(group);
    let session = GroupSession::start(me, host, group, &members).await?;
    let body = json!({"repo": group, "space": members, "collection": m("role"), "rkey": "moderator", "record": role});
    session
        .call("com.atproto.space.createRecord", &body)
        .await?;
    Ok(())
}

async fn seed(me: &Client, host: &Host, handle: &str, invite: &[String]) -> Result<String> {
    let body = json!({
        "handle": handle,
        "displayName": "A group to try things on",
        "description": "Seeded by atgrp group seed.",
        "joinPolicy": "approval",
    });
    let made = me.call(host, &internal("createGroup"), &body).await?;
    let group = text(&made["did"]).to_string();
    let mut out = format!("group      {handle} {group}\n");
    // Its rules are its own records, in its members space, written as it.
    let members = text(&made["members"]).to_string();
    let session = GroupSession::start(me, host, &group, &members).await?;
    for (title, words) in RULES {
        let rule = json!({"$type": m("rule"), "title": title, "text": words});
        let body =
            json!({"repo": group, "space": members, "collection": m("rule"), "record": rule});
        let written = session.call("com.atproto.space.createRecord", &body);
        written.await?;
        out += &format!("rule       {title}\n");
    }
    moderator(me, host, &group).await?;
    out += "role       moderator\n";
    for who in invite {
        let person = me.resolve(host, who).await?;
        let record = json!({"$type": m("invite"), "createdAt": now_iso()});
        me.invite(host, &group, &person, record).await?;
        out += &format!("invited    {who}\n");
    }
    Ok(out)
}
