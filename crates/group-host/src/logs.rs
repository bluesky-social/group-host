//! The lines the host logs, kept to read back for its operators
//! (`fyi.opensocial.getLogs`, `opensocial admin logs`).
//!
//! Each JSON line the host writes to stdout ([`Tee`]) is kept here too,
//! in `logs.sqlite` beside its database, written a second at a time off
//! the requests' way. What is kept is what stdout has, and for no
//! longer: lines at info and below only the last `LOGS_KEEP`, the most
//! numerous; any line only [`DAYS`] days.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

/// How many lines at info and below are kept, unless `LOGS_KEEP` says.
pub const KEEP: u64 = 500_000;

/// How many days any line is kept.
pub const DAYS: i64 = 30;

/// The most lines held between writes: a write that cannot keep up lets
/// the oldest go rather than grow without end.
const PENDING_MAX: usize = 50_000;

/// The most lines a read answers.
pub const LIMIT_MAX: u32 = 500;

/// The lines kept, and those logged since the last write.
pub struct Logs {
    db: Mutex<Connection>,
    /// Each line held, with when it was logged.
    pending: Mutex<VecDeque<(i64, Vec<u8>)>>,
    keep: i64,
}

impl Logs {
    /// The lines kept at `path`, keeping `keep` at info and below.
    pub fn open(path: &Path, keep: u64) -> rusqlite::Result<Logs> {
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        let made = db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS line (
               id INTEGER PRIMARY KEY, at INTEGER NOT NULL, level INTEGER NOT NULL,
               message TEXT NOT NULL, trace TEXT, ms INTEGER, line TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS line_trace ON line (trace) WHERE trace IS NOT NULL;
             CREATE INDEX IF NOT EXISTS line_level ON line (level, id);
             CREATE INDEX IF NOT EXISTS line_message ON line (message, id);
             CREATE INDEX IF NOT EXISTS line_at ON line (at);",
        );
        made?;
        Ok(Logs {
            db: Mutex::new(db),
            pending: Mutex::default(),
            keep: i64::try_from(keep).unwrap_or(i64::MAX),
        })
    }

    /// Holds `line`, one JSON line as logged, for the next write.
    pub fn keep(&self, line: &[u8]) {
        let mut pending = self.pending.lock().expect("not poisoned");
        if pending.len() >= PENDING_MAX {
            pending.pop_front();
        }
        pending.push_back((crate::time::now_millis(), line.to_vec()));
    }

    /// Writes the lines held, each with its level, message, trace and
    /// duration beside it to find it by. A line that is not JSON is kept
    /// as a message of its own. Lines a write fails to keep are held
    /// again, ahead of those logged since, for the next.
    pub fn flush(&self) -> rusqlite::Result<()> {
        let batch = std::mem::take(&mut *self.pending.lock().expect("not poisoned"));
        let written = self.write(&batch);
        if written.is_err() {
            let mut pending = self.pending.lock().expect("not poisoned");
            let since = std::mem::replace(&mut *pending, batch);
            pending.extend(since);
            let over = pending.len().saturating_sub(PENDING_MAX);
            pending.drain(..over);
        }
        written
    }

    /// Writes `pending` in one transaction: all of it or none.
    fn write(&self, pending: &VecDeque<(i64, Vec<u8>)>) -> rusqlite::Result<()> {
        let mut db = self.db.lock().expect("not poisoned");
        let tx = db.transaction()?;
        let mut stmt =
            tx.prepare_cached("INSERT INTO line VALUES (NULL, ?1, ?2, ?3, ?4, ?5, ?6)")?;
        for (at, line) in pending {
            let text = String::from_utf8_lossy(line);
            let text = text.trim();
            let v: Value =
                serde_json::from_str(text).unwrap_or_else(|_| json!({"fields": {"message": text}}));
            let fields = &v["fields"];
            let level = rank(v["level"].as_str().unwrap_or("INFO"));
            let message = fields["message"].as_str().unwrap_or_default();
            stmt.execute(params![
                at,
                level,
                message,
                trace_of(&v),
                fields["duration_ms"].as_i64(),
                v.to_string()
            ])?;
        }
        drop(stmt);
        tx.commit()
    }

