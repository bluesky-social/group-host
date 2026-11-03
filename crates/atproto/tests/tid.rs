//! TIDs, made from the time and read back.

use atproto::syntax::is_tid;
use atproto::tid::{TidClock, micros, tid_from};

#[test]
fn hold_their_time() {
    let tid = tid_from(1_700_000_000_000_000, 7);
    assert!(is_tid(&tid), "{tid}");
    assert_eq!(micros(&tid), Some(1_700_000_000_000_000));
    assert_eq!(tid_from(0, 0), "2222222222222");
    assert_eq!(micros("not a tid"), None);
}

#[test]
fn sort_as_their_times() {
    let earlier = tid_from(1_700_000_000_000_000, 1023);
    let later = tid_from(1_700_000_000_000_001, 0);
    assert!(earlier < later);
}

#[test]
fn a_clock_only_goes_forward() {
    let clock = TidClock::new(3);
    let first = clock.next_at(1_000);
    let stepped_back = clock.next_at(500);
    assert!(stepped_back > first);
    assert_eq!(micros(&stepped_back), Some(1_001));
    let now = clock.next();
    assert!(is_tid(&now) && now > stepped_back);
}
