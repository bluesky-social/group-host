//! The command as people run it.

use std::process::Command;

fn opensocial(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_opensocial"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn prints_usage() {
    for args in [&[][..], &["help"], &["--help"], &["-h"]] {
        let out = opensocial(args);
        assert!(out.status.success());
        assert_eq!(String::from_utf8(out.stdout).unwrap(), opensocial::USAGE);
    }
}

#[test]
fn refuses_an_unknown_command() {
    let out = opensocial(&["frobnicate"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("no command frobnicate"));
}