    /// Lets go of what is past keeping: lines at info and below past the
    /// last `keep` of them, and any line older than [`DAYS`].
    pub fn prune(&self) -> rusqlite::Result<()> {
        let db = self.db.lock().expect("not poisoned");
        let since = crate::time::now_millis() - DAYS * 86_400_000;
        db.execute("DELETE FROM line WHERE at < ?1", [since])?;
        let last: Option<i64> = db
            .query_row(
                "SELECT id FROM line WHERE level <= ?1 ORDER BY id DESC LIMIT 1 OFFSET ?2",
                params![rank("INFO"), self.keep],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(last) = last {
            db.execute(
                "DELETE FROM line WHERE level <= ?1 AND id <= ?2",
                params![rank("INFO"), last],
            )?;
        }
        Ok(())
    }

    /// The lines `asked` names, oldest first: all of one request's, by
    /// its trace, up to [`LIMIT_MAX`] unless asked for fewer; or the last
    /// 100 (or `limit`) of a message, a level or a duration.
    pub fn read(&self, asked: &Asked) -> rusqlite::Result<Vec<Value>> {
        let unasked = if asked.trace.is_some() {
            LIMIT_MAX
        } else {
            100
        };
        let limit = asked.limit.unwrap_or(unasked).clamp(1, LIMIT_MAX);
        let level = asked.level.as_deref().map_or(0, rank);
        let db = self.db.lock().expect("not poisoned");
        // A statement for each way of asking, each by its own index: one
        // asking `?2 IS NULL OR message = ?2` could use none.
        let sql = match (&asked.trace, &asked.message) {
            (Some(_), _) => {
                "SELECT line FROM line WHERE trace = ?1 AND (?2 IS NULL OR message = ?2)
                 AND level >= ?3 AND (?4 IS NULL OR ms >= ?4) ORDER BY id DESC LIMIT ?5"
            }
            (None, Some(_)) => {
                "SELECT line FROM line INDEXED BY line_message
                 WHERE message = ?2 AND level >= ?3 AND (?4 IS NULL OR ms >= ?4)
                 ORDER BY id DESC LIMIT ?5"
            }
            (None, None) => {
                "SELECT line FROM line WHERE level >= ?3 AND (?4 IS NULL OR ms >= ?4)
                 ORDER BY id DESC LIMIT ?5"
            }
        };
        let mut stmt = db.prepare_cached(sql)?;
        let p = params![asked.trace, asked.message, level, asked.min_ms, limit];
        let rows = stmt.query_map(p, |r| r.get::<_, String>(0))?;
        let mut lines: Vec<Value> = rows
            .map(|r| r.map(|text| serde_json::from_str(&text).unwrap_or_default()))
            .collect::<rusqlite::Result<_>>()?;
        lines.reverse();
        Ok(lines)
    }
}

/// The trace a line was logged in: its request's span's, the outermost
/// that names one.
fn trace_of(line: &Value) -> Option<&str> {
    let spans = line["spans"].as_array().into_iter().flatten();
    spans.filter_map(|s| s["trace"].as_str()).next()
}

/// A level, as a number to compare: `ERROR` the highest.
fn rank(level: &str) -> i64 {
    match level.to_ascii_uppercase().as_str() {
        "ERROR" => 5,
        "WARN" => 4,
        "INFO" => 3,
        "DEBUG" => 2,
        _ => 1,
    }
}

/// What a read asks for; nothing asks for the last lines of any kind.
#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
    /// All of one request's lines.
    pub trace: Option<String>,
    /// Only lines with this message: `request`, `mcp tool`.
    pub message: Option<String>,
    /// Only lines at this level or above.
    pub level: Option<String>,
    /// Only lines that took this many milliseconds or more.
    pub min_ms: Option<i64>,
    /// How many, the last of them.
    pub limit: Option<u32>,
}

