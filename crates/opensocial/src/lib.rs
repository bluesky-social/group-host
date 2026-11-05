//! A client for opensocial group hosts, and what the `opensocial` command
//! does with it.
//!
//! The command acts as a person: it signs in to their own PDS with their
//! handle and password (an app password is enough), and asks that PDS for
//! a service token for each method it calls on a host, as an app does. So
//! an account on any PDS may run the command against any host. To write as
//! a group, as an invite is written, it asks the host for a group token
//! (`getGroupAuth`), bound to a DPoP key of its own.

use anyhow::{Context, Result, anyhow, bail};
use atproto::jose;
use atproto::keys::{Curve, Keypair};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

/// What `opensocial` prints for `--help`, or with no command.
pub const USAGE: &str = "usage: opensocial <command>

  create-group <handle> [--name <name>] [--description <words>] [--join open|approval|invite]
                         start a group, run by you
  inspect <group>        a group as you may see it: its roles, members and spaces
  membership assign <group> <member> <role,role…>
                         give a member these roles, in place of theirs
  membership eject <group> <member> [--reason <words>]
                         put a member out
  membership invite <group> <person> [--message <words>]
                         invite someone in
  migrate <group> --to <address> [--to-did <did>]
                         move a group, whole, from this host to the one at <address>
  import <account> [--address <label>]
                         move an account from its server here, its spaces with it, to
                         become a group run by you; its server emails a code
  import <account> --code <code>
                         with that code, finish moving it
  seed <handle>          a group to try things on: two rules, a moderator role, and an
                         invite for each handle in OPENSOCIAL_SEED_INVITE

  A group or person is a handle or a DID. You sign in as OPENSOCIAL_AS (a
  handle) with OPENSOCIAL_PASSWORD, at PDS_URL; the host is OPENSOCIAL_HOST,
  its DID OPENSOCIAL_HOST_DID (by default the did:web of its address). An
  account you import signs in at its own server with OPENSOCIAL_IMPORT_PASSWORD,
  its own password: its server hands its identity over for no app password.
";

/// Where the command acts, and as whom: from the environment.
#[derive(Debug, Clone)]
pub struct Env {
    /// The host, `OPENSOCIAL_HOST`.
    pub host: String,
    /// Its DID, `OPENSOCIAL_HOST_DID`.
    pub host_did: String,
    /// The PDS the person signs in at, `PDS_URL`.
    pub pds: String,
    /// The person, `OPENSOCIAL_AS`, and their password,
    /// `OPENSOCIAL_PASSWORD`.
    pub handle: Option<String>,
    pub password: Option<String>,
    /// Who `seed` invites, `OPENSOCIAL_SEED_INVITE`, comma-separated.
    pub seed_invite: Vec<String>,
    /// The password of an account `import` moves, at its own server,
    /// `OPENSOCIAL_IMPORT_PASSWORD`.
    pub import_password: Option<String>,
}

/// The did:web a host at `url` is by default, as the host names itself.
pub fn did_web(url: &str) -> String {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    format!("did:web:{}", host.replace(':', "%3A"))
}

