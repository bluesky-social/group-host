//! The host's database: one SQLite file holding every group's account, its
//! repo's current commit, and its records. The tree a commit names is
//! not kept: it is built again from the records, as a group's repo is
//! small (`atproto::mst`).

use anyhow::{Context, Result};
use atproto::{Cid, Curve, Keypair};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Each version of the schema, applied in order to a database below it.
const MIGRATIONS: &[&str] = &[
    "
    CREATE TABLE account (
        did TEXT PRIMARY KEY,
        handle TEXT NOT NULL UNIQUE,
        signing_key BLOB NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE repo (
        did TEXT PRIMARY KEY REFERENCES account (did),
        cid TEXT NOT NULL,
        rev TEXT NOT NULL,
        commit_block BLOB NOT NULL
    );
    CREATE TABLE record (
        did TEXT NOT NULL REFERENCES account (did),
        collection TEXT NOT NULL,
        rkey TEXT NOT NULL,
        cid TEXT NOT NULL,
        value BLOB NOT NULL,
        rev TEXT NOT NULL,
        PRIMARY KEY (did, collection, rkey)
    );
",
    "
    CREATE TABLE event (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        did TEXT NOT NULL,
        kind TEXT NOT NULL,
        body BLOB NOT NULL
    );
",
    "
    CREATE TABLE blob (
        did TEXT NOT NULL REFERENCES account (did),
        cid TEXT NOT NULL,
        mime_type TEXT NOT NULL,
        size INTEGER NOT NULL,
        temporary INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (did, cid)
    );
    CREATE INDEX blob_temporary ON blob (temporary, created_at);
    CREATE TABLE record_blob (
        did TEXT NOT NULL,
        collection TEXT NOT NULL,
        rkey TEXT NOT NULL,
        cid TEXT NOT NULL,
        PRIMARY KEY (did, collection, rkey, cid)
    );
    CREATE INDEX record_blob_cid ON record_blob (did, cid);
",
];

const NEW_ACCOUNT: &str = "INSERT INTO account (did, handle, signing_key, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (handle) DO NOTHING";
const PUT_RECORD: &str = "INSERT INTO record (did, collection, rkey, cid, value, rev) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (did, collection, rkey) DO UPDATE SET cid = ?4, value = ?5, rev = ?6";
const LIST_UP: &str = "SELECT collection, rkey, cid, value FROM record WHERE did = ?1 AND collection = ?2 AND rkey > ?3 ORDER BY rkey LIMIT ?4";
const LIST_DOWN: &str = "SELECT collection, rkey, cid, value FROM record WHERE did = ?1 AND collection = ?2 AND rkey < ?3 ORDER BY rkey DESC LIMIT ?4";
const LINKED: &str = "SELECT cid FROM record_blob WHERE did = ?1 AND collection = ?2 AND rkey = ?3";
const UNLINK: &str = "DELETE FROM record_blob WHERE did = ?1 AND collection = ?2 AND rkey = ?3";
const LINK: &str =
    "INSERT OR IGNORE INTO record_blob (did, collection, rkey, cid) VALUES (?1, ?2, ?3, ?4)";
const KEEP_BLOB: &str = "UPDATE blob SET temporary = 0 WHERE did = ?1 AND cid = ?2";
const FORGET_ORPHAN: &str = "DELETE FROM blob WHERE did = ?1 AND cid = ?2 AND temporary = 0 AND NOT EXISTS (SELECT 1 FROM record_blob WHERE did = ?1 AND cid = ?2)";
const PUT_BLOB: &str = "INSERT INTO blob (did, cid, mime_type, size, temporary, created_at) VALUES (?1, ?2, ?3, ?4, 1, ?5) ON CONFLICT (did, cid) DO UPDATE SET mime_type = excluded.mime_type";
const GET_BLOB: &str = "SELECT mime_type, size, temporary FROM blob WHERE did = ?1 AND cid = ?2";
const LIST_BLOBS: &str = "SELECT DISTINCT rb.cid FROM record_blob rb JOIN record r ON r.did = rb.did AND r.collection = rb.collection AND r.rkey = rb.rkey WHERE rb.did = ?1 AND r.rev > ?2 AND rb.cid > ?3 ORDER BY rb.cid LIMIT ?4";
const SWEEP_BLOBS: &str =
    "DELETE FROM blob WHERE temporary = 1 AND created_at < ?1 RETURNING did, cid";
const SEQUENCE: &str = "INSERT INTO event (did, kind, body) VALUES (?1, ?2, ?3)";
const EVENTS: &str = "SELECT seq, kind, body FROM event WHERE seq > ?1 ORDER BY seq LIMIT ?2";
const ACCOUNTS: &str =
    "SELECT did, cid, rev, commit_block FROM repo WHERE did > ?1 ORDER BY did LIMIT ?2";
const COLLECTIONS: &str =
    "SELECT DISTINCT collection FROM record WHERE did = ?1 ORDER BY collection";

pub struct Store {
    conn: Mutex<Connection>,
}

/// An account the host keeps.
pub struct Account {
    pub did: String,
    pub handle: String,
    /// Signs its repo's commits.
    pub signing: Keypair,
}

/// A repo's current commit.
#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub cid: Cid,
    pub rev: String,
    /// The signed commit's block.
    pub block: Vec<u8>,
}