impl Asked {
    /// What it asks, as the line telling of the read says it.
    pub fn said(&self) -> String {
        let parts = [
            self.trace.as_ref().map(|t| format!("trace={t}")),
            self.message.as_ref().map(|m| format!("message={m}")),
            self.level.as_ref().map(|l| format!("level={l}")),
            self.min_ms.map(|ms| format!("minMs={ms}")),
            self.limit.map(|n| format!("limit={n}")),
        ];
        parts.into_iter().flatten().collect::<Vec<_>>().join(" ")
    }
}

/// The lines the [`Tee`] keeps: the host's, once it [`sink`]s them.
static SINK: RwLock<Weak<Logs>> = RwLock::new(Weak::new());

/// Has the lines logged from now kept in `logs`.
pub fn sink(logs: &Arc<Logs>) {
    *SINK.write().expect("not poisoned") = Arc::downgrade(logs);
}

/// Where the host's log lines go: stdout, as ever, and the [`sink`],
/// once named. A line is written to it whole when its writer is done.
#[derive(Default)]
pub struct Tee(Vec<u8>);

impl Write for Tee {
    /// Keeps `buf` whatever stdout does: a line is kept even when no
    /// one reads stdout any more.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        let _ = std::io::stdout().write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stdout().flush();
        Ok(())
    }
}

