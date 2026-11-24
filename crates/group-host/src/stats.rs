//! How the host is used, day by day, for its operators
//! (`fyi.opensocial.internal.getStats`, `opensocial admin stats`).
//!
//! - Every request but the health checks is counted by the day (UTC), by
//!   the route that answered it (its pattern, not its path), its method
//!   and its status's class, with how long it took and whether that was
//!   slow ([`SLOW`]). A stream, which is open as long as its reader
//!   keeps it, is never slow.
//! - Each MCP tool called, by its name and whether it worked.
//! - What the host holds, read from its database when asked: its groups,
//!   those switched off, their spaces and their files, and the groups
//!   and spaces made each day.
//!
//! Nothing here names who asked. The counts are held in memory and
//! written now and then ([`Stats::flush`]) to `stats.sqlite` beside the
//! database, and kept [`DAYS_MAX`] days.

use crate::store::Store;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// A request that took this long or longer is slow.
pub const SLOW: Duration = Duration::from_secs(1);

/// The most days a report answers for, and how long a count is kept.
pub const DAYS_MAX: u32 = 400;

const DAY: i64 = 86_400_000;

/// The day `millis` falls on, as `2026-11-06`.
pub fn day_of(millis: i64) -> String {
    crate::time::iso(millis)[..10].to_string()
}

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
struct Key {
    day: String,
    route: String,
    method: String,
    /// The status's class: 2 for 2xx, and so on.
    class: u16,
}

#[derive(Default, Debug, PartialEq)]
struct Tally {
    n: i64,
    ms: i64,
    slow: i64,
}

/// The counts kept, and those counted since the last write.
pub struct Stats {
    db: Mutex<Connection>,
    requests: Mutex<HashMap<Key, Tally>>,
    /// Tool calls, by day, name and whether they worked.
    tools: Mutex<HashMap<(String, String, bool), i64>>,
}

