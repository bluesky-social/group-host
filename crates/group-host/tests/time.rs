//! Times written as atproto writes them.

use group_host::time::{iso, now_millis};

#[test]
fn writes_known_instants() {
    assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(iso(951_782_400_000), "2000-02-29T00:00:00.000Z");
    assert_eq!(iso(1_793_740_439_195), "2026-11-03T21:13:59.195Z");
    assert_eq!(iso(-1), "1969-12-31T23:59:59.999Z");
    assert!(iso(now_millis()).starts_with("20"));
}