/// A record as kept: where, its CID and its DRISL.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub collection: String,
    pub rkey: String,
    pub cid: Cid,
    pub value: Vec<u8>,
}

/// An event for the firehose: its kind (`#commit`, `#identity`,
/// `#account`) and its body as DRISL, without its `seq`, which the
/// database gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: &'static str,
    pub body: Vec<u8>,
}

/// An event as kept: its `seq`, kind and body.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequenced {
    pub seq: i64,
    pub kind: String,
    pub body: Vec<u8>,
}

/// The blobs the record at `collection/rkey` references now.
fn linked(tx: &rusqlite::Transaction, did: &str, collection: &str, rkey: &str) -> Result<Vec<Cid>> {
    let mut q = tx.prepare(LINKED)?;
    let rows = q.query_map(params![did, collection, rkey], |r| Ok(cid(r.get(0)?)));
    let rows = rows?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn sequence(tx: &rusqlite::Transaction, did: &str, events: &[Event]) -> Result<()> {
    for e in events {
        let added = tx.execute(SEQUENCE, params![did, e.kind, e.body]);
        added?;
    }
    Ok(())
}

/// What a commit changes at one path: a record put, or taken out.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub collection: String,
    pub rkey: String,
    /// The record's CID and DRISL, or none for a delete.
    pub put: Option<(Cid, Vec<u8>)>,
    /// The blobs the record put references.
    pub blobs: Vec<Cid>,
}

/// A blob as kept: its type, its size, and whether it waits for a record
/// to reference it.
#[derive(Debug, Clone, PartialEq)]
pub struct Blob {
    pub mime_type: String,
    pub size: i64,
    pub temporary: bool,
}

/// A CID the host wrote: one that does not read is a database broken.
fn cid(text: String) -> Cid {
    Cid::parse(&text).expect("a CID the host wrote")
}

fn record(row: &rusqlite::Row) -> rusqlite::Result<Record> {
    Ok(Record {
        collection: row.get(0)?,
        rkey: row.get(1)?,
        cid: cid(row.get(2)?),
        value: row.get(3)?,
    })
}

impl Store {
    /// The database at `path`, made or brought up to date. `:memory:` for
    /// one that lasts as long as the process.
    pub fn open(path: &Path) -> Result<Store> {
        let opening = format!("opening {}", path.display());
        let mut conn = Connection::open(path).context(opening)?;
        conn.pragma_update(None, "journal_mode", "wal")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let tx = conn.transaction()?;
        for (i, migration) in (0..).zip(MIGRATIONS).skip(version as usize) {
            tx.execute_batch(migration)?;
            tx.pragma_update(None, "user_version", i + 1_i64)?;
        }
        tx.commit()?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("not poisoned")
    }

