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
pub const MIGRATIONS: &[&str] = &[
    "
    CREATE TABLE account (
        did TEXT PRIMARY KEY,
        handle TEXT NOT NULL UNIQUE,
        signing_key BLOB NOT NULL,
        kem_key BLOB NOT NULL,
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
    "
    CREATE TABLE space_repo (
        did TEXT NOT NULL REFERENCES account (did),
        space TEXT NOT NULL,
        rev TEXT NOT NULL,
        state BLOB NOT NULL,
        PRIMARY KEY (did, space)
    );
    CREATE TABLE space_record (
        did TEXT NOT NULL,
        space TEXT NOT NULL,
        path TEXT NOT NULL,
        cid TEXT NOT NULL,
        value BLOB NOT NULL,
        rev TEXT NOT NULL,
        PRIMARY KEY (did, space, path)
    );
    CREATE TABLE space_op (
        did TEXT NOT NULL,
        space TEXT NOT NULL,
        rev TEXT NOT NULL,
        idx INTEGER NOT NULL,
        path TEXT NOT NULL,
        cid TEXT,
        prev TEXT,
        PRIMARY KEY (did, space, rev, idx)
    );
    CREATE TABLE space_record_blob (
        did TEXT NOT NULL,
        space TEXT NOT NULL,
        path TEXT NOT NULL,
        cid TEXT NOT NULL,
        PRIMARY KEY (did, space, path, cid)
    );
    CREATE INDEX space_record_blob_cid ON space_record_blob (did, cid);
    CREATE TABLE revoked_credential (
        space TEXT NOT NULL,
        jti TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        PRIMARY KEY (space, jti)
    );
",
    "
    CREATE TABLE space (
        uri TEXT PRIMARY KEY,
        authority TEXT NOT NULL REFERENCES account (did),
        public_read INTEGER NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE space_grant (
        space TEXT NOT NULL REFERENCES space (uri),
        did TEXT NOT NULL,
        read INTEGER NOT NULL,
        write INTEGER NOT NULL,
        PRIMARY KEY (space, did)
    );
    CREATE TABLE space_writer (
        space TEXT NOT NULL REFERENCES space (uri),
        did TEXT NOT NULL,
        repo_rev TEXT NOT NULL,
        data BLOB NOT NULL,
        space_rev TEXT NOT NULL,
        PRIMARY KEY (space, did)
    );
    CREATE INDEX space_writer_rev ON space_writer (space, space_rev);
    CREATE TABLE space_recipient (
        space TEXT NOT NULL REFERENCES space (uri),
        service TEXT NOT NULL,
        endpoint TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        PRIMARY KEY (space, service)
    );
    CREATE TABLE spent_token (
        iss TEXT NOT NULL,
        jti TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        PRIMARY KEY (iss, jti)
    );
    CREATE TABLE outbox (
        repo TEXT NOT NULL,
        space TEXT NOT NULL,
        rev TEXT NOT NULL,
        data BLOB NOT NULL,
        attempts INTEGER NOT NULL,
        next_at INTEGER NOT NULL,
        deliver_by INTEGER NOT NULL,
        PRIMARY KEY (repo, space)
    );
    CREATE INDEX outbox_due ON outbox (next_at);
",
    "
    CREATE TABLE deactivation (
        did TEXT PRIMARY KEY REFERENCES account (did),
        at INTEGER NOT NULL,
        by TEXT NOT NULL,
        stewards TEXT NOT NULL
    );
",
    "
    CREATE TABLE address (
        did TEXT PRIMARY KEY REFERENCES account (did),
        address TEXT NOT NULL UNIQUE
    );
",
    "
    CREATE TABLE index_repo (
        space TEXT NOT NULL,
        did TEXT NOT NULL,
        rev TEXT NOT NULL,
        state BLOB NOT NULL,
        PRIMARY KEY (space, did)
    );
    CREATE TABLE index_record (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        space TEXT NOT NULL,
        did TEXT NOT NULL,
        path TEXT NOT NULL,
        cid TEXT NOT NULL,
        value BLOB NOT NULL,
        indexed_at INTEGER NOT NULL,
        UNIQUE (space, did, path)
    );
    CREATE INDEX index_record_newest ON index_record (space, seq);
",
    // Accounts made before addresses were kept: each its handle then.
    "
    INSERT OR IGNORE INTO address (did, address) SELECT did, handle FROM account;
",
    "
    CREATE TABLE issued_credential (
        space TEXT NOT NULL,
        did TEXT NOT NULL,
        jti TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        PRIMARY KEY (space, jti)
    );
    CREATE INDEX issued_credential_to ON issued_credential (did);
",
    "
    CREATE TABLE join_request (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        grp TEXT NOT NULL REFERENCES account (did),
        did TEXT NOT NULL,
        message TEXT,
        created_at INTEGER NOT NULL,
        UNIQUE (grp, did)
    );
",
    "
    CREATE TABLE invite (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        grp TEXT NOT NULL REFERENCES account (did),
        invitee TEXT NOT NULL,
        by TEXT NOT NULL,
        message TEXT,
        rkey TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        expires_at INTEGER,
        UNIQUE (grp, invitee)
    );
    CREATE INDEX invite_to ON invite (invitee);
",
    "
    CREATE TABLE oauth_request (
        id TEXT PRIMARY KEY,
        client_id TEXT NOT NULL,
        client_auth TEXT NOT NULL,
        parameters TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        grp TEXT,
        member TEXT,
        scope TEXT,
        code TEXT UNIQUE
    );
    CREATE TABLE oauth_token (
        id TEXT PRIMARY KEY,
        grp TEXT NOT NULL REFERENCES account (did),
        member TEXT NOT NULL,
        client_id TEXT NOT NULL,
        client_auth TEXT NOT NULL,
        scope TEXT NOT NULL,
        dpop_jkt TEXT NOT NULL,
        access_hash TEXT NOT NULL UNIQUE,
        access_expires INTEGER NOT NULL,
        refresh_hash TEXT NOT NULL UNIQUE,
        expires_at INTEGER NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX oauth_token_member ON oauth_token (grp, member);
",
    "
    ALTER TABLE oauth_token ADD COLUMN source TEXT NOT NULL DEFAULT 'oauth';
    ALTER TABLE oauth_token ADD COLUMN used_at INTEGER;
    CREATE TABLE group_activity (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        grp TEXT NOT NULL REFERENCES account (did),
        member TEXT NOT NULL,
        client_id TEXT,
        action TEXT NOT NULL,
        at INTEGER NOT NULL
    );
    CREATE INDEX group_activity_grp ON group_activity (grp, seq);
",
    "
    CREATE TABLE app_password (
        grp TEXT NOT NULL REFERENCES account (did),
        name TEXT NOT NULL,
        member TEXT NOT NULL,
        hash TEXT NOT NULL UNIQUE,
        privileged INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (grp, name)
    );
",
    "
    CREATE TABLE console_login (
        state TEXT PRIMARY KEY,
        did TEXT NOT NULL,
        issuer TEXT NOT NULL,
        token_endpoint TEXT NOT NULL,
        revocation_endpoint TEXT,
        key BLOB NOT NULL,
        verifier TEXT NOT NULL,
        back TEXT NOT NULL,
        started INTEGER NOT NULL
    );
    CREATE TABLE console_logout (
        did TEXT PRIMARY KEY,
        at INTEGER NOT NULL
    );
",
    "
    CREATE TABLE mod_subject (
        grp TEXT NOT NULL REFERENCES account (did),
        subject TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (grp, subject)
    );
    CREATE INDEX mod_subject_updated ON mod_subject (grp, updated_at);
    CREATE TABLE mod_event (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        grp TEXT NOT NULL REFERENCES account (did),
        subject TEXT NOT NULL,
        kind TEXT NOT NULL,
        actor TEXT,
        reason_type TEXT,
        reason TEXT,
        val TEXT,
        note TEXT,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX mod_event_subject ON mod_event (grp, subject, seq);
",
    "
    CREATE TABLE recovery_key (
        grp TEXT NOT NULL REFERENCES account (did),
        key TEXT NOT NULL,
        added_by TEXT NOT NULL,
        added_at INTEGER NOT NULL,
        PRIMARY KEY (grp, key)
    );
",
    "
    CREATE TABLE migration_export (
        did TEXT PRIMARY KEY REFERENCES account (did),
        token_hash TEXT NOT NULL,
        expires_at INTEGER NOT NULL
    );
",
    "
    CREATE TABLE account_import (
        did TEXT PRIMARY KEY REFERENCES account (did),
        founder TEXT NOT NULL,
        display_name TEXT NOT NULL,
        description TEXT,
        created_at INTEGER NOT NULL
    );
",
    "
    CREATE TABLE assistant_client (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        redirect_uris TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE assistant_request (
        id TEXT PRIMARY KEY,
        client_id TEXT NOT NULL,
        client_name TEXT NOT NULL,
        redirect_uri TEXT NOT NULL,
        state TEXT,
        code_challenge TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        grp TEXT REFERENCES account (did),
        member TEXT,
        code_hash TEXT UNIQUE
    );
    CREATE TABLE assistant_grant (
        id TEXT PRIMARY KEY,
        grp TEXT NOT NULL REFERENCES account (did),
        member TEXT NOT NULL,
        client_id TEXT NOT NULL,
        client_name TEXT NOT NULL,
        access_hash TEXT NOT NULL UNIQUE,
        access_expires INTEGER NOT NULL,
        refresh_hash TEXT NOT NULL UNIQUE,
        expires_at INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        used_at INTEGER NOT NULL
    );
    CREATE INDEX assistant_grant_member ON assistant_grant (grp, member);
",
    "
    ALTER TABLE console_login ADD COLUMN mover TEXT;
    ALTER TABLE console_login ADD COLUMN pds TEXT NOT NULL DEFAULT '';
    CREATE TABLE account_move (
        mover TEXT PRIMARY KEY,
        did TEXT NOT NULL,
        pds TEXT NOT NULL,
        issuer TEXT NOT NULL,
        token_endpoint TEXT NOT NULL,
        revocation_endpoint TEXT,
        key BLOB NOT NULL,
        access TEXT NOT NULL,
        refresh TEXT,
        expires_at INTEGER NOT NULL,
        started INTEGER NOT NULL
    );
",
    "
    ALTER TABLE assistant_client ADD COLUMN connected INTEGER NOT NULL DEFAULT 0;
    UPDATE assistant_client SET connected = 1
        WHERE id IN (SELECT client_id FROM assistant_grant);
",
    // A moderation event says which version of a record it is about, the
    // rule a report cites and the label record written; resolving,
    // escalating and noting are each a status set, a note beside it.
    "
    ALTER TABLE mod_event ADD COLUMN cid TEXT;
    ALTER TABLE mod_event ADD COLUMN rule TEXT;
    ALTER TABLE mod_event ADD COLUMN uri TEXT;
    ALTER TABLE mod_event ADD COLUMN status TEXT;
    UPDATE mod_event SET kind = 'status', status = 'resolved' WHERE kind = 'resolve';
    UPDATE mod_event SET kind = 'status', status = 'escalated' WHERE kind = 'escalate';
    UPDATE mod_event SET kind = 'status', status = coalesce((SELECT p.status FROM mod_event p
        WHERE p.grp = mod_event.grp AND p.subject = mod_event.subject AND p.seq < mod_event.seq
        AND p.status IS NOT NULL ORDER BY p.seq DESC LIMIT 1), 'open') WHERE kind = 'note';
",
    // The MCP server's clients, requests and grants, named for it.
    "
    ALTER TABLE assistant_client RENAME TO mcp_client;
    ALTER TABLE assistant_request RENAME TO mcp_request;
    ALTER TABLE assistant_grant RENAME TO mcp_grant;
    DROP INDEX IF EXISTS assistant_grant_member;
    CREATE INDEX IF NOT EXISTS mcp_grant_member ON mcp_grant (grp, member);
",
];

const NEW_ACCOUNT: &str = "INSERT INTO account (did, handle, signing_key, kem_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (handle) DO NOTHING";
const PUT_RECORD: &str = "INSERT INTO record (did, collection, rkey, cid, value, rev) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (did, collection, rkey) DO UPDATE SET cid = ?4, value = ?5, rev = ?6";
const LIST_UP: &str = "SELECT collection, rkey, cid, value FROM record WHERE did = ?1 AND collection = ?2 AND rkey > ?3 ORDER BY rkey LIMIT ?4";
const LIST_DOWN: &str = "SELECT collection, rkey, cid, value FROM record WHERE did = ?1 AND collection = ?2 AND rkey < ?3 ORDER BY rkey DESC LIMIT ?4";
const LINKED: &str = "SELECT cid FROM record_blob WHERE did = ?1 AND collection = ?2 AND rkey = ?3";
const UNLINK: &str = "DELETE FROM record_blob WHERE did = ?1 AND collection = ?2 AND rkey = ?3";
const LINK: &str =
    "INSERT OR IGNORE INTO record_blob (did, collection, rkey, cid) VALUES (?1, ?2, ?3, ?4)";
const KEEP_BLOB: &str = "UPDATE blob SET temporary = 0 WHERE did = ?1 AND cid = ?2";
const FORGET_ORPHAN: &str = "DELETE FROM blob WHERE did = ?1 AND cid = ?2 AND temporary = 0 AND NOT EXISTS (SELECT 1 FROM record_blob WHERE did = ?1 AND cid = ?2) AND NOT EXISTS (SELECT 1 FROM space_record_blob WHERE did = ?1 AND cid = ?2)";
const PUT_BLOB: &str = "INSERT INTO blob (did, cid, mime_type, size, temporary, created_at) VALUES (?1, ?2, ?3, ?4, 1, ?5) ON CONFLICT (did, cid) DO UPDATE SET mime_type = excluded.mime_type";
const GET_BLOB: &str = "SELECT mime_type, size, temporary FROM blob WHERE did = ?1 AND cid = ?2";
const LIST_BLOBS: &str = "SELECT DISTINCT rb.cid FROM record_blob rb JOIN record r ON r.did = rb.did AND r.collection = rb.collection AND r.rkey = rb.rkey WHERE rb.did = ?1 AND r.rev > ?2 AND rb.cid > ?3 ORDER BY rb.cid LIMIT ?4";
const SWEEP_BLOBS: &str =
    "DELETE FROM blob WHERE temporary = 1 AND created_at < ?1 RETURNING did, cid";
const SEQUENCE: &str = "INSERT INTO event (did, kind, body) VALUES (?1, ?2, ?3)";
const EVENTS: &str = "SELECT seq, kind, body FROM event WHERE seq > ?1 ORDER BY seq LIMIT ?2";
const ACCOUNTS: &str =
    "SELECT did, cid, rev, commit_block FROM repo WHERE did > ?1 ORDER BY did LIMIT ?2";
const SPACE_HEAD: &str = "SELECT rev, state FROM space_repo WHERE did = ?1 AND space = ?2";
const SPACE_START: &str = "INSERT INTO space_repo (did, space, rev, state) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (did, space) DO NOTHING";
const SPACE_MOVE: &str =
    "UPDATE space_repo SET rev = ?3, state = ?4 WHERE did = ?1 AND space = ?2 AND rev = ?5";
const SPACE_PATHS: &str = "SELECT path, cid FROM space_record WHERE did = ?1 AND space = ?2";
const SPACE_CID: &str = "SELECT cid FROM space_record WHERE did = ?1 AND space = ?2 AND path = ?3";
const SPACE_PUT: &str = "INSERT INTO space_record (did, space, path, cid, value, rev) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (did, space, path) DO UPDATE SET cid = ?4, value = ?5, rev = ?6";
const SPACE_DELETE: &str = "DELETE FROM space_record WHERE did = ?1 AND space = ?2 AND path = ?3";
const SPACE_OP: &str = "INSERT INTO space_op (did, space, rev, idx, path, cid, prev) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";
const SPACE_LINKED: &str =
    "SELECT cid FROM space_record_blob WHERE did = ?1 AND space = ?2 AND path = ?3";
const SPACE_UNLINK: &str =
    "DELETE FROM space_record_blob WHERE did = ?1 AND space = ?2 AND path = ?3";
const SPACE_LINK: &str =
    "INSERT OR IGNORE INTO space_record_blob (did, space, path, cid) VALUES (?1, ?2, ?3, ?4)";
const SPACE_RECORD: &str =
    "SELECT path, cid, value FROM space_record WHERE did = ?1 AND space = ?2 AND path = ?3";
const SPACE_RECORDS: &str =
    "SELECT path, cid, value FROM space_record WHERE did = ?1 AND space = ?2 ORDER BY path";
// A collection's records are those whose path starts `<collection>/`.
const SPACE_LIST_UP: &str = "SELECT path, cid, value FROM space_record WHERE did = ?1 AND space = ?2 AND (?3 IS NULL OR substr(path, 1, length(?3) + 1) = ?3 || '/') AND path > ?4 ORDER BY path LIMIT ?5";
const SPACE_LIST_DOWN: &str = "SELECT path, cid, value FROM space_record WHERE did = ?1 AND space = ?2 AND (?3 IS NULL OR substr(path, 1, length(?3) + 1) = ?3 || '/') AND path < ?4 ORDER BY path DESC LIMIT ?5";
// An op's value is its record's, while the record is still what it made.
const SPACE_OPS: &str = "SELECT o.rev, o.idx, o.path, o.cid, o.prev, r.value FROM space_op o LEFT JOIN space_record r ON r.did = o.did AND r.space = o.space AND r.path = o.path AND r.cid = o.cid WHERE o.did = ?1 AND o.space = ?2 AND (o.rev > ?3 OR (o.rev = ?3 AND o.idx > ?4)) ORDER BY o.rev, o.idx LIMIT ?5";
const SPACE_BLOBS: &str = "SELECT DISTINCT rb.cid FROM space_record_blob rb JOIN space_record r ON r.did = rb.did AND r.space = rb.space AND r.path = rb.path WHERE rb.did = ?1 AND rb.space = ?2 AND r.rev > ?3 AND rb.cid > ?4 ORDER BY rb.cid LIMIT ?5";
const SPACE_HAS_BLOB: &str =
    "SELECT 1 FROM space_record_blob WHERE did = ?1 AND space = ?2 AND cid = ?3 LIMIT 1";
const REVOKE: &str =
    "INSERT OR IGNORE INTO revoked_credential (space, jti, expires_at) VALUES (?1, ?2, ?3)";
const FORGET_REVOKED: &str = "DELETE FROM revoked_credential WHERE expires_at <= ?1";
const REVOKED: &str =
    "SELECT 1 FROM revoked_credential WHERE space = ?1 AND jti = ?2 AND expires_at > ?3";
const NEW_SPACE: &str = "INSERT INTO space (uri, authority, public_read, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (uri) DO NOTHING";
const SPACE: &str = "SELECT authority, public_read FROM space WHERE uri = ?1";
const PUT_GRANT: &str = "INSERT INTO space_grant (space, did, read, write) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (space, did) DO UPDATE SET read = ?3, write = ?4";
const GRANT: &str = "SELECT read, write FROM space_grant WHERE space = ?1 AND did = ?2";
const UNGRANT: &str = "DELETE FROM space_grant WHERE space = ?1 AND did = ?2";
const WRITER_REV: &str = "SELECT repo_rev FROM space_writer WHERE space = ?1 AND did = ?2";
const LAST_SPACE_REV: &str = "SELECT MAX(space_rev) FROM space_writer WHERE space = ?1";
const PUT_WRITER: &str = "INSERT INTO space_writer (space, did, repo_rev, data, space_rev) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (space, did) DO UPDATE SET repo_rev = ?3, data = ?4, space_rev = ?5";
const WRITERS: &str = "SELECT did, repo_rev, data, space_rev FROM space_writer WHERE space = ?1 AND space_rev > ?2 ORDER BY space_rev LIMIT ?3";
const PUT_RECIPIENT: &str = "INSERT INTO space_recipient (space, service, endpoint, expires_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (space, service) DO UPDATE SET endpoint = ?3, expires_at = ?4";
const RECIPIENTS: &str = "SELECT service, endpoint FROM space_recipient WHERE space = ?1 AND expires_at > ?2 ORDER BY service";
const UNREGISTER: &str = "DELETE FROM space_recipient WHERE space = ?1 AND service = ?2";
const FORGET_SPENT: &str = "DELETE FROM spent_token WHERE expires_at <= ?1";
const SPEND: &str = "INSERT OR IGNORE INTO spent_token (iss, jti, expires_at) VALUES (?1, ?2, ?3)";
const OWE: &str = "INSERT INTO outbox (repo, space, rev, data, attempts, next_at, deliver_by) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6) ON CONFLICT (repo, space) DO UPDATE SET rev = ?3, data = ?4, attempts = 0, next_at = ?5, deliver_by = ?6";
const DUE: &str = "SELECT repo, space, rev, data, attempts FROM outbox WHERE next_at <= ?1 ORDER BY next_at LIMIT ?2";
const DELIVERED: &str = "DELETE FROM outbox WHERE repo = ?1 AND space = ?2 AND rev = ?3";
const RETRY: &str = "UPDATE outbox SET attempts = attempts + 1, next_at = ?4 WHERE repo = ?1 AND space = ?2 AND rev = ?3";
const LAPSED: &str = "DELETE FROM outbox WHERE deliver_by <= ?1 RETURNING repo, space, rev";
/// What goes with a space whose authority is here.
const FORGET_SPACE: &[&str] = &[
    "DELETE FROM index_record WHERE space = ?1",
    "DELETE FROM index_repo WHERE space = ?1",
    "DELETE FROM space_recipient WHERE space = ?1",
    "DELETE FROM space_writer WHERE space = ?1",
    "DELETE FROM space_grant WHERE space = ?1",
    "DELETE FROM issued_credential WHERE space = ?1",
    "DELETE FROM space WHERE uri = ?1",
];
/// And the authority's own repo in it.
const FORGET_OWN_REPO: &[&str] = &[
    "DELETE FROM space_op WHERE space = ?1 AND did = ?2",
    "DELETE FROM space_record WHERE space = ?1 AND did = ?2",
    "DELETE FROM space_repo WHERE space = ?1 AND did = ?2",
];
const INDEXED_STATE: &str = "SELECT rev, state FROM index_repo WHERE space = ?1 AND did = ?2";
const INDEX_STATE: &str = "INSERT INTO index_repo (space, did, rev, state) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (space, did) DO UPDATE SET rev = ?3, state = ?4";
const INDEXED_PATHS: &str = "SELECT path, cid FROM index_record WHERE space = ?1 AND did = ?2";
const UNINDEX: &str = "DELETE FROM index_record WHERE space = ?1 AND did = ?2 AND path = ?3";
const INDEX: &str = "INSERT INTO index_record (space, did, path, cid, value, indexed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)";
const INDEXED: &str = "SELECT seq, did, path, cid, value, indexed_at FROM index_record WHERE space = ?1 AND (?2 IS NULL OR substr(path, 1, length(?2) + 1) = ?2 || '/') AND seq < ?3 ORDER BY seq DESC LIMIT ?4";
const INDEXED_ONE: &str = "SELECT seq, did, path, cid, value, indexed_at FROM index_record WHERE space = ?1 AND did = ?2 AND path = ?3";
const ISSUE: &str =
    "INSERT INTO issued_credential (space, did, jti, expires_at) VALUES (?1, ?2, ?3, ?4)";
const FORGET_ISSUED: &str = "DELETE FROM issued_credential WHERE expires_at <= ?1";
const ISSUED_TO: &str = "SELECT space, jti, expires_at FROM issued_credential WHERE did = ?1 AND expires_at > ?2 AND space IN (SELECT uri FROM space WHERE authority = ?3) ORDER BY space";
const UNINDEX_WRITER: &str = "DELETE FROM index_record WHERE space = ?1 AND did = ?2";
const UNINDEX_STATE: &str = "DELETE FROM index_repo WHERE space = ?1 AND did = ?2";
const ASK: &str = "INSERT INTO join_request (grp, did, message, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (grp, did) DO NOTHING";
const ASKED: &str =
    "SELECT seq, did, message, created_at FROM join_request WHERE grp = ?1 AND did = ?2";
const ASKING: &str = "SELECT seq, did, message, created_at FROM join_request WHERE grp = ?1 AND seq < ?2 ORDER BY seq DESC LIMIT ?3";
const DECIDE: &str = "DELETE FROM join_request WHERE grp = ?1 AND did = ?2";
const INVITE: &str = "INSERT INTO invite (grp, invitee, by, message, rkey, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";
const INVITED: &str = "SELECT seq, grp, invitee, by, message, rkey, created_at, expires_at FROM invite WHERE grp = ?1 AND invitee = ?2";
const INVITES: &str = "SELECT seq, grp, invitee, by, message, rkey, created_at, expires_at FROM invite WHERE grp = ?1 AND seq < ?2 AND (expires_at IS NULL OR expires_at > ?3) ORDER BY seq DESC LIMIT ?4";
const INVITES_TO: &str = "SELECT seq, grp, invitee, by, message, rkey, created_at, expires_at FROM invite WHERE invitee = ?1 AND (expires_at IS NULL OR expires_at > ?2) ORDER BY seq DESC";
const LAPSED_INVITES: &str = "SELECT seq, grp, invitee, by, message, rkey, created_at, expires_at FROM invite WHERE expires_at <= ?1 ORDER BY seq";
const UNINVITE: &str = "DELETE FROM invite WHERE grp = ?1 AND invitee = ?2";
const SPACES_OF: &str = "SELECT uri FROM space WHERE authority = ?1 ORDER BY uri";
const DEACTIVATE: &str = "INSERT INTO deactivation (did, at, by, stewards) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (did) DO NOTHING";
const DEACTIVATION: &str = "SELECT at, by, stewards FROM deactivation WHERE did = ?1";
const ACTIVATE: &str = "DELETE FROM deactivation WHERE did = ?1";
/// What goes with an account, in an order its references allow.
const FORGET_ACCOUNT: &[&str] = &[
    "DELETE FROM mcp_grant WHERE grp = ?1",
    "DELETE FROM mcp_request WHERE grp = ?1",
    "DELETE FROM account_import WHERE did = ?1",
    "DELETE FROM migration_export WHERE did = ?1",
    "DELETE FROM recovery_key WHERE grp = ?1",
    "DELETE FROM mod_event WHERE grp = ?1",
    "DELETE FROM mod_subject WHERE grp = ?1",
    "DELETE FROM app_password WHERE grp = ?1",
    "DELETE FROM group_activity WHERE grp = ?1",
    "DELETE FROM oauth_token WHERE grp = ?1",
    "DELETE FROM oauth_request WHERE grp = ?1",
    "DELETE FROM invite WHERE grp = ?1",
    "DELETE FROM join_request WHERE grp = ?1",
    "DELETE FROM issued_credential WHERE space IN (SELECT uri FROM space WHERE authority = ?1)",
    "DELETE FROM index_record WHERE space IN (SELECT uri FROM space WHERE authority = ?1)",
    "DELETE FROM index_repo WHERE space IN (SELECT uri FROM space WHERE authority = ?1)",
    "DELETE FROM deactivation WHERE did = ?1",
    "DELETE FROM address WHERE did = ?1",
    "DELETE FROM outbox WHERE repo = ?1",
    "DELETE FROM space_recipient WHERE space IN (SELECT uri FROM space WHERE authority = ?1)",
    "DELETE FROM space_writer WHERE space IN (SELECT uri FROM space WHERE authority = ?1)",
    "DELETE FROM space_grant WHERE space IN (SELECT uri FROM space WHERE authority = ?1)",
    "DELETE FROM space WHERE authority = ?1",
    "DELETE FROM space_record_blob WHERE did = ?1",
    "DELETE FROM space_op WHERE did = ?1",
    "DELETE FROM space_record WHERE did = ?1",
    "DELETE FROM space_repo WHERE did = ?1",
    "DELETE FROM record_blob WHERE did = ?1",
    "DELETE FROM record WHERE did = ?1",
    "DELETE FROM repo WHERE did = ?1",
];
const FORGET_BLOBS: &str = "DELETE FROM blob WHERE did = ?1 RETURNING cid";
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
    /// The P-256 key a space's syncer agrees a secret with, to have this
    /// account's repos in spaces vouched for (`#atproto_kem`).
    pub kem: Keypair,
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

pub(crate) fn sequence(tx: &rusqlite::Transaction, did: &str, events: &[Event]) -> Result<()> {
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

/// A repo's state in a space: its revision, and its records' set hash
/// (`atproto::space::LtHash::state`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceHead {
    pub rev: String,
    pub state: Vec<u8>,
}

/// A change made to a repo in a space, in the order made: its revision
/// and place in it, the path, the record's CID after (none for a delete)
/// and before (none for a create), and its value, while the record is
/// still as the change left it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceOp {
    pub rev: String,
    pub idx: i64,
    pub collection: String,
    pub rkey: String,
    pub cid: Option<Cid>,
    pub prev: Option<Cid>,
    pub value: Option<Vec<u8>>,
}

/// A space whose authority is an account here: the authority, and
/// whether anyone may read it.
#[derive(Debug, Clone, PartialEq)]
pub struct Space {
    pub authority: String,
    pub public_read: bool,
}

/// A repo's last write in a space, as its authority knows it: its
/// revision and digest, and the space's revision when it was told.
#[derive(Debug, Clone, PartialEq)]
pub struct Writer {
    pub did: String,
    pub repo_rev: String,
    pub data: Vec<u8>,
    pub space_rev: String,
}

/// A write an authority elsewhere is owed word of: the repo, the space,
/// the revision and digest, and how often telling it has failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub repo: String,
    pub space: String,
    pub rev: String,
    pub data: Vec<u8>,
    pub attempts: i64,
}

/// An account switched off: when, by whom, and who may switch it back on,
/// as they were when it was switched off.
#[derive(Debug, Clone, PartialEq)]
pub struct Deactivation {
    pub at: i64,
    pub by: String,
    pub stewards: Vec<String>,
}

/// A record in the host's index of a space: when it was indexed, in
/// order, whose it is, where, and its CID and DRISL.
#[derive(Debug, Clone, PartialEq)]
pub struct Indexed {
    pub seq: i64,
    pub did: String,
    pub collection: String,
    pub rkey: String,
    pub cid: Cid,
    pub value: Vec<u8>,
    pub indexed_at: i64,
}

fn indexed(row: &rusqlite::Row) -> rusqlite::Result<Indexed> {
    let (collection, rkey) = split(&row.get::<_, String>(2)?);
    Ok(Indexed {
        seq: row.get(0)?,
        did: row.get(1)?,
        collection,
        rkey,
        cid: cid(row.get(3)?),
        value: row.get(4)?,
        indexed_at: row.get(5)?,
    })
}

/// A request to join a group, while it waits: in order, who asks, what
/// they said, and when.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinRequest {
    pub seq: i64,
    pub did: String,
    pub message: Option<String>,
    pub created_at: i64,
}

fn join_request(row: &rusqlite::Row) -> rusqlite::Result<JoinRequest> {
    Ok(JoinRequest {
        seq: row.get(0)?,
        did: row.get(1)?,
        message: row.get(2)?,
        created_at: row.get(3)?,
    })
}

/// An invite a group sent: in order, the group, whom, who in it sent it,
/// what they said, its record's key in the invitee's inbox, when, and
/// when it lapses, if it does.
#[derive(Debug, Clone, PartialEq)]
pub struct Invite {
    pub seq: i64,
    pub group: String,
    pub invitee: String,
    pub by: String,
    pub message: Option<String>,
    pub rkey: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

fn invite(row: &rusqlite::Row) -> rusqlite::Result<Invite> {
    Ok(Invite {
        seq: row.get(0)?,
        group: row.get(1)?,
        invitee: row.get(2)?,
        by: row.get(3)?,
        message: row.get(4)?,
        rkey: row.get(5)?,
        created_at: row.get(6)?,
        expires_at: row.get(7)?,
    })
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

/// A path the host wrote, as `collection/rkey`.
fn split(path: &str) -> (String, String) {
    let (collection, rkey) = path.split_once('/').expect("a path the host wrote");
    (collection.into(), rkey.into())
}

/// A record in a space, from its path, CID and value.
fn space_record(row: &rusqlite::Row) -> rusqlite::Result<Record> {
    let (collection, rkey) = split(&row.get::<_, String>(0)?);
    Ok(Record {
        collection,
        rkey,
        cid: cid(row.get(1)?),
        value: row.get(2)?,
    })
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

    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
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
                account.kem.to_bytes().to_vec(),
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
        // Its first handle is its address, which stays when it takes
        // another.
        let addressed = tx.execute(
            "INSERT INTO address (did, address) VALUES (?1, ?2)",
            params![account.did, account.handle],
        );
        addressed?;
        sequence(&tx, &account.did, events)?;
        tx.commit()?;
        Ok(true)
    }

    /// The account `did` or `handle` names.
    pub fn account(&self, did_or_handle: &str) -> Result<Option<Account>> {
        let found = self
            .conn()
            .query_row(
                "SELECT did, handle, signing_key, kem_key FROM account WHERE did = ?1 OR handle = lower(?1)",
                [did_or_handle],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )
            .optional()?;
        found
            .map(|(did, handle, signing, kem)| {
                Ok(Account {
                    did,
                    handle,
                    signing: Keypair::from_bytes(Curve::K256, &signing)?,
                    kem: Keypair::from_bytes(Curve::P256, &kem)?,
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

    /// `did`'s repo in `space`, if it has written there.
    pub fn space_head(&self, did: &str, space: &str) -> Result<Option<SpaceHead>> {
        let conn = self.conn();
        let found = conn.query_row(SPACE_HEAD, [did, space], |r| {
            Ok(SpaceHead {
                rev: r.get(0)?,
                state: r.get(1)?,
            })
        });
        Ok(found.optional()?)
    }

    /// Every record of `did`'s in `space`, by path, with its CID.
    pub fn space_paths(&self, did: &str, space: &str) -> Result<BTreeMap<String, Cid>> {
        let conn = self.conn();
        let mut q = conn.prepare(SPACE_PATHS)?;
        let rows = q.query_map([did, space], |r| Ok((r.get(0)?, cid(r.get(1)?))));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Moves `did`'s repo in `space` from `prev` (none for its first
    /// write) to `head`, with `changes`, each kept as an op, unless another
    /// write moved it first: none then, and nothing changes. Blobs are
    /// linked and forgotten as [`Store::commit`] does.
    pub fn space_commit(
        &self,
        did: &str,
        space: &str,
        prev: Option<&str>,
        head: &SpaceHead,
        changes: &[Change],
    ) -> Result<Option<Vec<Cid>>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let moved = match prev {
            None => tx.execute(SPACE_START, params![did, space, head.rev, head.state]),
            Some(prev) => tx.execute(SPACE_MOVE, params![did, space, head.rev, head.state, prev]),
        };
        if moved? == 0 {
            return Ok(None);
        }
        let mut unlinked = Vec::new();
        for (idx, change) in changes.iter().enumerate() {
            let path = format!("{}/{}", change.collection, change.rkey);
            let before = tx.query_row(SPACE_CID, params![did, space, path], |r| {
                r.get::<_, String>(0)
            });
            let before = before.optional()?;
            let mut q = tx.prepare(SPACE_LINKED)?;
            let links = q.query_map(params![did, space, path], |r| Ok(cid(r.get(0)?)));
            unlinked.extend(links?.collect::<rusqlite::Result<Vec<_>>>()?);
            let cleared = tx.execute(SPACE_UNLINK, params![did, space, path]);
            cleared?;
            for blob in &change.blobs {
                let link = tx.execute(SPACE_LINK, params![did, space, path, blob.to_string()]);
                link?;
                let kept = tx.execute(KEEP_BLOB, params![did, blob.to_string()]);
                kept?;
            }
            let after = change.put.as_ref().map(|(cid, _)| cid.to_string());
            let done = match &change.put {
                Some((cid, value)) => tx.execute(
                    SPACE_PUT,
                    params![did, space, path, cid.to_string(), value, head.rev],
                ),
                None => tx.execute(SPACE_DELETE, params![did, space, path]),
            };
            done?;
            let op = tx.execute(
                SPACE_OP,
                params![did, space, head.rev, idx as i64, path, after, before],
            );
            op?;
        }
        let mut orphans = Vec::new();
        for cid in unlinked {
            let gone = tx.execute(FORGET_ORPHAN, params![did, cid.to_string()]);
            if gone? > 0 {
                orphans.push(cid);
            }
        }
        tx.commit()?;
        Ok(Some(orphans))
    }

    /// `did`'s record in `space` at `collection/rkey`.
    pub fn space_record(
        &self,
        did: &str,
        space: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<Option<Record>> {
        let conn = self.conn();
        let path = format!("{collection}/{rkey}");
        let found = conn.query_row(SPACE_RECORD, [did, space, &path], space_record);
        Ok(found.optional()?)
    }

    /// Every record of `did`'s in `space`, by path, as DRISL.
    pub fn space_records(&self, did: &str, space: &str) -> Result<BTreeMap<String, Vec<u8>>> {
        let conn = self.conn();
        let mut q = conn.prepare(SPACE_RECORDS)?;
        let rows = q.query_map([did, space], |r| Ok((r.get(0)?, r.get(2)?)));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// `did`'s records in `space`, of `collection` or all, past `cursor`
    /// (a path), at most `limit`: last path first, or first with `reverse`.
    pub fn space_list(
        &self,
        did: &str,
        space: &str,
        collection: Option<&str>,
        cursor: Option<&str>,
        limit: u32,
        reverse: bool,
    ) -> Result<Vec<Record>> {
        let conn = self.conn();
        let sql = if reverse {
            SPACE_LIST_UP
        } else {
            SPACE_LIST_DOWN
        };
        // No path sorts before "" or after "\u{7f}": its characters are ASCII.
        let from = cursor.unwrap_or(if reverse { "" } else { "\u{7f}" });
        let mut q = conn.prepare(sql)?;
        let rows = q.query_map(params![did, space, collection, from, limit], space_record)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The changes to `did`'s repo in `space` after `since` (a revision)
    /// and `cursor` (an op's revision and place), at most `limit`, with the
    /// repo's state as they leave it, read at one moment.
    pub fn space_ops(
        &self,
        did: &str,
        space: &str,
        after: (&str, i64),
        limit: u32,
    ) -> Result<(Vec<SpaceOp>, Option<SpaceHead>)> {
        let conn = self.conn();
        let mut q = conn.prepare(SPACE_OPS)?;
        let rows = q.query_map(params![did, space, after.0, after.1, limit], |r| {
            let (collection, rkey) = split(&r.get::<_, String>(2)?);
            Ok(SpaceOp {
                rev: r.get(0)?,
                idx: r.get(1)?,
                collection,
                rkey,
                cid: r.get::<_, Option<String>>(3)?.map(cid),
                prev: r.get::<_, Option<String>>(4)?.map(cid),
                value: r.get(5)?,
            })
        });
        let ops = rows?.collect::<rusqlite::Result<_>>()?;
        let head = conn.query_row(SPACE_HEAD, [did, space], |r| {
            Ok(SpaceHead {
                rev: r.get(0)?,
                state: r.get(1)?,
            })
        });
        Ok((ops, head.optional()?))
    }

    /// The blobs `did`'s records in `space` reference, as
    /// [`Store::list_blobs`] lists a repo's.
    pub fn space_blobs(
        &self,
        did: &str,
        space: &str,
        since: Option<&str>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Cid>> {
        let conn = self.conn();
        let mut q = conn.prepare(SPACE_BLOBS)?;
        let asked = params![did, space, since.unwrap_or(""), cursor.unwrap_or(""), limit];
        let rows = q.query_map(asked, |r| Ok(cid(r.get(0)?)));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Whether a record of `did`'s in `space` references blob `cid`.
    pub fn space_has_blob(&self, did: &str, space: &str, cid: &Cid) -> Result<bool> {
        let conn = self.conn();
        let found = conn.query_row(SPACE_HAS_BLOB, [did, space, &cid.to_string()], |_| Ok(()));
        Ok(found.optional()?.is_some())
    }

    /// Keeps the credentials `jtis` to `space` as revoked until
    /// `expires_at` (in seconds), when they would have lapsed anyway; those
    /// lapsed by `now` are forgotten.
    pub fn revoke(&self, space: &str, jtis: &[String], expires_at: i64, now: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let forgot = tx.execute(FORGET_REVOKED, [now]);
        forgot?;
        for jti in jtis {
            let kept = tx.execute(REVOKE, params![space, jti, expires_at]);
            kept?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Whether the credential `jti` to `space` is revoked at `now`.
    pub fn revoked(&self, space: &str, jti: &str, now: i64) -> Result<bool> {
        let conn = self.conn();
        let found = conn.query_row(REVOKED, params![space, jti, now], |_| Ok(()));
        Ok(found.optional()?.is_some())
    }

    /// Keeps a new space of `authority`'s; `false` if there is one at `uri`.
    pub fn create_space(
        &self,
        uri: &str,
        authority: &str,
        public_read: bool,
        now: i64,
    ) -> Result<bool> {
        let conn = self.conn();
        let added = conn.execute(NEW_SPACE, params![uri, authority, public_read, now])?;
        Ok(added > 0)
    }

    /// The space at `uri`, if its authority is here.
    pub fn space(&self, uri: &str) -> Result<Option<Space>> {
        let conn = self.conn();
        let found = conn.query_row(SPACE, [uri], |r| {
            Ok(Space {
                authority: r.get(0)?,
                public_read: r.get(1)?,
            })
        });
        Ok(found.optional()?)
    }

    /// Forgets the space at `uri` and `authority`'s own repo in it. The
    /// blobs only that repo referenced, for their files to go.
    pub fn delete_space(&self, uri: &str, authority: &str) -> Result<Vec<Cid>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let unlinked: Vec<Cid> = {
            let mut q = tx.prepare(
                "DELETE FROM space_record_blob WHERE space = ?1 AND did = ?2 RETURNING cid",
            )?;
            let rows = q.query_map([uri, authority], |r| Ok(cid(r.get(0)?)));
            rows?.collect::<rusqlite::Result<_>>()?
        };
        for sql in FORGET_SPACE {
            let gone = tx.execute(sql, [uri]);
            gone?;
        }
        for sql in FORGET_OWN_REPO {
            let gone = tx.execute(sql, [uri, authority]);
            gone?;
        }
        let mut orphans = Vec::new();
        for cid in unlinked {
            let gone = tx.execute(FORGET_ORPHAN, params![authority, cid.to_string()]);
            if gone? > 0 {
                orphans.push(cid);
            }
        }
        tx.commit()?;
        Ok(orphans)
    }

    /// How far the index has read `did`'s repo in `space`: its revision and
    /// set hash state then.
    pub fn index_state(&self, space: &str, did: &str) -> Result<Option<SpaceHead>> {
        let conn = self.conn();
        let found = conn.query_row(INDEXED_STATE, [space, did], |r| {
            Ok(SpaceHead {
                rev: r.get(0)?,
                state: r.get(1)?,
            })
        });
        Ok(found.optional()?)
    }

    /// Indexes `did`'s repo in `space` as at `head`: its records replaced
    /// by `records`, by path, when it was read whole; or each path in
    /// `changes` put (a CID and DRISL) or taken out, when it was read on.
    pub fn index(
        &self,
        space: &str,
        did: &str,
        head: &SpaceHead,
        whole: bool,
        changes: &BTreeMap<String, Option<(Cid, Vec<u8>)>>,
        now: i64,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        // A record read again as it was keeps its place, and when it was
        // first seen: newest first is by when each came, not when it was
        // last read.
        let held: BTreeMap<String, String> = {
            let mut q = tx.prepare(INDEXED_PATHS)?;
            let rows = q.query_map([space, did], |r| Ok((r.get(0)?, r.get(1)?)));
            rows?.collect::<rusqlite::Result<_>>()?
        };
        if whole {
            for path in held.keys().filter(|p| !changes.contains_key(*p)) {
                let gone = tx.execute(UNINDEX, [space, did, path]);
                gone?;
            }
        }
        for (path, put) in changes {
            let same = put.as_ref().map(|(cid, _)| cid.to_string());
            if same.is_some() && held.get(path) == same.as_ref() {
                continue;
            }
            let gone = tx.execute(UNINDEX, [space, did, path]);
            gone?;
            if let Some((cid, value)) = put {
                let kept = tx.execute(
                    INDEX,
                    params![space, did, path, cid.to_string(), value, now],
                );
                kept?;
            }
        }
        let state = tx.execute(INDEX_STATE, params![space, did, head.rev, head.state]);
        state?;
        tx.commit()?;
        Ok(())
    }

    /// The index's records in `space`, of `collection` or all, newest
    /// first, before `before` (a sequence number), at most `limit`.
    pub fn indexed(
        &self,
        space: &str,
        collection: Option<&str>,
        before: Option<i64>,
        limit: u32,
    ) -> Result<Vec<Indexed>> {
        let conn = self.conn();
        let mut q = conn.prepare(INDEXED)?;
        let asked = params![space, collection, before.unwrap_or(i64::MAX), limit];
        let rows = q.query_map(asked, indexed)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One record in the index of `space`: `did`'s at `path`.
    pub fn indexed_one(&self, space: &str, did: &str, path: &str) -> Result<Option<Indexed>> {
        let conn = self.conn();
        let found = conn.query_row(INDEXED_ONE, [space, did, path], indexed);
        Ok(found.optional()?)
    }

    /// Keeps that the credential `jti` to `space` went to `did`, until it
    /// lapses at `expires_at`; those lapsed by `now` are forgotten.
    pub fn issued(
        &self,
        space: &str,
        did: &str,
        jti: &str,
        expires_at: i64,
        now: i64,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let forgot = tx.execute(FORGET_ISSUED, [now]);
        forgot?;
        let kept = tx.execute(ISSUE, params![space, did, jti, expires_at]);
        kept?;
        tx.commit()?;
        Ok(())
    }

    /// The credentials to `authority`'s spaces still good at `now` that
    /// went to `did`: each space, jti and lapse.
    pub fn issued_to(
        &self,
        authority: &str,
        did: &str,
        now: i64,
    ) -> Result<Vec<(String, String, i64)>> {
        let conn = self.conn();
        let mut q = conn.prepare(ISSUED_TO)?;
        let rows = q.query_map(params![did, now, authority], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        });
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Forgets what the index read of `did`'s repo in `space`.
    pub fn unindex(&self, space: &str, did: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let gone = tx.execute(UNINDEX_WRITER, [space, did]);
        gone?;
        let gone = tx.execute(UNINDEX_STATE, [space, did]);
        gone?;
        tx.commit()?;
        Ok(())
    }

    /// Keeps `did`'s request to join `group`, unless one waits already.
    pub fn ask(&self, group: &str, did: &str, message: Option<&str>, now: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute(ASK, params![group, did, message, now])?;
        Ok(())
    }

    /// `did`'s request to join `group`, while it waits.
    pub fn asked(&self, group: &str, did: &str) -> Result<Option<JoinRequest>> {
        let conn = self.conn();
        let found = conn.query_row(ASKED, [group, did], join_request);
        Ok(found.optional()?)
    }

    /// The requests waiting to join `group`, newest first, before `before`
    /// (a sequence number), at most `limit`.
    pub fn asking(&self, group: &str, before: Option<i64>, limit: u32) -> Result<Vec<JoinRequest>> {
        let conn = self.conn();
        let mut q = conn.prepare(ASKING)?;
        let asked = params![group, before.unwrap_or(i64::MAX), limit];
        let rows = q.query_map(asked, join_request)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Settles `did`'s request to join `group`: `false` if none waited.
    pub fn decide(&self, group: &str, did: &str) -> Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(DECIDE, [group, did])? > 0)
    }

    /// Keeps a group's invite.
    pub fn invite(&self, invite: &Invite) -> Result<()> {
        let conn = self.conn();
        let i = invite;
        let kept = params![
            i.group,
            i.invitee,
            i.by,
            i.message,
            i.rkey,
            i.created_at,
            i.expires_at
        ];
        conn.execute(INVITE, kept)?;
        Ok(())
    }

    /// `group`'s invite to `invitee`, lapsed or not.
    pub fn invited(&self, group: &str, invitee: &str) -> Result<Option<Invite>> {
        let conn = self.conn();
        let found = conn.query_row(INVITED, [group, invitee], invite);
        Ok(found.optional()?)
    }

    /// `group`'s invites not lapsed at `now`, newest first, before
    /// `before`, at most `limit`.
    pub fn invites(
        &self,
        group: &str,
        before: Option<i64>,
        now: i64,
        limit: u32,
    ) -> Result<Vec<Invite>> {
        let conn = self.conn();
        let mut q = conn.prepare(INVITES)?;
        let asked = params![group, before.unwrap_or(i64::MAX), now, limit];
        let rows = q.query_map(asked, invite)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The invites to `invitee` not lapsed at `now`, newest first.
    pub fn invites_to(&self, invitee: &str, now: i64) -> Result<Vec<Invite>> {
        let conn = self.conn();
        let mut q = conn.prepare(INVITES_TO)?;
        let rows = q.query_map(params![invitee, now], invite)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The invites lapsed by `now`, oldest first.
    pub fn lapsed_invites(&self, now: i64) -> Result<Vec<Invite>> {
        let conn = self.conn();
        let mut q = conn.prepare(LAPSED_INVITES)?;
        let rows = q.query_map([now], invite)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Forgets `group`'s invite to `invitee`.
    pub fn uninvite(&self, group: &str, invitee: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(UNINVITE, [group, invitee])?;
        Ok(())
    }

    /// The spaces `authority` is the authority for.
    pub fn spaces_of(&self, authority: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut q = conn.prepare(SPACES_OF)?;
        let rows = q.query_map([authority], |r| r.get(0));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Lets `did` read or write `space`, or both, as given.
    pub fn grant(&self, space: &str, did: &str, read: bool, write: bool) -> Result<()> {
        let conn = self.conn();
        conn.execute(PUT_GRANT, params![space, did, read, write])?;
        Ok(())
    }

    /// What `did` may do in `space`: read, and write.
    pub fn granted(&self, space: &str, did: &str) -> Result<Option<(bool, bool)>> {
        let conn = self.conn();
        let found = conn.query_row(GRANT, [space, did], |r| Ok((r.get(0)?, r.get(1)?)));
        Ok(found.optional()?)
    }

    /// Takes back whatever `did` was let do in `space`.
    pub fn ungrant(&self, space: &str, did: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(UNGRANT, [space, did])?;
        Ok(())
    }

    /// Keeps `did`'s repo in `space` as at `repo_rev` with digest `data`, at
    /// the space's revision `space_rev`, unless it is known at that
    /// revision or a later one: none then. The space's revision before.
    pub fn record_writer(
        &self,
        space: &str,
        did: &str,
        repo_rev: &str,
        data: &[u8],
        space_rev: &str,
    ) -> Result<Option<Option<String>>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let known = tx.query_row(WRITER_REV, [space, did], |r| r.get::<_, String>(0));
        if known.optional()?.is_some_and(|k| k.as_str() >= repo_rev) {
            return Ok(None);
        }
        let before = tx.query_row(LAST_SPACE_REV, [space], |r| r.get::<_, Option<String>>(0))?;
        let kept = tx.execute(PUT_WRITER, params![space, did, repo_rev, data, space_rev]);
        kept?;
        tx.commit()?;
        Ok(Some(before))
    }

    /// The repos written in `space`, in the order the space last heard of
    /// each, after `cursor` (a space revision), at most `limit`.
    pub fn writers(&self, space: &str, cursor: Option<&str>, limit: u32) -> Result<Vec<Writer>> {
        let conn = self.conn();
        let mut q = conn.prepare(WRITERS)?;
        let rows = q.query_map(params![space, cursor.unwrap_or(""), limit], |r| {
            Ok(Writer {
                did: r.get(0)?,
                repo_rev: r.get(1)?,
                data: r.get(2)?,
                space_rev: r.get(3)?,
            })
        });
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Tells `service`, at `endpoint`, of `space`'s writes until
    /// `expires_at` (in seconds).
    pub fn register(
        &self,
        space: &str,
        service: &str,
        endpoint: &str,
        expires_at: i64,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(PUT_RECIPIENT, params![space, service, endpoint, expires_at])?;
        Ok(())
    }

    pub fn unregister(&self, space: &str, service: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(UNREGISTER, [space, service])?;
        Ok(())
    }

    /// The services told of `space`'s writes at `now`, and where.
    pub fn recipients(&self, space: &str, now: i64) -> Result<Vec<(String, String)>> {
        let conn = self.conn();
        let mut q = conn.prepare(RECIPIENTS)?;
        let rows = q.query_map(params![space, now], |r| Ok((r.get(0)?, r.get(1)?)));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Spends `iss`'s token `jti`, good until `expires_at`: `false` if it
    /// was spent already. Those lapsed by `now` are forgotten.
    pub fn spend(&self, iss: &str, jti: &str, expires_at: i64, now: i64) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let forgot = tx.execute(FORGET_SPENT, [now]);
        forgot?;
        let spent = tx.execute(SPEND, params![iss, jti, expires_at])?;
        tx.commit()?;
        Ok(spent > 0)
    }

    /// Owes `space`'s authority word that `repo` is at `rev`, with digest
    /// `data`, from `now` until `deliver_by`: in place of any word of an
    /// earlier revision.
    pub fn owe(&self, notice: &Notice, now: i64, deliver_by: i64) -> Result<()> {
        let conn = self.conn();
        let owed = params![
            notice.repo,
            notice.space,
            notice.rev,
            notice.data,
            now,
            deliver_by
        ];
        conn.execute(OWE, owed)?;
        Ok(())
    }

    /// The notices due at `now`, at most `limit`, the longest due first.
    pub fn due(&self, now: i64, limit: u32) -> Result<Vec<Notice>> {
        let conn = self.conn();
        let mut q = conn.prepare(DUE)?;
        let rows = q.query_map(params![now, limit], |r| {
            Ok(Notice {
                repo: r.get(0)?,
                space: r.get(1)?,
                rev: r.get(2)?,
                data: r.get(3)?,
                attempts: r.get(4)?,
            })
        });
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Owes `notice` no more; one for a later revision stays owed.
    pub fn delivered(&self, notice: &Notice) -> Result<()> {
        let conn = self.conn();
        conn.execute(DELIVERED, [&notice.repo, &notice.space, &notice.rev])?;
        Ok(())
    }

    /// Tries `notice` again at `next_at`.
    pub fn retry(&self, notice: &Notice, next_at: i64) -> Result<()> {
        let conn = self.conn();
        let asked = params![notice.repo, notice.space, notice.rev, next_at];
        conn.execute(RETRY, asked)?;
        Ok(())
    }

    /// Gives up the notices still owed at `now` past their time, answering
    /// each's repo, space and revision.
    pub fn lapse(&self, now: i64) -> Result<Vec<(String, String, String)>> {
        let conn = self.conn();
        let mut q = conn.prepare(LAPSED)?;
        let rows = q.query_map([now], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)));
        let rows = rows?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// `did`'s address: the handle it was made at, under the host's
    /// domain.
    pub fn address(&self, did: &str) -> Result<Option<String>> {
        let conn = self.conn();
        let found = conn.query_row("SELECT address FROM address WHERE did = ?1", [did], |r| {
            r.get(0)
        });
        Ok(found.optional()?)
    }

    /// Gives `did` the handle `handle`, with `events`; refused if another
    /// account has it.
    pub fn set_handle(&self, did: &str, handle: &str, events: &[Event]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let set = tx.execute(
            "UPDATE account SET handle = ?1 WHERE did = ?2",
            [handle, did],
        );
        set?;
        sequence(&tx, did, events)?;
        tx.commit()?;
        Ok(())
    }

    /// Switches `did` off, by `by` at `at`, `stewards` the ones who may
    /// switch it back on, with `events`: `false` if it is off already.
    pub fn deactivate(
        &self,
        did: &str,
        at: i64,
        by: &str,
        stewards: &[String],
        events: &[Event],
    ) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let listed = serde_json::to_string(stewards)?;
        if tx.execute(DEACTIVATE, params![did, at, by, listed])? == 0 {
            return Ok(false);
        }
        sequence(&tx, did, events)?;
        tx.commit()?;
        Ok(true)
    }

    /// Switches `did` back on, with `events`: `false` if it was on.
    pub fn activate(&self, did: &str, events: &[Event]) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if tx.execute(ACTIVATE, [did])? == 0 {
            return Ok(false);
        }
        sequence(&tx, did, events)?;
        tx.commit()?;
        Ok(true)
    }

    /// Whether `did` is switched off, and how.
    pub fn deactivation(&self, did: &str) -> Result<Option<Deactivation>> {
        let conn = self.conn();
        let found = conn.query_row(DEACTIVATION, [did], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, String>(2)?))
        });
        Ok(found.optional()?.map(|(at, by, stewards)| Deactivation {
            at,
            by,
            stewards: serde_json::from_str(&stewards).expect("stewards the host wrote"),
        }))
    }

    /// Forgets `did`: its account, its repos, its spaces and what the host
    /// keeps of them, with `events`. The blobs it had, for their files to
    /// go.
    pub fn delete_account(&self, did: &str, events: &[Event]) -> Result<Vec<Cid>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for sql in FORGET_ACCOUNT {
            let gone = tx.execute(sql, [did]);
            gone?;
        }
        let blobs = {
            let mut q = tx.prepare(FORGET_BLOBS)?;
            let rows = q.query_map([did], |r| Ok(cid(r.get(0)?)));
            rows?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let gone = tx.execute("DELETE FROM account WHERE did = ?1", [did]);
        gone?;
        sequence(&tx, did, events)?;
        tx.commit()?;
        Ok(blobs)
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
