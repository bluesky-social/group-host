//! A client for opensocial group hosts, and what the `opensocial` command
//! does with it.
//!
//! The command acts as a person: it signs in to their own PDS with their
//! handle and password (an app password is enough), and asks that PDS for
//! a service token for each method it calls on a host, as an app does. So
//! an account on any PDS may run the command against any host.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;

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
  seed <handle>          a group to try things on: two rules, a moderator role, and an
                         invite for each handle in OPENSOCIAL_SEED_INVITE

  A group or person is a handle or a DID. You sign in as OPENSOCIAL_AS (a
  handle) with OPENSOCIAL_PASSWORD, at PDS_URL; the host is OPENSOCIAL_HOST,
  its DID OPENSOCIAL_HOST_DID (by default the did:web of its address).
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
        ("invite", 2) => "createInvite",
        _ => return Err(usage()),
    };
    let group = me.resolve(host, group).await?;
    let person = me.resolve(host, who).await?;
    let mut body = json!({"group": group});
    let done = match what {
        "assign" => {
            let roles: Vec<&str> = rest[2].split(',').filter(|r| !r.is_empty()).collect();
            body["member"] = json!(person);
            let refs: Vec<Value> = roles.iter().map(|r| json!({ "role": r })).collect();
            body["roles"] = json!(refs);
            roles.join(", ")
        }
        "eject" => {
            body["member"] = json!(person);
            if let Some(r) = a.flags.get("reason") {
                body["reason"] = json!(r);
            }
            "put out".to_string()
        }
        _ => {
            body["invitee"] = json!(person);
            if let Some(words) = a.flags.get("message") {
                body["message"] = json!(words);
            }
            "invited".to_string()
        }
    };
    me.call(host, &m(call), &body).await?;
    Ok(format!("{who}: {done}\n"))
}

async fn migrate(me: &Client, from: &Host, to: &Host, group: &str) -> Result<String> {
    let group = me.resolve(from, group).await?;
    let mut out = format!("moving {group}\n  from {}\n  to   {}\n", from.url, to.url);
    let body = json!({"group": group, "destDid": to.did});
    let prepared = me.call(from, &m("migratePrepare"), &body).await?;
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
    let body = json!({"did": group});
    me.call(to, &m("migrateActivate"), &body).await?;
    out += "switched on\n";
    Ok(out)
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
    for (title, words) in RULES {
        let body = json!({"group": group, "title": title, "text": words});
        me.call(host, &m("putRule"), &body).await?;
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
        let body = json!({"group": group, "invitee": person});
        me.call(host, &m("createInvite"), &body).await?;
        out += &format!("invited    {who}\n");
    }
    Ok(out)
}