impl Env {
    /// The environment `var` reads.
    pub fn from(var: impl Fn(&str) -> Option<String>) -> Env {
        let host = var("OPENSOCIAL_HOST").unwrap_or_else(|| "http://localhost:2600".into());
        let host = host.trim_end_matches('/').to_string();
        let host_did = var("OPENSOCIAL_HOST_DID").unwrap_or_else(|| did_web(&host));
        let pds = var("PDS_URL").unwrap_or_else(|| "http://localhost:2583".into());
        let invite = var("OPENSOCIAL_SEED_INVITE").unwrap_or_default();
        Env {
            host,
            host_did,
            pds: pds.trim_end_matches('/').to_string(),
            handle: var("OPENSOCIAL_AS"),
            password: var("OPENSOCIAL_PASSWORD"),
            import_password: var("OPENSOCIAL_IMPORT_PASSWORD"),
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
    /// their inbox, from `getGroupAuth`, an invite written there through
    /// `com.atproto.space.createRecord`.
    pub async fn invite(
        &self,
        host: &Host,
        group: &str,
        invitee: &str,
        message: Option<&str>,
    ) -> Result<Value> {
        let inbox = format!("at://{invitee}/space/fyi.opensocial.invites/self");
        let session = GroupSession::start(self, host, group, &inbox).await?;
        let mut record = json!({"$type": "fyi.opensocial.invite", "createdAt": now_iso()});
        if let Some(words) = message {
            record["message"] = json!(words);
        }
        let body = json!({"repo": group, "space": inbox, "collection": "fyi.opensocial.invite", "record": record});
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

/// Now, as atproto writes a datetime.
fn now_iso() -> String {
    atproto::time::iso(atproto::time::now_millis())
}

const GET_GROUP_AUTH: &str = "fyi.opensocial.getGroupAuth";

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
        let mut claims = json!({
            "jti": jose::random(12),
            "htm": "POST",
            "htu": format!("{}/xrpc/{nsid}", self.public),
            "iat": atproto::time::now_millis() / 1000,
        });
        if let Some(nonce) = self.nonce.lock().expect("not poisoned").clone() {
            claims["nonce"] = json!(nonce);
        }
        if let Some(token) = token {
            claims["ath"] = json!(jose::sha256(token));
        }
        let header = json!({"typ": "dpop+jwt", "jwk": jose::jwk(&self.key.public())});
        jose::sign(&self.key, &header, &claims)
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

fn parse<'a>(args: &'a [String], known: &[&str]) -> Result<Args<'a>> {
    let mut words = Vec::new();
    let mut flags = HashMap::new();
    let mut it = args.iter().map(String::as_str);
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

/// Signs in as the environment's person.
async fn signed_in(env: &Env) -> Result<Client> {
    let (Some(handle), Some(password)) = (&env.handle, &env.password) else {
        bail!("set OPENSOCIAL_AS and OPENSOCIAL_PASSWORD to sign in");
    };
    Client::sign_in(&env.pds, handle, password).await
}

/// Runs the command `args` name (without the program's own name), in
/// `env`, returning what it prints.
pub async fn run(args: &[String], env: &Env) -> Result<String> {
    let host = Host {
        url: env.host.clone(),
        did: env.host_did.clone(),
    };
    let Some((command, rest)) = args.split_first() else {
        return Ok(USAGE.to_string());
    };
    match command.as_str() {
        "help" | "--help" | "-h" => Ok(USAGE.to_string()),
        "create-group" => {
            let a = parse(rest, &["name", "description", "join"])?;
            let [handle] = a.take()?;
            create_group(&signed_in(env).await?, &host, handle, &a.flags).await
        }
        "inspect" => {
            let [group] = parse(rest, &[])?.take()?;
            inspect(&signed_in(env).await?, &host, group).await
        }
        "membership" => {
            // Each takes its own option: eject a reason, invite a message.
            let known: &[&str] = match rest.first().map(String::as_str) {
                Some("eject") => &["reason"],
                Some("invite") => &["message"],
                _ => &[],
            };
            let a = parse(rest, known)?;
            membership(&signed_in(env).await?, &host, &a).await
        }
        "migrate" => {
            let a = parse(rest, &["to", "to-did"])?;
            let [group] = a.take()?;
            let to = a.flags.get("to").ok_or_else(usage)?.trim_end_matches('/');
            let did = a.flags.get("to-did").map(|d| d.to_string());
            let dest = Host {
                url: to.to_string(),
                did: did.unwrap_or_else(|| did_web(to)),
            };
            migrate(&signed_in(env).await?, &host, &dest, group).await
        }
        "import" => {
            let a = parse(rest, &["address", "code"])?;
            let [account] = a.take()?;
            import(&signed_in(env).await?, &host, env, account, &a).await
        }
        "seed" => {
            let [handle] = parse(rest, &[])?.take()?;
            seed(&signed_in(env).await?, &host, handle, &env.seed_invite).await
        }
        other => bail!("no command {other}\n\n{USAGE}"),
    }
}

const NS: &str = "fyi.opensocial";

fn m(name: &str) -> String {
    format!("{NS}.{name}")
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
    let name = flags.get("name").copied().unwrap_or(label);
    let mut body = json!({"handle": handle, "displayName": name});
    if let Some(d) = flags.get("description") {
        body["description"] = json!(d);
    }
    if let Some(j) = flags.get("join") {
        body["joinPolicy"] = json!(j);
    }
    let made = me.call(host, &m("createGroup"), &body).await?;
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

async fn inspect(me: &Client, host: &Host, group: &str) -> Result<String> {
    let did = me.resolve(host, group).await?;
    let said = me.query(host, &m("getGroup"), &[("group", &did)]).await?;
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
    out += &lines(&said["members"], member_line);
    out += "spaces\n";
    out += &lines(&said["spaces"], |s| text(&s["uri"]).to_string());
    Ok(out)
}

async fn membership(me: &Client, host: &Host, a: &Args<'_>) -> Result<String> {
    let (&what, rest) = a.words.split_first().ok_or_else(usage)?;
    let (&group, &who) = (
        rest.first().ok_or_else(usage)?,
        rest.get(1).ok_or_else(usage)?,
    );
    let call = match (what, rest.len()) {
        ("assign", 3) => "assignRoles",
        ("eject", 2) => "ejectMember",
        ("invite", 2) => "",
        _ => return Err(usage()),
    };
    let group = me.resolve(host, group).await?;
    let person = me.resolve(host, who).await?;
    // An invite is a record the group writes in their inbox.
    if call.is_empty() {
        let message = a.flags.get("message").copied();
        me.invite(host, &group, &person, message).await?;
        return Ok(format!("{who}: invited\n"));
    }
    let mut body = json!({"group": group});
    let done = match what {
        "assign" => {
            let roles: Vec<&str> = rest[2].split(',').filter(|r| !r.is_empty()).collect();
            body["member"] = json!(person);
            let refs: Vec<Value> = roles.iter().map(|r| json!({ "role": r })).collect();
            body["roles"] = json!(refs);
            roles.join(", ")
        }
        _ => {
            body["member"] = json!(person);
            if let Some(r) = a.flags.get("reason") {
                body["reason"] = json!(r);
            }
            "put out".to_string()
        }
    };
    me.call(host, &m(call), &body).await?;
    Ok(format!("{who}: {done}\n"))
}

async fn migrate(me: &Client, from: &Host, to: &Host, group: &str) -> Result<String> {
    let group = me.resolve(from, group).await?;
    let mut out = format!("moving {group}\n  from {}\n  to   {}\n", from.url, to.url);
    let body = json!({"group": group, "destDid": to.did});
    let prepared = match me.call(from, &m("migratePrepare"), &body).await {
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
    let received = me.call(to, &m("migrateReceive"), &body).await?;
    out += &format!(
        "received  {} records, {} files, {} spaces\n",
        received["records"], received["blobs"], received["spaces"]
    );
    let body = json!({"group": group, "recommendedCreds": received["recommendedCreds"]});
    let done = me.call(from, &m("migrateComplete"), &body).await?;
    out += &format!("handed over to {}\n", text(&done["movedTo"]));
    activate(me, to, &group, out).await
}

/// Switches `group` on at `to`, once handed over: what was done, or, if it
/// could not be yet, that the same command run again does it.
async fn activate(me: &Client, to: &Host, group: &str, mut out: String) -> Result<String> {
    let body = json!({"did": group});
    if let Err(e) = me.call(to, &m("migrateActivate"), &body).await {
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
    let plan = me.query(host, &m("checkImport"), &asked).await?;
    // A move of yours already pending is finished, whatever the address
    // would be now.
    let pending = plan.get("recommendedCreds").is_some();
    if let Some(no) = plan.get("refused").filter(|_| !pending) {
        bail!("{} can't move here: {}", account, text(&no["message"]));
    }
    let Some(password) = &env.import_password else {
        bail!("set OPENSOCIAL_IMPORT_PASSWORD to {account}'s own password at its server");
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
    let copied = me.call(host, &m("importAccount"), &body).await?;
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
        spaces.extend(uris.map(|s| text(&s["uri"]).to_string()));
        cursor = next(&listed);
    }
    let (mut files, mut records) = (0, 0);
    let bring = &m("importAccountSpace");
    for space in &spaces {
        let at = [("space", space.as_str()), ("repo", did)];
        let governs = space.starts_with(&format!("at://{did}/"));
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
        "its server has emailed a code to move its identity; finish with\n  opensocial import {account} --code <code>\n"
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
    let done = me.call(host, &m("completeImport"), &body).await?;
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

async fn seed(me: &Client, host: &Host, handle: &str, invite: &[String]) -> Result<String> {
    let body = json!({
        "handle": handle,
        "displayName": "A group to try things on",
        "description": "Seeded by opensocial seed.",
        "joinPolicy": "approval",
    });
    let made = me.call(host, &m("createGroup"), &body).await?;
    let group = text(&made["did"]).to_string();
    let mut out = format!("group      {handle} {group}\n");
    // Its rules are its own records, in its meta space, written as it.
    let meta = text(&made["meta"]).to_string();
    let session = GroupSession::start(me, host, &group, &meta).await?;
    for (title, words) in RULES {
        let rule = json!({"$type": m("rule"), "title": title, "text": words});
        let body = json!({"repo": group, "space": meta, "collection": m("rule"), "record": rule});
        let written = session.call("com.atproto.space.createRecord", &body);
        written.await?;
        out += &format!("rule       {title}\n");
    }
    let role = json!({
        "group": group,
        "role": "moderator",
        "name": "Moderator",
        "description": "Keeps the group to its rules.",
        "actions": ["mod.read", "mod.resolve", "label"],
    });
    me.call(host, &m("putRole"), &role).await?;
    out += "role       moderator\n";
    for who in invite {
        let person = me.resolve(host, who).await?;
        me.invite(host, &group, &person, None).await?;
        out += &format!("invited    {who}\n");
    }
    Ok(out)
}