impl Stats {
    pub fn open(path: &Path) -> rusqlite::Result<Stats> {
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        let made = db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS request (
               day TEXT NOT NULL, route TEXT NOT NULL, method TEXT NOT NULL,
               class INTEGER NOT NULL, n INTEGER NOT NULL, ms INTEGER NOT NULL,
               slow INTEGER NOT NULL,
               PRIMARY KEY (day, route, method, class)) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS tool (
               day TEXT NOT NULL, name TEXT NOT NULL, ok INTEGER NOT NULL,
               n INTEGER NOT NULL, PRIMARY KEY (day, name, ok)) WITHOUT ROWID;",
        );
        made?;
        Ok(Stats {
            db: Mutex::new(db),
            requests: Mutex::default(),
            tools: Mutex::default(),
        })
    }

    /// Counts a request `route` answered with `status`, which took
    /// `took`; a stream's (`streams`) never slow.
    pub fn served(&self, route: &str, method: &str, status: u16, took: Duration, streams: bool) {
        let key = Key {
            day: day_of(crate::time::now_millis()),
            route: route.to_string(),
            method: method.to_string(),
            class: status / 100,
        };
        let mut requests = self.requests.lock().expect("not poisoned");
        let tally = requests.entry(key).or_default();
        tally.n += 1;
        tally.ms += i64::try_from(took.as_millis()).unwrap_or(i64::MAX);
        tally.slow += i64::from(!streams && took >= SLOW);
    }

    /// Counts a call of the MCP tool `name`, and whether it worked.
    pub fn tool(&self, name: &str, ok: bool) {
        let key = (day_of(crate::time::now_millis()), name.to_string(), ok);
        *self
            .tools
            .lock()
            .expect("not poisoned")
            .entry(key)
            .or_default() += 1;
    }

    /// Writes what was counted since the last write, added to what is
    /// kept, and lets go of the days past [`DAYS_MAX`]. What a write
    /// fails to keep is counted again for the next.
    pub fn flush(&self) -> rusqlite::Result<()> {
        let requests = std::mem::take(&mut *self.requests.lock().expect("not poisoned"));
        let tools = std::mem::take(&mut *self.tools.lock().expect("not poisoned"));
        let written = self.write(&requests, &tools);
        if written.is_err() {
            let mut held = self.requests.lock().expect("not poisoned");
            for (key, t) in requests {
                let tally = held.entry(key).or_default();
                tally.n += t.n;
                tally.ms += t.ms;
                tally.slow += t.slow;
            }
            let mut held = self.tools.lock().expect("not poisoned");
            for (key, n) in tools {
                *held.entry(key).or_default() += n;
            }
        }
        written
    }

    fn write(
        &self,
        requests: &HashMap<Key, Tally>,
        tools: &HashMap<(String, String, bool), i64>,
    ) -> rusqlite::Result<()> {
        let mut db = self.db.lock().expect("not poisoned");
        let tx = db.transaction()?;
        let mut add = tx.prepare_cached(
            "INSERT INTO request VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT DO UPDATE SET n = n + ?5, ms = ms + ?6, slow = slow + ?7",
        )?;
        for (k, t) in requests {
            add.execute(params![
                k.day, k.route, k.method, k.class, t.n, t.ms, t.slow
            ])?;
        }
        let mut called = tx.prepare_cached(
            "INSERT INTO tool VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO UPDATE SET n = n + ?4",
        )?;
        for ((day, name, ok), n) in tools {
            called.execute(params![day, name, ok, n])?;
        }
        drop((add, called));
        let gone = day_of(crate::time::now_millis() - i64::from(DAYS_MAX) * DAY);
        tx.execute("DELETE FROM request WHERE day < ?1", [&gone])?;
        tx.execute("DELETE FROM tool WHERE day < ?1", [&gone])?;
        tx.commit()
    }

    /// How the host was used over the last `days` days to `now`, today
    /// the last, and what `store` holds.
    pub fn report(&self, store: &Store, days: u32, now: i64) -> anyhow::Result<Value> {
        let days = days.clamp(1, DAYS_MAX);
        let start = (now.div_euclid(DAY) - i64::from(days) + 1) * DAY;
        let first = day_of(start);
        // Bounded by today too: a count of a day past it, as a clock set
        // back leaves, is not one asked for.
        let last = day_of(now);
        let mut by_day: BTreeMap<String, Value> = (0..i64::from(days))
            .map(|i| {
                let day = day_of(start + i * DAY);
                let zero = json!({"day": day, "requests": 0, "refused": 0, "failed": 0,
                    "slow": 0, "toolCalls": 0, "groupsMade": 0, "spacesMade": 0});
                (day, zero)
            })
            .collect();
        let db = self.db.lock().expect("not poisoned");
        let mut q = db.prepare(
            "SELECT day, SUM(n), SUM(CASE WHEN class = 4 THEN n ELSE 0 END),
               SUM(CASE WHEN class = 5 THEN n ELSE 0 END), SUM(slow)
             FROM request WHERE day >= ?1 AND day <= ?2 GROUP BY day",
        )?;
        let rows = q.query_map([&first, &last], |r| {
            Ok((
                r.get::<_, String>(0)?,
                [r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)?],
            ))
        });
        let rows = rows?;
        for row in rows {
            let (day, [n, refused, failed, slow]) = row?;
            let d = by_day.get_mut(&day).expect("a day asked for");
            d["requests"] = json!(n);
            d["refused"] = json!(refused);
            d["failed"] = json!(failed);
            d["slow"] = json!(slow);
        }
        let mut q =
            db.prepare("SELECT day, SUM(n) FROM tool WHERE day >= ?1 AND day <= ?2 GROUP BY day")?;
        let rows = q.query_map([&first, &last], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        });
        let rows = rows?;
        for row in rows {
            let (day, n) = row?;
            by_day.get_mut(&day).expect("a day asked for")["toolCalls"] = json!(n);
        }
        let mut q = db.prepare(
            "SELECT route, method, SUM(n), SUM(CASE WHEN class = 4 THEN n ELSE 0 END),
               SUM(CASE WHEN class = 5 THEN n ELSE 0 END), SUM(ms), SUM(slow)
             FROM request WHERE day >= ?1 AND day <= ?2 GROUP BY route, method
             ORDER BY SUM(n) DESC, route, method",
        )?;
        let rows = q.query_map([&first, &last], |r| {
            let n: i64 = r.get(2)?;
            let ms: i64 = r.get(5)?;
            Ok(
                json!({"route": r.get::<_, String>(0)?, "method": r.get::<_, String>(1)?,
                "requests": n, "refused": r.get::<_, i64>(3)?, "failed": r.get::<_, i64>(4)?,
                "meanMs": ms / n, "slow": r.get::<_, i64>(6)?}),
            )
        });
        let rows = rows?;
        let routes = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut q = db.prepare(
            "SELECT name, SUM(n), SUM(CASE WHEN ok THEN 0 ELSE n END)
             FROM tool WHERE day >= ?1 AND day <= ?2 GROUP BY name ORDER BY SUM(n) DESC, name",
        )?;
        let rows = q.query_map([&first, &last], |r| {
            Ok(
                json!({"tool": r.get::<_, String>(0)?, "calls": r.get::<_, i64>(1)?,
                "failed": r.get::<_, i64>(2)?}),
            )
        });
        let rows = rows?;
        let tools = rows.collect::<rusqlite::Result<Vec<_>>>()?;

        let conn = store.conn();
        for (table, field) in [("account", "groupsMade"), ("space", "spacesMade")] {
            // Made at a time in seconds.
            let sql = format!("SELECT created_at FROM {table} WHERE created_at >= ?1");
            let mut q = conn.prepare(&sql)?;
            let made = q.query_map([start / 1000], |r| r.get::<_, i64>(0))?;
            for at in made {
                // One made after `now`, as by a clock set back, is today's.
                let day = day_of((at? * 1000).min(now));
                let d = by_day.get_mut(&day).expect("a day asked for");
                d[field] = json!(d[field].as_i64().unwrap_or_default() + 1);
            }
        }
        let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0));
        let held = json!({
            "groups": count("SELECT COUNT(*) FROM account")?,
            "deactivated": count("SELECT COUNT(*) FROM deactivation")?,
            "spaces": count("SELECT COUNT(*) FROM space")?,
            "blobBytes": count("SELECT COALESCE(SUM(size), 0) FROM blob WHERE temporary = 0")?,
        });
        Ok(json!({
            "host": held,
            "days": by_day.into_values().collect::<Vec<_>>(),
            "routes": routes,
            "tools": tools,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened() -> (tempfile::TempDir, Stats, Store) {
        let dir = tempfile::tempdir().unwrap();
        let stats = Stats::open(&dir.path().join("stats.sqlite")).unwrap();
        let store = Store::open(&dir.path().join("host.sqlite")).unwrap();
        (dir, stats, store)
    }

    #[test]
    fn counts_by_day_route_and_tool() {
        let (_dir, stats, store) = opened();
        let ms = Duration::from_millis;
        stats.served("/xrpc/a", "GET", 200, ms(10), false);
        stats.served("/xrpc/a", "GET", 200, ms(30), false);
        stats.served("/xrpc/a", "GET", 403, ms(2), false);
        stats.served("/xrpc/b", "POST", 500, ms(1500), false);
        stats.served("/stream", "GET", 101, ms(60_000), true);
        stats.tool("member_list", true);
        stats.tool("member_list", false);
        stats.tool("group_show", true);
        stats.flush().unwrap();
        // Counted again, added to what was kept.
        stats.served("/xrpc/a", "GET", 200, ms(0), false);
        stats.flush().unwrap();

        let now = crate::time::now_millis();
        let r = stats.report(&store, 2, now).unwrap();
        let days = r["days"].as_array().unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!(days[0]["day"], day_of(now - DAY));
        assert_eq!(days[0]["requests"], 0);
        let today = &days[1];
        assert_eq!(today["day"], day_of(now));
        assert_eq!(today["requests"], 6);
        assert_eq!(today["refused"], 1);
        assert_eq!(today["failed"], 1);
        assert_eq!(today["slow"], 1);
        assert_eq!(today["toolCalls"], 3);
        let a = &r["routes"][0];
        assert_eq!(a["route"], "/xrpc/a");
        assert_eq!(a["requests"], 4);
        assert_eq!(a["refused"], 1);
        assert_eq!(a["meanMs"], 10);
        let tools = &r["tools"];
        assert_eq!(
            tools[0],
            json!({"tool": "member_list", "calls": 2, "failed": 1})
        );
        assert_eq!(
            tools[1],
            json!({"tool": "group_show", "calls": 1, "failed": 0})
        );
        assert_eq!(
            r["host"],
            json!({"groups": 0, "deactivated": 0, "spaces": 0, "blobBytes": 0})
        );
        // At least a day, and at most DAYS_MAX.
        let days = |n| {
            stats.report(&store, n, now).unwrap()["days"]
                .as_array()
                .unwrap()
                .len()
        };
        assert_eq!(days(0), 1);
        assert_eq!(days(10_000), DAYS_MAX as usize);
    }

    /// A count of a day past today, as a clock set back leaves, is not
    /// one asked for.
    #[test]
    fn leaves_out_a_day_past_today() {
        let (_dir, stats, store) = opened();
        let now = crate::time::now_millis();
        let ahead = day_of(now + DAY);
        let db = stats.db.lock().unwrap();
        db.execute(
            "INSERT INTO request VALUES (?1, '/x', 'GET', 2, 1, 1, 0)",
            [&ahead],
        )
        .unwrap();
        db.execute("INSERT INTO tool VALUES (?1, 't', 1, 1)", [&ahead])
            .unwrap();
        drop(db);
        let r = stats.report(&store, 1, now).unwrap();
        assert_eq!(r["days"][0]["requests"], 0);
        assert_eq!(r["routes"], json!([]));
        assert_eq!(r["tools"], json!([]));
    }

    /// The days past keeping go at the next write.
    #[test]
    fn lets_old_days_go() {
        let (_dir, stats, store) = opened();
        let old = day_of(crate::time::now_millis() - i64::from(DAYS_MAX + 1) * DAY);
        let db = stats.db.lock().unwrap();
        db.execute(
            "INSERT INTO request VALUES (?1, '/x', 'GET', 2, 1, 1, 0)",
            [&old],
        )
        .unwrap();
        db.execute("INSERT INTO tool VALUES (?1, 't', 1, 1)", [&old])
            .unwrap();
        drop(db);
        stats.flush().unwrap();
        let db = stats.db.lock().unwrap();
        let left: i64 = db
            .query_row(
                "SELECT (SELECT COUNT(*) FROM request) + (SELECT COUNT(*) FROM tool)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0);
        drop(db);
        drop(store);
    }

    /// A group moved here before its time was kept in seconds counts on
    /// the day it was moved, as one made here does.
    #[test]
    fn counts_a_group_moved_here_on_its_day() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host.sqlite");
        let migrations = crate::store::MIGRATIONS;
        let before = migrations.len() - 1;
        let conn = Connection::open(&path).unwrap();
        for m in &migrations[..before] {
            conn.execute_batch(m).unwrap();
        }
        conn.pragma_update(None, "user_version", before as i64)
            .unwrap();
        let now = crate::time::now_millis();
        conn.execute(
            "INSERT INTO account VALUES ('did:plc:moved', 'moved.test', x'', x'', ?1)",
            [now],
        )
        .unwrap();
        drop(conn);
        let store = Store::open(&path).unwrap();
        let stats = Stats::open(&dir.path().join("stats.sqlite")).unwrap();
        let r = stats.report(&store, 1, now).unwrap();
        assert_eq!(r["days"][0]["groupsMade"], 1);
    }

    /// What a write fails to keep is counted again for the next.
    #[test]
    fn a_failed_write_keeps_its_counts() {
        let (dir, stats, store) = opened();
        let other = Connection::open(dir.path().join("stats.sqlite")).unwrap();
        stats.served("/xrpc/a", "GET", 200, Duration::ZERO, false);
        stats.tool("t", true);
        other
            .execute_batch("ALTER TABLE tool RENAME TO away")
            .unwrap();
        assert!(stats.flush().is_err());
        stats.served("/xrpc/a", "GET", 200, Duration::ZERO, false);
        stats.tool("t", true);
        other
            .execute_batch("ALTER TABLE away RENAME TO tool")
            .unwrap();
        stats.flush().unwrap();
        let r = stats.report(&store, 1, crate::time::now_millis()).unwrap();
        assert_eq!(r["days"][0]["requests"], 2);
        assert_eq!(r["days"][0]["toolCalls"], 2);
    }
}