impl Drop for Tee {
    fn drop(&mut self) {
        let logs = SINK.read().expect("not poisoned").upgrade();
        if let Some(logs) = logs.filter(|_| !self.0.is_empty()) {
            logs.keep(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(level: &str, message: &str, trace: Option<&str>, ms: Option<i64>) -> Vec<u8> {
        let v = json!({"level": level, "fields": {"message": message, "duration_ms": ms},
            "spans": [{"name": "request", "trace": trace}]});
        format!("{v}\n").into_bytes()
    }

    fn messages(lines: &[Value]) -> Vec<String> {
        let message = |l: &Value| l["fields"]["message"].as_str().unwrap_or("?").to_string();
        lines.iter().map(message).collect()
    }

    #[test]
    fn keeps_reads_and_lets_go() {
        let dir = tempfile::tempdir().unwrap();
        let logs = Logs::open(&dir.path().join("logs.sqlite"), 3).unwrap();
        logs.keep(&line("WARN", "long ago", Some("t0"), Some(5000)));
        logs.flush().unwrap();
        logs.db
            .lock()
            .unwrap()
            .execute("UPDATE line SET at = 0", [])
            .unwrap();
        for i in 0..5i64 {
            logs.keep(&line("INFO", "request", Some("t1"), Some(i * 100)));
        }
        logs.keep(&line("WARN", "sql", Some("t1"), Some(400)));
        logs.keep(&line("DEBUG", "outgoing", Some("t1"), Some(20)));
        logs.keep(b"not json\n");
        logs.flush().unwrap();
        // A write with nothing held writes nothing.
        logs.flush().unwrap();
        let read = |asked: Asked| logs.read(&asked).unwrap();
        let trace = read(Asked {
            trace: Some("t1".into()),
            ..Asked::default()
        });
        assert_eq!(trace.len(), 7);
        assert_eq!(messages(&trace)[5..], ["sql", "outgoing"]);
        let in_trace = read(Asked {
            trace: Some("t1".into()),
            message: Some("sql".into()),
            ..Asked::default()
        });
        assert_eq!(messages(&in_trace), ["sql"]);
        let slow = read(Asked {
            min_ms: Some(400),
            ..Asked::default()
        });
        assert_eq!(messages(&slow), ["long ago", "request", "sql"]);
        let warned = read(Asked {
            level: Some("warn".into()),
            limit: Some(1),
            ..Asked::default()
        });
        assert_eq!(messages(&warned), ["sql"]);
        let plain = read(Asked {
            message: Some("not json".into()),
            ..Asked::default()
        });
        assert_eq!(messages(&plain), ["not json"]);

        // Past keeping: the line of long ago, and all but the last three at
        // info and below; what warned stays.
        logs.prune().unwrap();
        let left = read(Asked::default());
        assert_eq!(messages(&left), ["request", "sql", "outgoing", "not json"]);
        // With nothing past keeping, nothing goes.
        logs.prune().unwrap();
        assert_eq!(read(Asked::default()).len(), 4);
    }

    /// A write that fails keeps what it held, ahead of what was logged
    /// since, for the next; and still holds only so many.
    #[test]
    fn a_failed_write_keeps_its_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logs.sqlite");
        let logs = Logs::open(&path, KEEP).unwrap();
        let other = Connection::open(&path).unwrap();
        logs.keep(&line("INFO", "first", Some("t"), None));
        other
            .execute_batch("ALTER TABLE line RENAME TO away")
            .unwrap();
        assert!(logs.flush().is_err());
        logs.keep(&line("INFO", "second", Some("t"), None));
        other
            .execute_batch("ALTER TABLE away RENAME TO line")
            .unwrap();
        logs.flush().unwrap();
        let trace = logs
            .read(&Asked {
                trace: Some("t".into()),
                ..Asked::default()
            })
            .unwrap();
        assert_eq!(messages(&trace), ["first", "second"]);

        other
            .execute_batch("ALTER TABLE line RENAME TO away")
            .unwrap();
        for _ in 0..PENDING_MAX {
            logs.keep(b"{}");
        }
        assert!(logs.flush().is_err());
        logs.keep(b"{}");
        assert_eq!(logs.pending.lock().unwrap().len(), PENDING_MAX);
    }

    /// A trace reads up to the most a read answers unless asked for
    /// fewer; anything else, the last 100.
    #[test]
    fn a_trace_reads_whole() {
        let dir = tempfile::tempdir().unwrap();
        let logs = Logs::open(&dir.path().join("logs.sqlite"), KEEP).unwrap();
        for _ in 0..150 {
            logs.keep(&line("INFO", "request", Some("t"), None));
        }
        logs.flush().unwrap();
        let read = |trace: Option<&str>, limit| {
            let asked = Asked {
                trace: trace.map(str::to_string),
                limit,
                ..Asked::default()
            };
            logs.read(&asked).unwrap().len()
        };
        assert_eq!(read(Some("t"), None), 150);
        assert_eq!(read(Some("t"), Some(10)), 10);
        assert_eq!(read(None, None), 100);
    }

    #[test]
    fn ranks_and_says() {
        let ranks: Vec<i64> = ["ERROR", "warn", "INFO", "DEBUG", "TRACE"]
            .into_iter()
            .map(rank)
            .collect();
        assert_eq!(ranks, [5, 4, 3, 2, 1]);
        let asked = Asked {
            trace: Some("t".into()),
            message: Some("request".into()),
            level: Some("warn".into()),
            min_ms: Some(100),
            limit: Some(5),
        };
        assert_eq!(
            asked.said(),
            "trace=t message=request level=warn minMs=100 limit=5"
        );
        assert_eq!(Asked::default().said(), "");
    }

    /// The tee writes to stdout, and keeps each line whole once the sink
    /// is named; an empty writer keeps nothing.
    #[test]
    fn tees_each_line_to_the_sink() {
        let dir = tempfile::tempdir().unwrap();
        let logs = Arc::new(Logs::open(&dir.path().join("logs.sqlite"), KEEP).unwrap());
        sink(&logs);
        let mut tee = Tee::default();
        tee.write_all(br#"{"level":"INFO","fields":{"message":"#)
            .unwrap();
        tee.write_all(b"\"teed\"}}\n").unwrap();
        tee.flush().unwrap();
        drop(tee);
        drop(Tee::default());
        logs.flush().unwrap();
        let asked = Asked {
            message: Some("teed".into()),
            ..Asked::default()
        };
        assert_eq!(logs.read(&asked).unwrap().len(), 1);
        assert_eq!(logs.read(&Asked::default()).unwrap().len(), 1);
    }
}
