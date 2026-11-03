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
const MIGRATIONS: &[&str] = &["
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
"];

const NEW_ACCOUNT: &str = "INSERT INTO account (did, handle, signing_key, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (handle) DO NOTHING";
const PUT_RECORD: &str = "INSERT INTO record (did, collection, rkey, cid, value, rev) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (did, collection, rkey) DO UPDATE SET cid = ?4, value = ?5, rev = ?6";
const LIST_UP: &str = "SELECT collection, rkey, cid, value FROM record WHERE did = ?1 AND collection = ?2 AND rkey > ?3 ORDER BY rkey LIMIT ?4";
const LIST_DOWN: &str = "SELECT collection, rkey, cid, value FROM record WHERE did = ?1 AND collection = ?2 AND rkey < ?3 ORDER BY rkey DESC LIMIT ?4";
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

/// What a commit changes: a record put, or taken out (`None`).
pub type Change = (String, String, Option<(Cid, Vec<u8>)>);

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

    /// Keeps a new account, with its first commit. `false` if its handle
    /// is taken.
    pub fn create_account(&self, account: &Account, head: &Head, now: i64) -> Result<bool> {
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

    /// Moves `did`'s repo from `prev` to `head`, with `changes`, unless
    /// another commit moved it first: `false` then, and nothing changes.
    pub fn commit(&self, did: &str, prev: &Cid, head: &Head, changes: &[Change]) -> Result<bool> {
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
            return Ok(false);
        }
        for (collection, rkey, put) in changes {
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
        tx.commit()?;
        Ok(true)
    }
}
