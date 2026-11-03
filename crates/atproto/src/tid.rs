//! TIDs: the record keys and revisions atproto makes from the time, 13
//! characters of sortable base32 holding the microseconds since 1970 and a
//! clock id, so that a later one sorts after.

use crate::base32::SORTABLE;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The TID of `micros` since 1970 from clock `clock` (its low 10 bits).
pub fn tid_from(micros: u64, clock: u64) -> String {
    // 63 bits, the top one clear, five to a character from the last.
    let mut v = ((micros & ((1 << 53) - 1)) << 10) | (clock & 1023);
    let mut out = [0; 13];
    for c in out.iter_mut().rev() {
        *c = SORTABLE[(v & 31) as usize];
        v >>= 5;
    }
    String::from_utf8(out.to_vec()).expect("ascii")
}

/// The microseconds a TID holds, or `None` for what is not one.
pub fn micros(tid: &str) -> Option<u64> {
    if !crate::syntax::is_tid(tid) {
        return None;
    }
    let value = tid.bytes().fold(0u64, |v, c| {
        (v << 5) | SORTABLE.iter().position(|&a| a == c).expect("checked") as u64
    });
    Some(value >> 10)
}

/// Makes TIDs that only go forward, each after the last this clock made,
/// even when the system's clock steps back.
pub struct TidClock {
    clock: u64,
    last: Mutex<u64>,
}

impl TidClock {
    /// A clock whose TIDs carry `clock` as their clock id.
    pub fn new(clock: u64) -> TidClock {
        TidClock {
            clock,
            last: Mutex::new(0),
        }
    }

    /// The next TID, at now or just after the last one made.
    pub fn next(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after 1970")
            .as_micros() as u64;
        self.next_at(now)
    }

    /// The next TID, as [`TidClock::next`], at `now` microseconds.
    pub fn next_at(&self, now: u64) -> String {
        let mut last = self.last.lock().expect("not poisoned");
        *last = now.max(*last + 1);
        tid_from(*last, self.clock)
    }
}