    /// Keeps a new account, with its first commit and `events`. `false` if
    /// its handle is taken.
    pub fn create_account(
        &self,
        account: &Account,
        head: &Head,
        now: i64,
        events: &[Event],
    ) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let added = tx.execute(
            NEW_ACCOUNT,
            params![
                account.did,
                account.handle,
                account.signing.to_bytes().to_vec(),
                now
            ],
        );
        if added? == 0 {
            return Ok(false);
        }
        let kept = tx.execute(
            "INSERT INTO repo (did, cid, rev, commit_block) VALUES (?1, ?2, ?3, ?4)",
            params![account.did, head.cid.to_string(), head.rev, head.block],
        );
        kept?;
        sequence(&tx, &account.did, events)?;
        tx.commit()?;
        Ok(true)
    }

    /// The account `did` or `handle` names.
    pub fn account(&self, did_or_handle: &str) -> Result<Option<Account>> {
        let found = self
            .conn()
            .query_row(
                "SELECT did, handle, signing_key FROM account WHERE did = ?1 OR handle = lower(?1)",
                [did_or_handle],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        found
            .map(|(did, handle, key)| {
                Ok(Account {
                    did,
                    handle,
                    signing: Keypair::from_bytes(Curve::K256, &key)?,
                })
            })
            .transpose()
    }

    /// Accounts in DID order after `cursor`, at most `limit`, with their heads.
    pub fn accounts(&self, cursor: Option<&str>, limit: u32) -> Result<Vec<(String, Head)>> {
        let conn = self.conn();
        let mut q = conn.prepare(ACCOUNTS)?;
        let rows = q.query_map(params![cursor.unwrap_or(""), limit], |r| {
            Ok((
                r.get(0)?,
                Head {
                    cid: cid(r.get(1)?),
                    rev: r.get(2)?,
                    block: r.get(3)?,
                },
            ))
        });
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// `did`'s current commit.
    pub fn head(&self, did: &str) -> Result<Option<Head>> {
        let head = self
            .conn()
            .query_row(
                "SELECT cid, rev, commit_block FROM repo WHERE did = ?1",
                [did],
                |r| {
                    Ok(Head {
                        cid: cid(r.get(0)?),
                        rev: r.get(1)?,
                        block: r.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(head)
    }

    /// Every record of `did`'s, by path (`collection/rkey`), with its CID.
    pub fn paths(&self, did: &str) -> Result<BTreeMap<String, Cid>> {
        let conn = self.conn();
        let mut q = conn.prepare("SELECT collection, rkey, cid FROM record WHERE did = ?1")?;
        let rows = q.query_map([did], |r| {
            Ok((
                format!("{}/{}", r.get::<_, String>(0)?, r.get::<_, String>(1)?),
                cid(r.get(2)?),
            ))
        });
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every record of `did`'s.
    pub fn records(&self, did: &str) -> Result<Vec<Record>> {
        let conn = self.conn();
        let mut q =
            conn.prepare("SELECT collection, rkey, cid, value FROM record WHERE did = ?1")?;
        let rows = q.query_map([did], record)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// `did`'s record at `collection/rkey`.
    pub fn record(&self, did: &str, collection: &str, rkey: &str) -> Result<Option<Record>> {
        let found = self
            .conn()
            .query_row(
                "SELECT collection, rkey, cid, value FROM record WHERE did = ?1 AND collection = ?2 AND rkey = ?3",
                [did, collection, rkey],
                record,
            )
            .optional()?;
        Ok(found)
    }

    /// `did`'s records in `collection`, past `cursor` (an rkey), at most
    /// `limit`: newest key first, or oldest with `reverse`.
    pub fn list(
        &self,
        did: &str,
        collection: &str,
        cursor: Option<&str>,
        limit: u32,
        reverse: bool,
    ) -> Result<Vec<Record>> {
        let conn = self.conn();
        let sql = if reverse { LIST_UP } else { LIST_DOWN };
        // No rkey sorts before "" or after "\u{7f}": its characters are ASCII.
        let from = cursor.unwrap_or(if reverse { "" } else { "\u{7f}" });
        let mut q = conn.prepare(sql)?;
        let rows = q.query_map(params![did, collection, from, limit], record)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The collections `did` holds records in.
    pub fn collections(&self, did: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut q = conn.prepare(COLLECTIONS)?;
        let rows = q.query_map([did], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Moves `did`'s repo from `prev` to `head`, with `changes` and
    /// `events`, unless another commit moved it first: none then, and
    /// nothing changes. Each record's blobs are linked to it and made
    /// permanent; the blobs no record references any more are forgotten,
    /// and answered, for their files to go.
    pub fn commit(
        &self,
        did: &str,
        prev: &Cid,
        head: &Head,
        changes: &[Change],
        events: &[Event],
    ) -> Result<Option<Vec<Cid>>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let moved = tx.execute(
            "UPDATE repo SET cid = ?1, rev = ?2, commit_block = ?3 WHERE did = ?4 AND cid = ?5",
            params![
                head.cid.to_string(),
                head.rev,
                head.block,
                did,
                prev.to_string()
            ],
        );
        if moved? == 0 {
            return Ok(None);
        }
        let mut unlinked = Vec::new();
        for Change {
            collection,
            rkey,
            put,
            blobs,
        } in changes
        {
            unlinked.extend(linked(&tx, did, collection, rkey)?);
            let cleared = tx.execute(UNLINK, params![did, collection, rkey]);
            cleared?;
            for cid in blobs {
                let link = tx.execute(LINK, params![did, collection, rkey, cid.to_string()]);
                link?;
                let kept = tx.execute(KEEP_BLOB, params![did, cid.to_string()]);
                kept?;
            }
            let done = match put {
                Some((cid, value)) => tx.execute(
                    PUT_RECORD,
                    params![did, collection, rkey, cid.to_string(), value, head.rev],
                ),
                None => tx.execute(
                    "DELETE FROM record WHERE did = ?1 AND collection = ?2 AND rkey = ?3",
                    params![did, collection, rkey],
                ),
            };
            done?;
        }
        let mut orphans = Vec::new();
        for cid in unlinked {
            let gone = tx.execute(FORGET_ORPHAN, params![did, cid.to_string()]);
            if gone? > 0 {
                orphans.push(cid);
            }
        }
        sequence(&tx, did, events)?;
        tx.commit()?;
        Ok(Some(orphans))
    }

    /// Keeps a blob of `did`'s, temporary until a record references it; one
    /// kept already keeps its standing.
    pub fn put_blob(
        &self,
        did: &str,
        cid: &Cid,
        mime_type: &str,
        size: i64,
        now: i64,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            PUT_BLOB,
            params![did, cid.to_string(), mime_type, size, now],
        )?;
        Ok(())
    }

    /// `did`'s blob `cid`.
    pub fn blob(&self, did: &str, cid: &Cid) -> Result<Option<Blob>> {
        let conn = self.conn();
        let found = conn.query_row(GET_BLOB, params![did, cid.to_string()], |r| {
            Ok(Blob {
                mime_type: r.get(0)?,
                size: r.get(1)?,
                temporary: r.get(2)?,
            })
        });
        Ok(found.optional()?)
    }

    /// The blobs `did`'s records reference, in CID order after `cursor`,
    /// at most `limit`; with `since`, of records written after that rev.
    pub fn list_blobs(
        &self,
        did: &str,
        since: Option<&str>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Cid>> {
        let conn = self.conn();
        let mut q = conn.prepare(LIST_BLOBS)?;
        let asked = params![did, since.unwrap_or(""), cursor.unwrap_or(""), limit];
        let rows = q.query_map(asked, |r| Ok(cid(r.get(0)?)));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Forgets the temporary blobs made before `cutoff`, answering them,
    /// for their files to go.
    pub fn sweep_blobs(&self, cutoff: i64) -> Result<Vec<(String, Cid)>> {
        let conn = self.conn();
        let mut q = conn.prepare(SWEEP_BLOBS)?;
        let rows = q.query_map([cutoff], |r| Ok((r.get(0)?, cid(r.get(1)?))));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Up to `limit` events after `cursor`, in order.
    pub fn events_after(&self, cursor: i64, limit: u32) -> Result<Vec<Sequenced>> {
        let conn = self.conn();
        let mut q = conn.prepare(EVENTS)?;
        let rows = q.query_map(params![cursor, limit], |r| {
            Ok(Sequenced {
                seq: r.get(0)?,
                kind: r.get(1)?,
                body: r.get(2)?,
            })
        });
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The last event's `seq`, 0 before any.
    pub fn last_seq(&self) -> Result<i64> {
        let conn = self.conn();
        Ok(conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM event", [], |r| r.get(0))?)
    }
}
